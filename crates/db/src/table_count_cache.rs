//! 表行数缓存
//!
//! 表格翻页时每次都会重跑 `COUNT(*)`，大表上这是主要耗时来源（百万行级别的
//! 全表统计往往比取一页数据还慢）。这里按「连接 + 表 + WHERE 指纹」复用计数结果，
//! 写操作发生后由调用方整体清空，避免长期显示过期行数。

use std::collections::HashMap;
use std::sync::Mutex;

use one_core::storage::DbConnectionConfig;

/// 缓存条目上限。
///
/// 键里带上了连接与 WHERE 条件，用户在不同筛选条件下反复翻页会让条目数增长，
/// 这里给一个上限，超限时整体清空——统计结果重新算一次的成本远低于
/// 无限增长的内存占用。
const MAX_ENTRIES: usize = 512;

/// 表行数缓存。
#[derive(Default)]
pub struct TableCountCache {
    entries: Mutex<HashMap<String, usize>>,
}

impl TableCountCache {
    /// 读取缓存的计数。
    pub fn get(&self, key: &str) -> Option<usize> {
        match self.entries.lock() {
            Ok(entries) => entries.get(key).copied(),
            Err(poisoned) => {
                tracing::error!("[TableCountCache] 缓存锁中毒，忽略本次读取");
                poisoned.into_inner().get(key).copied()
            }
        }
    }

    /// 写入计数。
    pub fn put(&self, key: String, count: usize) {
        match self.entries.lock() {
            Ok(mut entries) => {
                if entries.len() >= MAX_ENTRIES {
                    entries.clear();
                }
                entries.insert(key, count);
            }
            Err(poisoned) => {
                tracing::error!("[TableCountCache] 缓存锁中毒，忽略本次写入");
                let mut entries = poisoned.into_inner();
                if entries.len() >= MAX_ENTRIES {
                    entries.clear();
                }
                entries.insert(key, count);
            }
        }
    }

    /// 清空缓存（写操作后调用）。
    pub fn clear(&self) {
        match self.entries.lock() {
            Ok(mut entries) => entries.clear(),
            Err(poisoned) => {
                tracing::error!("[TableCountCache] 缓存锁中毒，忽略本次清空");
                poisoned.into_inner().clear();
            }
        }
    }
}

/// 构造表行数缓存的键。
///
/// 键里必须带上连接身份（类型、主机、端口、库名、用户），否则不同连接下的同名表
/// 会互相串号；WHERE 条件也参与键计算，避免不同筛选条件复用同一个计数。
/// 各字段用 U+0001 分隔，避免拼接歧义。
///
/// 键只用于进程内 `HashMap` 索引，不落盘、不进日志、不跨进程传递。
pub fn table_count_cache_key(
    config: &DbConnectionConfig,
    database: &str,
    schema: Option<&str>,
    table: &str,
    where_clause: Option<&str>,
) -> String {
    format!(
        "{}\u{1}{}:{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}",
        config.database_type.as_str(),
        config.host,
        config.port,
        config.database.as_deref().unwrap_or_default(),
        config.username,
        database,
        schema.unwrap_or_default(),
        table,
        where_clause.unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::{TableCountCache, table_count_cache_key};
    use one_core::storage::{DatabaseType, DbConnectionConfig};

    fn config(host: &str, username: &str) -> DbConnectionConfig {
        DbConnectionConfig {
            id: "1".to_string(),
            database_type: DatabaseType::PostgreSQL,
            name: "测试".to_string(),
            host: host.to_string(),
            port: 5432,
            username: username.to_string(),
            password: String::new(),
            database: Some("app".to_string()),
            workspace_id: None,
            extra_params: Default::default(),
            service_name: None,
            sid: None,
            credential_ref: None,
            ssh_tunnel_credential_ref: None,
        }
    }

    #[test]
    fn table_count_cache_读写与清空() {
        let cache = TableCountCache::default();
        assert_eq!(cache.get("k"), None);

        cache.put("k".to_string(), 42);
        assert_eq!(cache.get("k"), Some(42));

        cache.clear();
        assert_eq!(cache.get("k"), None);
    }

    #[test]
    fn table_count_cache_超过上限后整体清空() {
        let cache = TableCountCache::default();
        cache.put("k".to_string(), 42);

        for index in 0..super::MAX_ENTRIES {
            cache.put(format!("k-{index}"), index);
        }

        // 超过上限触发整体清空，最早写入的条目不再存在
        assert_eq!(cache.get("k"), None);
    }

    #[test]
    fn table_count_cache_key_区分连接与筛选条件() {
        let base = config("db-a", "alice");

        let key = table_count_cache_key(&base, "app", Some("public"), "users", Some("id > 0"));
        // 同参数得到同一个键
        assert_eq!(
            key,
            table_count_cache_key(&base, "app", Some("public"), "users", Some("id > 0"))
        );
        // 不同连接身份/表/筛选条件得到不同的键
        assert_ne!(
            key,
            table_count_cache_key(
                &config("db-b", "alice"),
                "app",
                Some("public"),
                "users",
                Some("id > 0")
            )
        );
        assert_ne!(
            key,
            table_count_cache_key(&base, "app", Some("public"), "orders", Some("id > 0"))
        );
        assert_ne!(
            key,
            table_count_cache_key(&base, "app", Some("public"), "users", Some("id > 1"))
        );
        assert_ne!(
            key,
            table_count_cache_key(&base, "app", None, "users", Some("id > 0"))
        );
    }
}

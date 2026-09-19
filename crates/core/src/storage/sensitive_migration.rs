//! A1：启动期迁移函数 —— 把存量数据库里以明文形态存在的 SSH 私钥等敏感字段
//! 重加密为 `ENC:V3:` 密文。
//!
//! 历史背景：仓库在 2026 年 9 月前，`connections.params` 与 `certificates.params`
//! 里的 `ssh_private_key` 字段以明文落库（H-1 高危）。`is_sensitive_field`
//! 只覆盖 `password` / `passphrase`，未覆盖 SSH 私钥，导致任何能读 SQLite
//! 文件的攻击者都能直接拿到用户 SSH 私钥。
//!
//! 修复路径：
//! 1. 扩展 `models::is_sensitive_field` 让 SSH 私钥字段也走加密管线
//! 2. 本模块的 `migrate_encrypt_existing_sensitive_fields` 在用户解锁主密钥后
//!    一次性扫两表，把"非 ENC: 开头"的 ssh_private_key 字段就地重加密
//! 3. 通过 `_migrations` 版本号 + 函数自身幂等检查保证多次启动不破坏数据
//!
//! 安全约束：
//! - 主密钥未解锁时调用方应**主动跳过**而不是抛错（用户可能在未设置密码的旧设备）
//! - 失败回滚：使用 SQLite 单事务，失败时全部回滚
//! - 幂等：`crypto::is_encrypted` 已加密的字段不会被二次加密

use anyhow::Result;
use rusqlite::Connection;
use rust_i18n::t;
use serde_json::Value;
use tracing::warn;

use super::manager::StorageManager;

/// 迁移返回值：(证书表行数处理结果, 连接表行数处理结果)，便于日志观测。
pub type MigrationCounts = (usize, usize);

/// 启动期执行一次敏感字段加密迁移。
///
/// # 调用时机（当前方案 A：未接入）
///
/// 本函数**当前不接入任何调用链**。这是方案 A 兼容性策略的一部分：
/// - 新写入路径已自动加密（`is_sensitive_field` 扩展后所有新 `ssh_private_key`
///   字段都会以 ENC:V3 密文落库）
/// - 存量明文保持明文状态，等待所有客户端升级到 ≥ 7b36af88（V3 派生槽引入）后
///   再启用本迁移函数
///
/// 启用窗口期建议：所有客户端升级完成后（约 1-2 个月），
/// 把本函数调用接入 `verify_and_set_master_key` 之后的回调里即可。
/// 函数本身已实现、已测试、可幂等重复调用——只需补一个接入点。
///
/// # 函数行为
///
/// - 主密钥未解锁时跳过整个流程（旧设备用户可能没有设密码），返回 `(0, 0)`
/// - 单事务扫两表（`certificates` / `connections`），对每行 params 调用
///   `encrypt_json_passwords` 重加密（含 `ssh_private_key` 等新增字段）
/// - 失败回滚事务并返回错误
///
/// # 幂等
///
/// 函数重复调用不会破坏已加密数据；`_migrations` 由调用方通过
/// `migration::run_migrations` 注册版本号跟踪（当前未注册，待启用时同步加）。
///
/// # 兼容性详细说明
///
/// 详见 `docs/plans/2026-09-18-fix-round-01.md` 的"兼容性策略：方案 A"章节。
pub fn migrate_encrypt_existing_sensitive_fields(
    storage: &StorageManager,
) -> Result<MigrationCounts> {
    // 主密钥未解锁时跳过整个流程（旧设备用户可能没有设密码）。
    if !crate::crypto::has_master_key() {
        warn!("{}", t!("SensitiveMigration.master_key_locked_skip"));
        return Ok((0, 0));
    }

    storage.connection().with_connection_mut(|conn| {
        let cert_count = migrate_table(conn, "certificates", "params")?;
        let conn_count = migrate_table(conn, "connections", "params")?;
        Ok((cert_count, conn_count))
    })
}

/// 单表迁移：对指定表中每行的 `params_col` JSON 字符串调用
/// `encrypt_json_passwords` 重加密（新增的 `ssh_private_key` 等字段会自动被覆盖），
/// 然后 UPDATE 写回。所有 UPDATE 在同一事务中，失败全部回滚。
fn migrate_table(conn: &mut Connection, table: &str, params_col: &str) -> Result<usize> {
    // 防御：表名/列名由调用方硬编码，不接受外部输入，无须转义。
    let sql_select = format!(
        "SELECT id, {params_col} FROM {table}",
        params_col = params_col
    );

    // 先把所有行 materialize 进 Vec，drop stmt 后再开事务（避开 borrow 冲突）。
    let rows: Vec<(i64, String)> = {
        let mut stmt = conn.prepare(&sql_select)?;
        let mapped = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let params: String = row.get(1)?;
            Ok((id, params))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };

    // 收集 (id, new_params) 待更新项
    let mut updates: Vec<(i64, String)> = Vec::new();
    for (id, params) in rows {
        if let Some(new_params) = reencrypt_if_needed(&params) {
            updates.push((id, new_params));
        }
    }

    if updates.is_empty() {
        return Ok(0);
    }

    let tx = conn.transaction()?;
    let sql_update = format!("UPDATE {table} SET {params_col} = ?1 WHERE id = ?2");
    {
        let mut upd = tx.prepare(&sql_update)?;
        for (id, new_params) in &updates {
            upd.execute(rusqlite::params![new_params, id])?;
        }
    }
    tx.commit()?;

    Ok(updates.len())
}

/// 给一段 params JSON 字符串做重加密判断：
/// - 解析失败 → 原样返回 None（不动它）
/// - 解析成功 → 走 `encrypt_json_passwords` 重加密
/// - 重加密前后一致（已全部是密文或不含敏感字段）→ 返回 None
/// - 否则返回 Some(新字符串)
fn reencrypt_if_needed(params: &str) -> Option<String> {
    let before: Value = serde_json::from_str(params).ok()?;
    let after_str = crate::storage::models::encrypt_json_passwords(params);
    let after: Value = serde_json::from_str(&after_str).ok()?;
    if before == after {
        None
    } else {
        Some(after_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::lock_for_test;
    use crate::storage::manager::StorageManager;
    use std::sync::{Mutex, OnceLock};

    /// 隔离测试 HOME 目录，避免覆盖真实用户数据库。
    /// 每次调用都用 atomic 计数器 + 进程 ID 生成完全独立的目录，
    /// 防止两个测试在同一进程内共用同一文件触发 panic。
    fn tmp_storage() -> StorageManager {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|t| t.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "omnihub-sens-mig-{}-{}-{}",
            std::process::id(),
            unique,
            n
        ));
        std::fs::create_dir_all(&dir).expect("建临时目录");
        let db_path = dir.join("omnihub.db");
        StorageManager::with_path(&db_path).expect("建 StorageManager")
    }

    /// 防止多个测试并发跑时互相影响临时目录
    static DIR_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    fn dir_lock() -> std::sync::MutexGuard<'static, ()> {
        match DIR_LOCK.get_or_init(|| Mutex::new(())).lock() {
            Ok(g) => g,
            // Poisoned 锁：上一个测试 panic 了；让本测试继续（不污染全局）
            Err(p) => p.into_inner(),
        }
    }

    /// `StorageManager::with_path` 已经运行过 migrations，所以 `certificates` /
    /// `connections` 表已存在；本函数仅负责插入测试数据。
    /// `name` / `connection_type` / `created_at` / `updated_at` 都是 NOT NULL。
    fn insert_test_rows(conn: &Connection) {
        let now = crate::storage::now() as i64;
        conn.execute(
            "INSERT INTO connections (name, connection_type, params, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            rusqlite::params![
                "test-conn-0",
                "ssh",
                r#"{"host":"1.2.3.4","ssh_private_key":"PLAIN_KEY"}"#,
                now,
            ],
        )
        .expect("insert conn 0");
        conn.execute(
            "INSERT INTO connections (name, connection_type, params, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            rusqlite::params!["test-conn-1", "ssh", r#"{"host":"2.3.4.5"}"#, now,],
        )
        .expect("insert conn 1");
    }

    fn insert_test_row_plain(conn: &Connection) {
        let now = crate::storage::now() as i64;
        conn.execute(
            "INSERT INTO connections (name, connection_type, params, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            rusqlite::params![
                "test-conn-plain",
                "ssh",
                r#"{"ssh_private_key":"PLAIN_KEY"}"#,
                now,
            ],
        )
        .expect("insert");
    }

    #[test]
    fn migrate_encrypts_plain_ssh_private_key() {
        let _lock = dir_lock();
        let _guard = lock_for_test();
        crate::crypto::test_support::set_master_key("migrate_test_master_key");

        let storage = tmp_storage();
        storage
            .connection()
            .with_connection_mut(|conn| {
                insert_test_rows(conn);
                Ok(())
            })
            .unwrap();

        // 第一次迁移：应重加密第一行
        let (cert_n, conn_n) = migrate_encrypt_existing_sensitive_fields(&storage).unwrap();
        assert_eq!(cert_n, 0, "无证书行");
        assert_eq!(conn_n, 1, "应重加密 1 行连接");

        // 校验：第一行字段已加密，第二行未被改动
        let rows: Vec<(i64, String)> = storage
            .connection()
            .with_connection(|conn| {
                let mut stmt = conn.prepare("SELECT id, params FROM connections ORDER BY id")?;
                let rows: Vec<(i64, String)> = stmt
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<rusqlite::Result<_>>()?;
                Ok(rows)
            })
            .unwrap();
        let row0: Value = serde_json::from_str(&rows[0].1).unwrap();
        assert!(
            crate::crypto::is_encrypted(row0["ssh_private_key"].as_str().unwrap_or("")),
            "row 0.ssh_private_key 应已加密"
        );
        assert_eq!(
            rows[1].1, r#"{"host":"2.3.4.5"}"#,
            "row 1 不含敏感字段，应保持原样"
        );

        // 第二次迁移：幂等，应 0 行处理（无变更）
        let (cert_n, conn_n) = migrate_encrypt_existing_sensitive_fields(&storage).unwrap();
        assert_eq!(cert_n, 0);
        assert_eq!(conn_n, 0, "二次迁移无变更");

        crate::crypto::clear_master_key();
    }

    #[test]
    fn migrate_no_op_when_master_key_not_unlocked() {
        let _lock = dir_lock();
        // 不调 set_master_key —— 主密钥槽保持空
        crate::crypto::clear_master_key();

        let storage = tmp_storage();
        storage
            .connection()
            .with_connection_mut(|conn| {
                insert_test_row_plain(conn);
                Ok(())
            })
            .unwrap();

        let (cert_n, conn_n) = migrate_encrypt_existing_sensitive_fields(&storage).unwrap();
        assert_eq!((cert_n, conn_n), (0, 0), "主密钥未解锁应空操作");

        // 验证原明文未被改动
        let row: String = storage
            .connection()
            .with_connection(|conn| {
                let v: String =
                    conn.query_row("SELECT params FROM connections LIMIT 1", [], |r| r.get(0))?;
                Ok(v)
            })
            .unwrap();
        assert_eq!(row, r#"{"ssh_private_key":"PLAIN_KEY"}"#, "不应动明文");
    }
}

//! 冲突检测与解决
//!
//! 提供同步冲突的检测、分类和解决策略。
//!
//! ## 冲突类型
//!
//! - BothModified: 本地和云端都修改了同一连接
//! - LocalDeletedCloudModified: 本地删除了连接，但云端有更新
//! - LocalModifiedCloudDeleted: 本地有更新，但云端删除了连接
//!
//! ## 解决策略
//!
//! - UseCloud: 使用云端版本（丢弃本地修改）
//! - UseLocal: 使用本地版本（覆盖云端）
//! - KeepBoth: 保留两个版本（创建副本）

#[cfg(test)]
use crate::cloud_sync::models::ConflictResolution;
use crate::cloud_sync::models::{CloudSyncData, ConflictType, SyncConflict};
use crate::storage::StoredConnection;
#[cfg(test)]
use serde_json::{Map, Value};
#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

/// 计算内容的 fingerprint（递归键排序后序列化）
#[cfg(test)]
fn fingerprint(content: &str) -> String {
    let parsed: Value =
        serde_json::from_str(content).unwrap_or_else(|_| Value::String(content.to_string()));
    let normalized = sort_json_keys(&parsed);
    serde_json::to_string(&normalized).unwrap_or_else(|_| content.to_string())
}

/// 递归对 JSON 对象键排序
#[cfg(test)]
fn sort_json_keys(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sorted: Map<String, Value> = Map::new();
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            for key in keys {
                sorted.insert(key.clone(), sort_json_keys(&map[key]));
            }
            Value::Object(sorted)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(|v| sort_json_keys(v)).collect()),
        _ => value.clone(),
    }
}

/// 冲突解决器
///
/// 负责：
/// - 检测冲突
/// - 根据策略自动解决冲突
/// - 创建冲突副本
pub(crate) struct ConflictResolver {
    /// 默认解决策略
    #[cfg(test)]
    default_strategy: ConflictResolution,
}

impl ConflictResolver {
    /// 创建新的冲突解决器
    #[cfg(test)]
    fn new(default_strategy: ConflictResolution) -> Self {
        Self { default_strategy }
    }

    /// 使用默认策略创建冲突解决器（使用云端版本）
    #[cfg(test)]
    fn with_cloud_priority() -> Self {
        Self::new(ConflictResolution::UseCloud)
    }

    /// 获取当前默认策略
    #[cfg(test)]
    fn default_strategy(&self) -> ConflictResolution {
        self.default_strategy
    }

    /// 检测本地修改但云端已删除的情况
    pub(crate) fn detect_local_modified_cloud_deleted(
        local: &StoredConnection,
        cloud_id: &str,
    ) -> SyncConflict {
        // 构造占位 CloudSyncData
        let placeholder = CloudSyncData {
            id: cloud_id.to_string(),
            owner_id: String::new(),
            data_type: crate::cloud_sync::models::data_type::CONNECTION.to_string(),
            name: local.name.clone(),
            encrypted_data: String::new(),
            key_version: 0,
            checksum: String::new(),
            version: 0,
            updated_at: 0,
            deleted_at: None,
        };

        SyncConflict {
            local: local.clone(),
            cloud: placeholder,
            cloud_name: local.name.clone(),
            conflict_type: ConflictType::LocalModifiedCloudDeleted,
        }
    }

    /// 创建冲突副本
    ///
    /// 复制连接并重命名，标记来源和时间
    #[cfg(test)]
    fn create_conflict_copy(&self, conn: &StoredConnection, source: &str) -> StoredConnection {
        let timestamp = Self::current_timestamp();
        let formatted_time = Self::format_timestamp(timestamp);

        let mut copy = conn.clone();
        copy.id = None; // 清除 ID，作为新连接插入
        copy.sort_order = None; // 让仓库层为副本分配新的组内顺序
        copy.cloud_id = None; // 清除云端关联
        copy.last_synced_at = None; // 清除同步状态
        copy.name = format!("{} ({} {})", conn.name, source, formatted_time);

        copy
    }

    /// 获取当前时间戳（秒）
    #[cfg(test)]
    fn current_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// 格式化时间戳为可读字符串
    #[cfg(test)]
    fn format_timestamp(timestamp: i64) -> String {
        use chrono::{DateTime, Utc};
        DateTime::from_timestamp(timestamp, 0)
            .map(|dt: DateTime<Utc>| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| timestamp.to_string())
    }
}

#[cfg(test)]
impl Default for ConflictResolver {
    fn default() -> Self {
        Self::with_cloud_priority()
    }
}

// ============================================================================
// 三路合并算法（Three-Way Merge）
// 移植自 Netcatty syncMerge.ts
// ============================================================================

/// 三路合并结果
#[cfg(test)]
#[derive(Debug, Clone)]
enum ThreeWayMergeResult<T> {
    /// 合并后保留的实体
    Merged(T),
    /// 仅存在于 base 中（两边都删除了），保留为删除标记
    Deleted,
}

/// 三路合并器
///
/// 对给定 ID 的实体，比较 base（共同祖先）、local（本地版本）、remote（云端版本）：
/// - 纯新增 → 保留
/// - 纯删除 → 标记删除
/// - 两边都改 → 优先本地（记录冲突）
/// - 一方改一方删 → 保留修改（安全优先）
#[cfg(test)]
struct ThreeWayMerger<T: Clone> {
    /// 内容提取函数
    extract_content: Box<dyn Fn(&T) -> String + Send + Sync>,
}

#[cfg(test)]
impl<T: Clone> ThreeWayMerger<T> {
    /// 创建三路合并器
    ///
    /// `extract_content` 用于将实体转换为可比较的字符串，用于 fingerprint 计算。
    fn new<F>(extract_content: F) -> Self
    where
        F: Fn(&T) -> String + 'static + Send + Sync,
    {
        Self {
            extract_content: Box::new(extract_content),
        }
    }

    /// 计算指纹（空内容返回固定值）
    fn calc_fp(content: &str) -> String {
        if content.is_empty() {
            "null".to_string()
        } else {
            fingerprint(content)
        }
    }

    /// 对单条记录执行三路合并
    fn merge_entity(
        &self,
        base_fp: &str,
        local: Option<&T>,
        remote: Option<&T>,
    ) -> ThreeWayMergeResult<T> {
        let local_fp = local
            .map(|e| Self::calc_fp(&(self.extract_content)(e)))
            .unwrap_or_else(|| "null".to_string());
        let remote_fp = remote
            .map(|e| Self::calc_fp(&(self.extract_content)(e)))
            .unwrap_or_else(|| "null".to_string());

        // 基础版本
        let base_fp_actual = if base_fp.is_empty() {
            "null".to_string()
        } else {
            base_fp.to_string()
        };

        // 两边都没变化
        if local_fp == base_fp_actual && remote_fp == base_fp_actual {
            if let Some(v) = local {
                return ThreeWayMergeResult::Merged(v.clone());
            }
            return ThreeWayMergeResult::Deleted;
        }

        // 纯新增（local 新增，remote 不变）
        if local_fp != base_fp_actual && remote_fp == base_fp_actual {
            if let Some(v) = local {
                return ThreeWayMergeResult::Merged(v.clone());
            }
        }

        // 纯新增（remote 新增，local 不变）
        if remote_fp != base_fp_actual && local_fp == base_fp_actual {
            if let Some(v) = remote {
                return ThreeWayMergeResult::Merged(v.clone());
            }
        }

        // 两边都删除了（相对于 base）
        if local_fp == "null" && remote_fp == "null" && base_fp_actual != "null" {
            return ThreeWayMergeResult::Deleted;
        }

        // 一方删一方改 → 优先保留修改（安全优先）
        if local_fp == "null" && remote_fp != base_fp_actual {
            if let Some(v) = remote {
                return ThreeWayMergeResult::Merged(v.clone());
            }
        }
        if remote_fp == "null" && local_fp != base_fp_actual {
            if let Some(v) = local {
                return ThreeWayMergeResult::Merged(v.clone());
            }
        }

        // 两边都改（冲突） → 优先本地
        if local_fp != base_fp_actual && remote_fp != base_fp_actual && local_fp != remote_fp {
            if let Some(v) = local {
                return ThreeWayMergeResult::Merged(v.clone());
            }
        }

        // 两边相同修改
        if local_fp == remote_fp && local_fp != base_fp_actual {
            if let Some(v) = local {
                return ThreeWayMergeResult::Merged(v.clone());
            }
        }

        // fallback
        if let Some(v) = local {
            ThreeWayMergeResult::Merged(v.clone())
        } else if let Some(v) = remote {
            ThreeWayMergeResult::Merged(v.clone())
        } else {
            ThreeWayMergeResult::Deleted
        }
    }
}

#[cfg(test)]
mod three_way_merge_tests {
    use super::*;

    #[derive(Clone)]
    struct TestEntity {
        content: String,
    }

    fn extract(e: &TestEntity) -> String {
        e.content.clone()
    }

    #[test]
    fn test_both_unchanged() {
        let merger = ThreeWayMerger::new(extract);
        let entity = TestEntity {
            content: r#"{"name":"test"}"#.to_string(),
        };
        let fp = fingerprint(r#"{"name":"test"}"#);

        let result = merger.merge_entity(&fp, Some(&entity), Some(&entity));
        match result {
            ThreeWayMergeResult::Merged(v) => assert_eq!(v.content, entity.content),
            _ => panic!("expected Merged"),
        }
    }

    #[test]
    fn test_local_added() {
        let merger = ThreeWayMerger::new(extract);
        let entity = TestEntity {
            content: r#"{"name":"test"}"#.to_string(),
        };

        let result = merger.merge_entity("", Some(&entity), None);
        match result {
            ThreeWayMergeResult::Merged(v) => assert_eq!(v.content, entity.content),
            _ => panic!("expected Merged"),
        }
    }

    #[test]
    fn test_remote_added() {
        let merger = ThreeWayMerger::new(extract);
        let entity = TestEntity {
            content: r#"{"name":"test"}"#.to_string(),
        };

        let result = merger.merge_entity("", None, Some(&entity));
        match result {
            ThreeWayMergeResult::Merged(v) => assert_eq!(v.content, entity.content),
            _ => panic!("expected Merged"),
        }
    }

    #[test]
    fn test_both_modified_conflict_prefers_local() {
        let merger = ThreeWayMerger::new(extract);
        let base = r#"{"name":"test"}"#;
        let local = TestEntity {
            content: r#"{"name":"local"}"#.to_string(),
        };
        let remote = TestEntity {
            content: r#"{"name":"remote"}"#.to_string(),
        };
        let base_fp = fingerprint(base);

        let result = merger.merge_entity(&base_fp, Some(&local), Some(&remote));
        match result {
            ThreeWayMergeResult::Merged(v) => {
                // 优先本地
                assert_eq!(v.content, r#"{"name":"local"}"#);
            }
            _ => panic!("expected Merged"),
        }
    }

    #[test]
    fn test_same_modification() {
        let merger = ThreeWayMerger::new(extract);
        let base = r#"{"name":"test"}"#;
        let local = TestEntity {
            content: r#"{"name":"both"}"#.to_string(),
        };
        let remote = TestEntity {
            content: r#"{"name":"both"}"#.to_string(),
        };
        let base_fp = fingerprint(base);

        let result = merger.merge_entity(&base_fp, Some(&local), Some(&remote));
        match result {
            ThreeWayMergeResult::Merged(v) => assert_eq!(v.content, r#"{"name":"both"}"#),
            _ => panic!("expected Merged"),
        }
    }

    #[test]
    fn test_delete_vs_modify_prefers_modify() {
        let merger = ThreeWayMerger::new(extract);
        let base = r#"{"name":"test"}"#;
        let local = TestEntity {
            content: r#"{"name":"modified"}"#.to_string(),
        };
        let base_fp = fingerprint(base);

        // remote 删除了，local 修改了 → 保留修改
        let result = merger.merge_entity(&base_fp, Some(&local), None);
        match result {
            ThreeWayMergeResult::Merged(v) => assert_eq!(v.content, r#"{"name":"modified"}"#),
            _ => panic!("expected Merged"),
        }

        // local 删除了，remote 修改了 → 保留修改
        let result2 = merger.merge_entity(&base_fp, None, Some(&local));
        match result2 {
            ThreeWayMergeResult::Merged(v) => assert_eq!(v.content, r#"{"name":"modified"}"#),
            _ => panic!("expected Merged"),
        }
    }

    #[test]
    fn test_fingerprint_stable() {
        let content = r#"{"z":"last","a":"first","nested":{"b":1,"a":2}}"#;
        let fp1 = fingerprint(content);
        let fp2 = fingerprint(content);
        assert_eq!(fp1, fp2);

        // 不同顺序应产生相同 fingerprint
        let content2 = r#"{"a":"first","nested":{"a":2,"b":1},"z":"last"}"#;
        let fp3 = fingerprint(content2);
        assert_eq!(fp1, fp3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conflict_resolver_default() {
        let resolver = ConflictResolver::default();
        assert_eq!(resolver.default_strategy(), ConflictResolution::UseCloud);
    }

    #[test]
    fn test_conflict_copy_naming() {
        let resolver = ConflictResolver::default();
        let conn = StoredConnection {
            id: Some(1),
            name: "Test Connection".to_string(),
            connection_type: crate::storage::ConnectionType::Database,
            sort_order: Some(0),
            workspace_id: None,
            params: "{}".to_string(),
            selected_databases: None,
            remark: None,
            sync_enabled: true,
            cloud_id: Some("cloud-123".to_string()),
            last_synced_at: Some(100),
            created_at: None,
            updated_at: Some(200),
            owner_id: None,
        };

        let copy = resolver.create_conflict_copy(&conn, "本地");
        assert!(copy.name.contains("Test Connection"));
        assert!(copy.name.contains("本地"));
        assert!(copy.id.is_none());
        assert!(copy.cloud_id.is_none());
    }
}

//! sync_server 后端同步驱动
//!
//! 通过自建 REST API 进行同步，支持：
//! - 用户认证
//! - 云端密钥配置同步（key_version）
//! - 通过 `generic_sync` 通用流程同步各数据类型
//! - 冲突检测与解决

use super::engine::SyncEngine;
use super::models::SyncResult;
use super::service::SyncError;

impl SyncEngine {
    /// sync_server 后端同步流程
    pub(crate) async fn sync_server_flow(&self) -> Result<SyncResult, SyncError> {
        let mut result = SyncResult::default();

        for handler in &self.handlers {
            match handler.sync(self).await {
                Ok(sync_result) => {
                    result.uploaded += sync_result.uploaded;
                    result.downloaded += sync_result.downloaded;
                    result.deleted += sync_result.deleted;
                    result.conflicts.extend(sync_result.conflicts);
                    result.errors.extend(sync_result.errors);
                }
                Err(e) => {
                    tracing::error!("[同步] {}同步失败: {}", handler.name(), e);
                    result
                        .errors
                        .push(format!("{}同步失败: {}", handler.name(), e));
                }
            }
        }

        tracing::info!(
            "========== 同步完成: 上传 {} 个, 下载 {} 个, 错误 {} 个 ==========",
            result.uploaded,
            result.downloaded,
            result.errors.len()
        );

        Ok(result)
    }
}

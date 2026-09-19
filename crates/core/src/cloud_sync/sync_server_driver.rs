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
use rust_i18n::t;

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
                    let message = t!(
                        "CloudSync.handler_sync_failed",
                        name = handler.name(),
                        error = e.to_string()
                    )
                    .to_string();
                    tracing::error!(
                        "{}",
                        t!("CloudSync.log_handler_error_wrapper", message = message)
                    );
                    result.errors.push(message);
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

//! BlobVault 后端同步驱动
//!
//! 通过文件级存储后端（GitHub Gist、WebDAV、OneDrive、Google Drive 等）进行同步。
//! 不依赖 sync_server REST API，直接通过 `BlobVault` trait 操作文件。
//!
//! ## 差异点（与 sync_server 对比）
//! - 不需要用户登录态
//! - 不需要团队列表
//! - 不需要云端密钥配置同步
//! - 数据以加密 blob 文件形式存储，而非 REST 项

use super::blob_vault::BlobVault;
use super::engine::SyncEngine;
use super::models::SyncResult;
use super::service::SyncError;
use std::sync::Arc;

impl SyncEngine {
    /// BlobVault 后端同步流程
    pub(crate) async fn blob_sync(&self) -> Result<SyncResult, SyncError> {
        let vault = self
            .blob_vault
            .as_ref()
            .ok_or_else(|| SyncError::NetworkError("未配置 Blob 存储后端".to_string()))?;

        tracing::info!("[Blob同步] 后端类型: {}", vault.backend_type());

        let mut result = SyncResult::default();

        for handler in &self.handlers {
            match self.sync_handler_via_blob(handler.as_ref(), vault).await {
                Ok(sync_result) => {
                    result.uploaded += sync_result.uploaded;
                    result.downloaded += sync_result.downloaded;
                    result.deleted += sync_result.deleted;
                    result.conflicts.extend(sync_result.conflicts);
                    result.errors.extend(sync_result.errors);
                }
                Err(e) => {
                    tracing::error!("[Blob同步] {}同步失败: {}", handler.name(), e);
                    result
                        .errors
                        .push(format!("{}同步失败: {}", handler.name(), e));
                }
            }
        }

        tracing::info!(
            "========== Blob同步完成: 上传 {} 个, 下载 {} 个, 错误 {} 个 ==========",
            result.uploaded,
            result.downloaded,
            result.errors.len()
        );

        Ok(result)
    }

    /// 通过 BlobVault 同步单个 handler 的数据
    async fn sync_handler_via_blob(
        &self,
        handler: &dyn super::engine::SyncHandler,
        vault: &Arc<dyn BlobVault>,
    ) -> Result<SyncResult, SyncError> {
        let type_name = handler.name();
        let result = SyncResult::default();

        // 注意：handler 内部通过 generic_sync 调用 cloud_client API，
        // blob 后端无法直接使用。当前跳过 handlers，后续可为每种 handler
        // 类型实现独立的 blob 同步逻辑。

        tracing::info!("[Blob同步] 跳过 {}（待实现 blob 同步逻辑）", type_name);

        // 列举所有 blob（验证连接）
        match vault.list(None).await {
            Ok(blobs) => {
                tracing::info!("[Blob同步] 云端已有 {} 个 blob", blobs.len());
            }
            Err(e) => {
                tracing::warn!("[Blob同步] 列举 blob 失败: {}", e);
            }
        }

        Ok(result)
    }
}

//! sync_server 后端同步驱动
//!
//! 通过自建 REST API 进行同步，支持：
//! - 用户认证、团队管理
//! - 云端密钥配置同步（key_version）
//! - 通过 `generic_sync` 通用流程同步各数据类型
//! - 冲突检测与解决

use super::engine::SyncEngine;
use super::models::SyncResult;
use super::service::SyncError;

impl SyncEngine {
    /// sync_server 后端同步流程
    pub(crate) async fn sync_server_flow(&self) -> Result<SyncResult, SyncError> {
        // 获取并缓存团队列表
        match self.cloud_client.list_teams().await {
            Ok(teams) => {
                tracing::info!("[同步] 获取到 {} 个团队", teams.len());

                // 获取当前用户 ID
                let user_id = self
                    .crypto_service
                    .read()
                    .ok()
                    .and_then(|s| s.user_id().map(|id| id.to_string()));

                // 缓存团队角色信息到 team_key_cache
                if let Some(uid) = &user_id {
                    self.cache_team_roles(&teams, uid).await;
                }

                if let Ok(mut cache) = self.cached_teams.write() {
                    *cache = teams;
                }
            }
            Err(e) => {
                tracing::warn!("[同步] 获取团队列表失败: {}（将仅同步个人数据）", e);
            }
        }

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

    /// 缓存团队角色信息到 team_key_cache 表
    async fn cache_team_roles(&self, teams: &[super::models::Team], user_id: &str) {
        use crate::storage::traits::Repository;
        use crate::storage::TeamKeyCacheRepository;

        let repo = match self.storage.get::<TeamKeyCacheRepository>() {
            Some(repo) => repo,
            None => return,
        };

        for team in teams {
            match self.cloud_client.list_team_members(&team.id).await {
                Ok(members) => {
                    if let Some(member) = members.iter().find(|m| m.user_id == user_id) {
                        let role_str = match member.role {
                            super::models::TeamRole::Owner => "owner",
                            super::models::TeamRole::Member => "member",
                        };
                        if let Ok(Some(mut cache)) = repo.get(&team.id) {
                            cache.role = Some(role_str.to_string());
                            if let Err(e) = repo.upsert(&cache) {
                                tracing::warn!("[同步] 更新团队 {} 角色缓存失败: {}", team.id, e);
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("[同步] 获取团队 {} 成员列表失败: {}", team.id, e);
                }
            }
        }
    }
}

//! 传输任务的源/目标会话管理，获取与释放方式对齐现有 import/export 后台任务

use std::sync::Arc;

use anyhow::Result;
use one_core::storage::{DatabaseType, DbConnectionConfig};

use crate::manager::{GlobalDbState, SessionConnectionGuard};
use crate::plugin::DatabasePlugin;

use super::types::TransferConfig;

/// 源/目标两端已建立的会话
pub(crate) struct TransferEndpoints {
    pub source_plugin: Arc<dyn DatabasePlugin>,
    pub source_session: String,
    pub target_plugin: Arc<dyn DatabasePlugin>,
    pub target_session: String,
}

pub(crate) async fn open_endpoints(
    state: &GlobalDbState,
    config: &TransferConfig,
) -> Result<TransferEndpoints> {
    let source_config = &config.source_config;
    let target_config = &config.target_config;
    let source_plugin = state.get_plugin(&source_config.database_type)?;
    let target_plugin = state.get_plugin(&target_config.database_type)?;
    let source_session = open_session(state, source_config, &config.source_db).await?;
    let target_session = match open_session(state, target_config, &config.target_db).await {
        Ok(session) => session,
        Err(e) => {
            let _ = state
                .connection_manager
                .release_session(&source_session)
                .await;
            return Err(e);
        }
    };
    Ok(TransferEndpoints {
        source_plugin,
        source_session,
        target_plugin,
        target_session,
    })
}

/// 建会话前把配置指向目标库（Oracle 走 sid/service_name，与 manager 现有约定一致）
async fn open_session(
    state: &GlobalDbState,
    base_config: &DbConnectionConfig,
    database: &str,
) -> Result<String> {
    let mut cfg = base_config.clone();
    if cfg.database_type != DatabaseType::Oracle {
        cfg.database = Some(database.to_string());
    }
    state
        .connection_manager
        .create_session(cfg, &state.db_manager)
        .await
        .map_err(Into::into)
}

pub(crate) async fn release_sessions(state: &GlobalDbState, endpoints: &TransferEndpoints) {
    for session in [&endpoints.source_session, &endpoints.target_session] {
        if let Err(e) = state.connection_manager.release_session(session).await {
            tracing::warn!("release transfer session failed: {}", e);
        }
    }
}

pub(crate) async fn lock_connections(
    state: &GlobalDbState,
    endpoints: &TransferEndpoints,
) -> Result<(SessionConnectionGuard, SessionConnectionGuard)> {
    let source = state
        .connection_manager
        .get_session_connection(&endpoints.source_session)
        .await?;
    let target = state
        .connection_manager
        .get_session_connection(&endpoints.target_session)
        .await?;
    Ok((source, target))
}

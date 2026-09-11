//! 视图传输：取源端定义并在目标端执行 CREATE VIEW

use anyhow::{Context, Result};

use crate::types::ViewInfo;

use super::engine::{StepError, TransferCtx, execute_target, failed, is_cancelled, send};
use super::sql;
use super::types::{TransferProgressEvent, TransferSummary};

/// 返回值表示是否因取消而提前结束
pub(crate) async fn transfer_views(
    ctx: &TransferCtx<'_>,
    summary: &mut TransferSummary,
) -> Result<bool> {
    if ctx.config.views.is_empty() {
        return Ok(false);
    }
    if is_cancelled(ctx.cancel) {
        return Ok(true);
    }
    let views = ctx
        .source_plugin
        .list_views(ctx.source, &ctx.config.source_db, None)
        .await
        .context("list source views failed")?;
    for name in &ctx.config.views {
        if is_cancelled(ctx.cancel) {
            return Ok(true);
        }
        if transfer_one_view(ctx, &views, name, summary).await? {
            return Ok(true);
        }
        if summary.views_failed > 0 && !ctx.config.continue_on_error {
            return Ok(false);
        }
    }
    Ok(false)
}

/// 返回值表示是否因取消而提前结束
async fn transfer_one_view(
    ctx: &TransferCtx<'_>,
    views: &[ViewInfo],
    name: &str,
    summary: &mut TransferSummary,
) -> Result<bool> {
    let definition = views
        .iter()
        .find(|v| &v.name == name)
        .and_then(|v| v.definition.clone());
    let outcome = match definition {
        Some(def) => {
            let create_sql = sql::build_create_view_sql(ctx.target_plugin, name, &def);
            execute_target(ctx, &create_sql).await
        }
        None => Err(failed(format!("view {} definition not found", name))),
    };
    match outcome {
        Ok(()) => {
            summary.views_ok += 1;
            send(
                ctx.tx,
                TransferProgressEvent::ViewDone {
                    name: name.to_string(),
                },
            );
        }
        Err(StepError::Cancelled) => return Ok(true),
        Err(StepError::Failed(e)) => {
            summary.views_failed += 1;
            send(
                ctx.tx,
                TransferProgressEvent::Log(format!("view {} failed: {}", name, e)),
            );
            send(
                ctx.tx,
                TransferProgressEvent::ViewFailed {
                    name: name.to_string(),
                    error: e.to_string(),
                },
            );
        }
    }
    Ok(false)
}

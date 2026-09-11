//! 数据传输编排引擎：建会话、逐表复制结构与数据、进度上报与取消

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, anyhow};

use crate::connection::DbConnection;
use crate::executor::{ExecOptions, SqlResult};
use crate::manager::GlobalDbState;
use crate::plugin::DatabasePlugin;

use super::session::{self, TransferEndpoints};
use super::sql;
use super::types::{
    TransferConfig, TransferProgressEvent, TransferProgressSender, TransferSummary,
};
use super::views::transfer_views;

/// 默认数据分页批量大小
pub const DEFAULT_BATCH_SIZE: usize = 1000;

/// 传输步骤内部错误：区分用户取消与执行失败
pub(crate) enum StepError {
    Cancelled,
    Failed(anyhow::Error),
}

pub(crate) fn send(tx: &TransferProgressSender, event: TransferProgressEvent) {
    let _ = tx.send(event);
}

pub(crate) fn is_cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

pub(crate) fn failed(error: impl std::fmt::Display) -> StepError {
    StepError::Failed(anyhow!("{}", error))
}

/// 一次传输任务的全部输入（聚合以避免过长参数列表）
struct TransferJob<'a> {
    state: &'a GlobalDbState,
    config: &'a TransferConfig,
    tx: &'a TransferProgressSender,
    cancel: &'a AtomicBool,
    batch_size: usize,
}

/// 持有两端连接期间的传输上下文
pub(crate) struct TransferCtx<'a> {
    pub source: &'a (dyn DbConnection + Send + Sync),
    pub target: &'a (dyn DbConnection + Send + Sync),
    pub source_plugin: &'a dyn DatabasePlugin,
    pub target_plugin: &'a dyn DatabasePlugin,
    pub config: &'a TransferConfig,
    pub tx: &'a TransferProgressSender,
    pub cancel: &'a AtomicBool,
    pub batch_size: usize,
}

/// 执行数据传输任务：为源/目标各建会话，逐表复制结构与数据，再复制视图。
///
/// 会话获取/释放与现有 import/export 后台任务一致（ConnectionManager
/// create_session + get_session_connection + release_session）。
pub async fn run_transfer(
    state: &GlobalDbState,
    config: TransferConfig,
    progress_tx: TransferProgressSender,
    cancel: Arc<AtomicBool>,
) -> Result<TransferSummary> {
    let batch_size = if config.batch_size == 0 {
        DEFAULT_BATCH_SIZE
    } else {
        config.batch_size
    };
    let endpoints = session::open_endpoints(state, &config).await?;
    let job = TransferJob {
        state,
        config: &config,
        tx: &progress_tx,
        cancel: &cancel,
        batch_size,
    };
    let mut summary = TransferSummary::default();
    let result = run_with_sessions(&job, &endpoints, &mut summary).await;
    session::release_sessions(state, &endpoints).await;
    if result? {
        send(&progress_tx, TransferProgressEvent::Cancelled);
    } else {
        send(
            &progress_tx,
            TransferProgressEvent::Finished(summary.clone()),
        );
    }
    Ok(summary)
}

/// 返回值表示是否因取消而提前结束
async fn run_with_sessions(
    job: &TransferJob<'_>,
    endpoints: &TransferEndpoints,
    summary: &mut TransferSummary,
) -> Result<bool> {
    let (mut source_guard, mut target_guard) =
        session::lock_connections(job.state, endpoints).await?;
    let source = source_guard
        .connection()
        .ok_or_else(|| anyhow!("source session connection not found"))?;
    let target = target_guard
        .connection()
        .ok_or_else(|| anyhow!("target session connection not found"))?;
    let ctx = TransferCtx {
        source,
        target,
        source_plugin: endpoints.source_plugin.as_ref(),
        target_plugin: endpoints.target_plugin.as_ref(),
        config: job.config,
        tx: job.tx,
        cancel: job.cancel,
        batch_size: job.batch_size,
    };
    if transfer_tables(&ctx, summary).await? {
        return Ok(true);
    }
    transfer_views(&ctx, summary).await
}

async fn transfer_tables(ctx: &TransferCtx<'_>, summary: &mut TransferSummary) -> Result<bool> {
    let total = ctx.config.tables.len();
    for (index, table) in ctx.config.tables.iter().enumerate() {
        if is_cancelled(ctx.cancel) {
            return Ok(true);
        }
        send(
            ctx.tx,
            TransferProgressEvent::TableStart {
                name: table.clone(),
                index,
                total,
            },
        );
        match transfer_one_table(ctx, table).await {
            Ok(rows) => {
                summary.tables_ok += 1;
                summary.rows_copied += rows;
                send(
                    ctx.tx,
                    TransferProgressEvent::TableDone {
                        name: table.clone(),
                        rows,
                    },
                );
            }
            Err(StepError::Cancelled) => return Ok(true),
            Err(StepError::Failed(e)) => {
                summary.tables_failed += 1;
                send(
                    ctx.tx,
                    TransferProgressEvent::TableFailed {
                        name: table.clone(),
                        error: e.to_string(),
                    },
                );
                if !ctx.config.continue_on_error {
                    return Ok(false);
                }
            }
        }
    }
    Ok(false)
}

async fn transfer_one_table(
    ctx: &TransferCtx<'_>,
    table: &str,
) -> std::result::Result<u64, StepError> {
    if ctx.config.drop_target_first {
        let drop_sql = sql::build_drop_table_sql(ctx.target_plugin, table);
        execute_target(ctx, &drop_sql).await?;
    }
    let columns = ctx
        .source_plugin
        .list_columns(ctx.source, &ctx.config.source_db, None, table)
        .await
        .map_err(failed)?;
    if columns.is_empty() {
        return Err(failed(format!("table {} has no columns", table)));
    }
    let create_sql = sql::build_create_table_sql(ctx.target_plugin, table, &columns);
    execute_target(ctx, &create_sql).await?;
    copy_table_data(ctx, table).await
}

async fn copy_table_data(
    ctx: &TransferCtx<'_>,
    table: &str,
) -> std::result::Result<u64, StepError> {
    let mut total = 0u64;
    let mut offset = 0usize;
    loop {
        if is_cancelled(ctx.cancel) {
            return Err(StepError::Cancelled);
        }
        let select = sql::build_select_batch_sql(ctx.source_plugin, table, ctx.batch_size, offset);
        let query = match ctx.source.query(&select).await.map_err(failed)? {
            SqlResult::Query(q) => q,
            SqlResult::Error(e) => return Err(failed(e.message)),
            other => return Err(failed(format!("unexpected result: {:?}", other))),
        };
        let returned = query.rows.len();
        if returned > 0 {
            let insert =
                sql::build_insert_sql(ctx.target_plugin, table, &query.column_meta, &query.rows);
            execute_target(ctx, &insert).await?;
            total += returned as u64;
            send(
                ctx.tx,
                TransferProgressEvent::TableProgress {
                    name: table.to_string(),
                    rows: total,
                },
            );
        }
        if sql::is_last_batch(returned, ctx.batch_size) {
            break;
        }
        offset += ctx.batch_size;
    }
    Ok(total)
}

/// 在目标连接上执行 DDL/DML，任一语句报错即视为失败
pub(crate) async fn execute_target(
    ctx: &TransferCtx<'_>,
    sql_text: &str,
) -> std::result::Result<(), StepError> {
    let options = ExecOptions {
        stop_on_error: true,
        transactional: false,
        max_rows: None,
        streaming: false,
    };
    let results = ctx
        .target
        .execute(ctx.target_plugin, sql_text, options)
        .await
        .map_err(failed)?;
    for result in &results {
        if let SqlResult::Error(e) = result {
            return Err(failed(e.message.clone()));
        }
    }
    Ok(())
}

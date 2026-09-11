//! 执行阶段的进度事件处理：事件 -> 日志/进度/当前对象的映射与后台任务

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{App, AsyncApp, Entity};
use gpui_component::VirtualListScrollHandle;
use rust_i18n::t;
use tokio::sync::mpsc;

use db::{GlobalDbState, TransferConfig, TransferProgressEvent, TransferSummary, run_transfer};
use one_core::gpui_tokio::Tokio;

use super::step_execute::TransferLogEntry;

/// 异步任务里需要回写的实体集合
#[derive(Clone)]
pub(crate) struct ExecHandles {
    pub logs: Entity<Vec<TransferLogEntry>>,
    pub scroll_handle: VirtualListScrollHandle,
    pub progress: Entity<f32>,
    pub current_object: Entity<String>,
    pub summary: Entity<Option<TransferSummary>>,
}

pub(crate) fn push_log(cx: &AsyncApp, handles: &ExecHandles, message: String, is_error: bool) {
    let handles = handles.clone();
    let _ = cx.update(|cx: &mut App| {
        handles.logs.update(cx, |l, cx| {
            l.push(TransferLogEntry { message, is_error });
            cx.notify();
        });
        handles.scroll_handle.scroll_to_bottom();
    });
}

/// 把进度事件转成一行日志（Finished 单独处理，不产生日志）
pub(crate) fn event_log(event: &TransferProgressEvent) -> Option<(String, bool)> {
    match event {
        TransferProgressEvent::TableStart { name, index, total } => Some((
            t!(
                "DataTransfer.log_table_start",
                name = name,
                current = *index + 1,
                total = total
            )
            .to_string(),
            false,
        )),
        TransferProgressEvent::TableProgress { name, rows } => Some((
            t!("DataTransfer.log_table_progress", name = name, rows = rows).to_string(),
            false,
        )),
        TransferProgressEvent::TableDone { name, rows } => Some((
            t!("DataTransfer.log_table_done", name = name, rows = rows).to_string(),
            false,
        )),
        TransferProgressEvent::TableFailed { name, error } => Some((
            t!("DataTransfer.log_table_failed", name = name, error = error).to_string(),
            true,
        )),
        TransferProgressEvent::ViewDone { name } => Some((
            t!("DataTransfer.log_view_done", name = name).to_string(),
            false,
        )),
        TransferProgressEvent::ViewFailed { name, error } => Some((
            t!("DataTransfer.log_view_failed", name = name, error = error).to_string(),
            true,
        )),
        TransferProgressEvent::Log(message) => Some((message.clone(), false)),
        TransferProgressEvent::Cancelled => {
            Some((t!("DataTransfer.transfer_cancelled").to_string(), true))
        }
        TransferProgressEvent::Finished(_) => None,
    }
}

pub(crate) fn is_object_completed(event: &TransferProgressEvent) -> bool {
    matches!(
        event,
        TransferProgressEvent::TableDone { .. }
            | TransferProgressEvent::TableFailed { .. }
            | TransferProgressEvent::ViewDone { .. }
            | TransferProgressEvent::ViewFailed { .. }
    )
}

/// 后台执行传输：消费进度事件并回写 UI 状态
pub(crate) async fn run_transfer_task(
    global_state: GlobalDbState,
    config: TransferConfig,
    handles: ExecHandles,
    running: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    finished: Entity<bool>,
    total_objects: usize,
    cx: &mut AsyncApp,
) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let task = Tokio::spawn_result(cx, async move {
        run_transfer(&global_state, config, tx, cancel).await
    });
    let mut done = 0usize;
    while let Some(event) = rx.recv().await {
        if is_object_completed(&event) {
            done += 1;
        }
        apply_event(cx, &handles, &event, done, total_objects);
        if let Some((message, is_error)) = event_log(&event) {
            push_log(cx, &handles, message, is_error);
        }
    }
    if let Err(e) = task.await {
        push_log(
            cx,
            &handles,
            t!("DataTransfer.transfer_failed", error = e).to_string(),
            true,
        );
    }
    running.store(false, Ordering::Relaxed);
    let _ = cx.update(|cx: &mut App| {
        finished.update(cx, |f, cx| {
            *f = true;
            cx.notify();
        });
    });
}

/// 应用进度事件：更新当前对象、总进度与完成摘要
pub(crate) fn apply_event(
    cx: &AsyncApp,
    handles: &ExecHandles,
    event: &TransferProgressEvent,
    done: usize,
    total: usize,
) {
    match event {
        TransferProgressEvent::TableStart { name, .. } => {
            let name = name.clone();
            let h = handles.clone();
            let _ = cx.update(|cx: &mut App| {
                h.current_object.update(cx, |current, cx| {
                    *current = name;
                    cx.notify();
                });
            });
        }
        TransferProgressEvent::Finished(summary) => {
            let summary = summary.clone();
            let h = handles.clone();
            let _ = cx.update(|cx: &mut App| {
                h.summary.update(cx, |s, cx| {
                    *s = Some(summary);
                    cx.notify();
                });
                h.progress.update(cx, |p, cx| {
                    *p = 100.0;
                    cx.notify();
                });
            });
            return;
        }
        _ => {}
    }
    let progress = if total > 0 {
        (done as f32 / total as f32) * 100.0
    } else {
        0.0
    };
    let h = handles.clone();
    let _ = cx.update(|cx: &mut App| {
        h.progress.update(cx, |p, cx| {
            *p = progress;
            cx.notify();
        });
    });
}

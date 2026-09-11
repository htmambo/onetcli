//! 数据传输向导第四步：执行进度、日志列表与取消

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{
    App, AppContext, AsyncApp, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    Styled, div, prelude::FluentBuilder, px,
};
use gpui_component::{ActiveTheme, VirtualListScrollHandle, h_flex, v_flex, v_virtual_list};
use rust_i18n::t;

use db::{DEFAULT_BATCH_SIZE, GlobalDbState, TransferConfig, TransferSummary};

use super::progress::{ExecHandles, run_transfer_task};
use super::view::DataTransferWindow;

const LOG_LINE_HEIGHT: f32 = 20.0;
const LOG_TAG: &str = "[TRS]";

#[derive(Debug, Clone)]
pub(crate) struct TransferLogEntry {
    pub message: String,
    pub is_error: bool,
}

/// 执行阶段共享状态（取消标志、运行标志与进度实体）
pub(crate) struct ExecState {
    pub logs: Entity<Vec<TransferLogEntry>>,
    pub scroll_handle: VirtualListScrollHandle,
    pub progress: Entity<f32>,
    pub current_object: Entity<String>,
    pub summary: Entity<Option<TransferSummary>>,
    pub finished: Entity<bool>,
    /// 与窗口关闭守卫共享的执行中标志
    pub running: Arc<AtomicBool>,
    pub cancel: Arc<AtomicBool>,
    pub started: bool,
}

impl ExecState {
    pub fn new(cx: &mut App, running: Arc<AtomicBool>) -> Self {
        Self {
            logs: cx.new(|_| Vec::new()),
            scroll_handle: VirtualListScrollHandle::new(),
            progress: cx.new(|_| 0.0),
            current_object: cx.new(|_| String::new()),
            summary: cx.new(|_| None),
            finished: cx.new(|_| false),
            running,
            cancel: Arc::new(AtomicBool::new(false)),
            started: false,
        }
    }

    fn handles(&self) -> ExecHandles {
        ExecHandles {
            logs: self.logs.clone(),
            scroll_handle: self.scroll_handle.clone(),
            progress: self.progress.clone(),
            current_object: self.current_object.clone(),
            summary: self.summary.clone(),
        }
    }
}

impl DataTransferWindow {
    pub(crate) fn build_transfer_config(&self, cx: &App) -> Option<TransferConfig> {
        Some(TransferConfig {
            source_config: self.source.selected_config(cx)?.clone(),
            source_db: self.source.selected_database(cx)?,
            target_config: self.target.selected_config(cx)?.clone(),
            target_db: self.target.selected_database(cx)?,
            tables: self.objects.selected_tables_in_order(),
            views: self.objects.selected_views_in_order(),
            continue_on_error: self.continue_on_error,
            drop_target_first: self.drop_target_first,
            batch_size: DEFAULT_BATCH_SIZE,
        })
    }

    /// 启动传输任务：Tokio 跑引擎，GPUI 侧消费进度事件
    pub(crate) fn start_transfer(&mut self, cx: &mut Context<Self>) {
        if self.exec.started {
            return;
        }
        self.exec.started = true;
        let Some(config) = self.build_transfer_config(cx) else {
            self.exec.logs.update(cx, |l, cx| {
                l.push(TransferLogEntry {
                    message: t!("DataTransfer.endpoints_incomplete").to_string(),
                    is_error: true,
                });
                cx.notify();
            });
            return;
        };
        let total_objects = config.tables.len() + config.views.len();
        self.exec.running.store(true, Ordering::Relaxed);

        let global_state = cx.global::<GlobalDbState>().clone();
        let handles = self.exec.handles();
        let running = self.exec.running.clone();
        let cancel = self.exec.cancel.clone();
        let finished = self.exec.finished.clone();

        cx.spawn(async move |_, cx: &mut AsyncApp| {
            run_transfer_task(
                global_state,
                config,
                handles,
                running,
                cancel,
                finished,
                total_objects,
                cx,
            )
            .await;
        })
        .detach();
        cx.notify();
    }
}

fn log_row(entry: &TransferLogEntry, idx: usize, cx: &App) -> impl IntoElement + use<> {
    let text = format!("{} {}", LOG_TAG, entry.message);
    div()
        .id(("log-entry", idx))
        .w_full()
        .text_xs()
        .h(px(LOG_LINE_HEIGHT))
        .text_ellipsis()
        .overflow_hidden()
        .text_color(if entry.is_error {
            cx.theme().danger
        } else {
            cx.theme().foreground
        })
        .child(text)
}

fn progress_bar(progress: f32, cx: &App) -> impl IntoElement {
    div()
        .h_2()
        .w_full()
        .rounded_full()
        .bg(cx.theme().primary.opacity(0.2))
        .child(
            div()
                .h_full()
                .rounded_full()
                .bg(cx.theme().primary)
                .w(gpui::relative(progress / 100.0)),
        )
}

impl DataTransferWindow {
    fn render_log_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let logs = self.exec.logs.read(cx).clone();
        let item_sizes = Rc::new(
            logs.iter()
                .map(|_| gpui::size(px(0.), px(LOG_LINE_HEIGHT)))
                .collect::<Vec<_>>(),
        );
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .overflow_hidden()
            .bg(cx.theme().background)
            .p_2()
            .child(
                v_virtual_list(
                    cx.entity().clone(),
                    "transfer-logs",
                    item_sizes,
                    move |view, visible_range, _window, cx| {
                        let logs = view.exec.logs.read(cx);
                        visible_range
                            .into_iter()
                            .filter_map(|idx| logs.get(idx).map(|entry| log_row(entry, idx, cx)))
                            .collect()
                    },
                )
                .size_full()
                .track_scroll(&self.exec.scroll_handle),
            )
    }

    /// 渲染第四步：当前对象 + 总进度条 + 日志 + 完成统计
    pub(crate) fn render_execute_step(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let progress = *self.exec.progress.read(cx);
        let current_object = self.exec.current_object.read(cx).clone();
        let finished = *self.exec.finished.read(cx);
        let summary = self.exec.summary.read(cx).clone();

        v_flex()
            .size_full()
            .gap_2()
            .p_4()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{}:", t!("DataTransfer.current_object"))),
                    )
                    .child(div().text_sm().child(current_object)),
            )
            .when(finished, |this| {
                this.when_some(summary, |this, s| {
                    this.child(
                        div().text_sm().child(
                            t!(
                                "DataTransfer.summary_stats",
                                tables_ok = s.tables_ok,
                                tables_failed = s.tables_failed,
                                views_ok = s.views_ok,
                                views_failed = s.views_failed,
                                rows = s.rows_copied
                            )
                            .to_string(),
                        ),
                    )
                })
            })
            .child(self.render_log_list(cx))
            .child(progress_bar(progress, cx))
    }
}

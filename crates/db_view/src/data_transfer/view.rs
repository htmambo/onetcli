//! 数据传输向导根视图：四步状态机 + 标题栏/步骤条/底部按钮

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{
    AnyElement, App, AppContext, AsyncApp, Context, Entity, FocusHandle, Focusable, IntoElement,
    ParentElement, Render, Styled, Subscription, Window, div,
};
use gpui_component::{
    ActiveTheme, TitleBar,
    input::InputEvent,
    select::{SelectEvent, SelectState},
    v_flex,
};
use rust_i18n::t;

use db::{GlobalDbState, PrecheckIssue, PrecheckReport, run_precheck};
use one_core::gpui_tokio::Tokio;
use one_core::popup_window::{
    PopupWindowOptions, open_popup_window_with_should_close, request_popup_window_close,
};
use one_core::storage::traits::Repository;
use one_core::storage::{
    ConnectionRepository, ConnectionType, DbConnectionConfig, GlobalStorageState,
};

use super::step_endpoints::{ConnectionItem, EndpointState};
use super::step_execute::ExecState;
use super::step_objects::ObjectsState;
const WINDOW_WIDTH: f32 = 900.0;
const WINDOW_HEIGHT: f32 = 620.0;

/// 向导四步：端点 -> 对象 -> 摘要 -> 执行
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferStep {
    Endpoints,
    Objects,
    Summary,
    Execute,
}

impl TransferStep {
    pub fn index(self) -> usize {
        match self {
            Self::Endpoints => 0,
            Self::Objects => 1,
            Self::Summary => 2,
            Self::Execute => 3,
        }
    }

    pub(crate) fn titles() -> [String; 4] {
        [
            t!("DataTransfer.step_endpoints").to_string(),
            t!("DataTransfer.step_objects").to_string(),
            t!("DataTransfer.step_summary").to_string(),
            t!("DataTransfer.step_execute").to_string(),
        ]
    }
}

/// 数据传输向导窗口根视图
pub struct DataTransferWindow {
    pub(crate) step: TransferStep,
    pub(crate) source: EndpointState,
    pub(crate) target: EndpointState,
    pub(crate) objects: ObjectsState,
    pub(crate) continue_on_error: bool,
    pub(crate) drop_target_first: bool,
    pub(crate) exec: ExecState,
    /// 摘要页预检报告；None 表示尚未完成或连接信息不全
    pub(crate) precheck_report: Entity<Option<PrecheckReport>>,
    pub(crate) precheck_loading: Entity<bool>,
    /// 预检轮次计数器：用于丢弃 Summary↔Objects 往返造成的过期 run_precheck 结果
    pub(crate) precheck_generation: u64,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl DataTransferWindow {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        preset_source: Option<(DbConnectionConfig, String)>,
        running: Arc<AtomicBool>,
    ) -> Self {
        let configs = Self::load_stored_configs(cx);
        let preset_index = preset_source
            .as_ref()
            .and_then(|(config, _)| configs.iter().position(|c| c.id == config.id));

        let mut source = EndpointState::new(&configs, preset_index, window, cx);
        if let Some((_, db)) = &preset_source {
            source.pending_db = (!db.is_empty()).then(|| db.clone());
        }
        let target = EndpointState::new(&configs, None, window, cx);
        let objects = ObjectsState::new(window, cx);
        let precheck_report = cx.new(|_| None);
        let precheck_loading = cx.new(|_| false);

        let subscriptions = vec![
            Self::subscribe_connection_select(cx, window, &source.connection_select, true),
            Self::subscribe_connection_select(cx, window, &target.connection_select, false),
            cx.subscribe_in(
                &objects.search,
                window,
                |this, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.objects.query = input.read(cx).text().to_string();
                        cx.notify();
                    }
                },
            ),
        ];

        let mut this = Self {
            step: TransferStep::Endpoints,
            source,
            target,
            objects,
            continue_on_error: false,
            drop_target_first: true,
            exec: ExecState::new(cx, running),
            precheck_report,
            precheck_loading,
            precheck_generation: 0,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };

        if let Some((config, _)) = preset_source {
            if preset_index.is_some() {
                this.on_connection_changed(true, Some(config.id), window, cx);
            }
        }
        this
    }

    /// 从本地存储读取全部数据库连接配置（供源/目标下拉）
    fn load_stored_configs(cx: &App) -> Vec<DbConnectionConfig> {
        let Some(repo) = cx
            .try_global::<GlobalStorageState>()
            .and_then(|state| state.storage.get::<ConnectionRepository>())
        else {
            return vec![];
        };
        repo.list()
            .map(|items| {
                items
                    .into_iter()
                    .filter(|c| c.connection_type == ConnectionType::Database)
                    .filter_map(|c| c.to_db_connection().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn subscribe_connection_select(
        cx: &mut Context<Self>,
        window: &mut Window,
        select: &Entity<SelectState<Vec<ConnectionItem>>>,
        is_source: bool,
    ) -> Subscription {
        cx.subscribe_in(
            select,
            window,
            move |this, _, event: &SelectEvent<Vec<ConnectionItem>>, window, cx| {
                let SelectEvent::Confirm(value) = event;
                this.on_connection_changed(is_source, value.clone(), window, cx);
            },
        )
    }

    /// 源与目标是否指向同一连接的同一数据库
    pub(crate) fn same_endpoint(&self, cx: &App) -> bool {
        let (Some(source), Some(target)) = (
            self.source.selected_config(cx),
            self.target.selected_config(cx),
        ) else {
            return false;
        };
        if source.id.is_empty() || source.id != target.id {
            return false;
        }
        match (
            self.source.selected_database(cx),
            self.target.selected_database(cx),
        ) {
            (Some(source_db), Some(target_db)) => source_db == target_db,
            _ => false,
        }
    }

    fn endpoints_ready(&self, cx: &App) -> bool {
        self.source.selected_config(cx).is_some()
            && self.source.selected_database(cx).is_some()
            && self.target.selected_config(cx).is_some()
            && self.target.selected_database(cx).is_some()
    }

    pub(crate) fn can_go_next(&self, cx: &App) -> bool {
        match self.step {
            TransferStep::Endpoints => self.endpoints_ready(cx) && !self.same_endpoint(cx),
            TransferStep::Objects => self.objects.has_selection() && !self.objects.loading,
            TransferStep::Summary => {
                self.objects.has_selection()
                    && !self.precheck_loading.read(cx)
                    && !self
                        .precheck_report
                        .read(cx)
                        .as_ref()
                        .is_some_and(|r| r.has_blocking_errors())
            }
            TransferStep::Execute => false,
        }
    }

    pub(crate) fn go_next(&mut self, cx: &mut Context<Self>) {
        if !self.can_go_next(cx) {
            return;
        }
        match self.step {
            TransferStep::Endpoints => {
                self.step = TransferStep::Objects;
                self.load_objects(cx);
            }
            TransferStep::Objects => {
                self.step = TransferStep::Summary;
                self.start_precheck(cx);
            }
            TransferStep::Summary => {
                self.step = TransferStep::Execute;
                self.start_transfer(cx);
            }
            TransferStep::Execute => {}
        }
        cx.notify();
    }

    /// 进入摘要页时运行传输预检测：配置不全时返回带 Error 的报告，否则后台异步执行
    ///
    /// 失败/异常路径一律构造 Error 报告，避免 `can_go_next` 基于空报告放行；
    /// generation 计数器丢弃 Summary↔Objects 往返造成的过期结果。
    fn start_precheck(&mut self, cx: &mut Context<Self>) {
        // 失效旧报告并提升 generation：防止往返后 can_go_next 基于陈旧结果放行
        self.precheck_report.update(cx, |report, cx| {
            *report = None;
            cx.notify();
        });
        self.precheck_generation = self.precheck_generation.wrapping_add(1);
        let generation = self.precheck_generation;

        let Some(config) = self.build_transfer_config(cx) else {
            self.precheck_report.update(cx, |report, cx| {
                *report = Some(PrecheckReport {
                    connectivity_ok: false,
                    issues: vec![PrecheckIssue::error(
                        t!("DataTransfer.precheck_connect_failed").to_string(),
                        t!("DataTransfer.precheck_config_missing").to_string(),
                    )],
                    ..Default::default()
                });
                cx.notify();
            });
            self.precheck_loading.update(cx, |loading, cx| {
                *loading = false;
                cx.notify();
            });
            return;
        };
        self.precheck_loading.update(cx, |loading, cx| {
            *loading = true;
            cx.notify();
        });
        let global_state = cx.global::<GlobalDbState>().clone();
        // 预检内部会用 tokio spawn_blocking 打开连接，必须跑在 tokio runtime 里
        let task = Tokio::spawn(
            cx,
            async move { run_precheck(&global_state, &config).await },
        );
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let report = match task.await {
                Ok(report) => report,
                Err(e) => PrecheckReport {
                    connectivity_ok: false,
                    issues: vec![PrecheckIssue::error(
                        t!("DataTransfer.precheck_connect_failed").to_string(),
                        e.to_string(),
                    )],
                    ..Default::default()
                },
            };
            this.update(cx, |view, cx| {
                // 丢弃过期结果：仅当 generation 匹配时才写回
                if view.precheck_generation != generation {
                    return;
                }
                view.precheck_report.update(cx, |slot, cx| {
                    *slot = Some(report);
                    cx.notify();
                });
                view.precheck_loading.update(cx, |loading, cx| {
                    *loading = false;
                    cx.notify();
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn go_prev(&mut self, cx: &mut Context<Self>) {
        self.step = match self.step {
            TransferStep::Objects => TransferStep::Endpoints,
            TransferStep::Summary => TransferStep::Objects,
            other => other,
        };
        cx.notify();
    }
}

impl Focusable for DataTransferWindow {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DataTransferWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content: AnyElement = match self.step {
            TransferStep::Endpoints => self.render_endpoints_step(window, cx).into_any_element(),
            TransferStep::Objects => self.render_objects_step(window, cx).into_any_element(),
            TransferStep::Summary => self.render_summary_step(cx).into_any_element(),
            TransferStep::Execute => self.render_execute_step(cx).into_any_element(),
        };

        v_flex()
            .size_full()
            .rounded(cx.theme().radius_lg)
            .bg(cx.theme().background)
            .child(
                TitleBar::new().child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .flex_1()
                        .text_sm()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(t!("DataTransfer.title").to_string()),
                ),
            )
            .child(self.render_stepper(cx))
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
            .child(self.render_footer(window, cx))
    }
}

/// 打开数据传输向导弹窗；preset_source 为右键来源的连接配置与库名（源侧预选）
pub fn open_data_transfer_window(
    window: &mut Window,
    cx: &mut App,
    preset_source: Option<(DbConnectionConfig, String)>,
) {
    let running = Arc::new(AtomicBool::new(false));
    let running_for_close = running.clone();
    open_popup_window_with_should_close(
        window,
        PopupWindowOptions::new(t!("DataTransfer.title").to_string())
            .size(WINDOW_WIDTH, WINDOW_HEIGHT),
        move |window, cx| cx.new(|cx| DataTransferWindow::new(window, cx, preset_source, running)),
        move |window, cx| {
            // 执行中禁止关窗
            if running_for_close.load(Ordering::Relaxed) {
                return false;
            }
            request_popup_window_close(window, cx);
            false
        },
        cx,
    );
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_step_index_matches_wizard_order() {
        assert_eq!(TransferStep::Endpoints.index(), 0);
        assert_eq!(TransferStep::Objects.index(), 1);
        assert_eq!(TransferStep::Summary.index(), 2);
        assert_eq!(TransferStep::Execute.index(), 3);
    }
}

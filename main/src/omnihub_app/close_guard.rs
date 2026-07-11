use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AnyWindowHandle, App, AppContext, Context, Entity, InteractiveElement, IntoElement, KeyBinding,
    ParentElement, Render, Styled, Task, Window, actions, div, px,
};
use gpui_component::{ActiveTheme, Icon, IconName, h_flex};
use one_core::tab_container::TabContainer;
use one_core::tab_persistence::save_tab_state;
use one_core::{PendingChangeLevel, RunningKind, RunningState};
use rust_i18n::t;
use terminal_view::with_recovery_snapshot_overrides;

use crate::setting_tab::AppSettings;

use super::{GlobalTabContainer, WINDOW_CLOSE_TERMINAL_RECOVERY_MAX_CHARS, collect_running_states};
#[cfg(target_os = "macos")]
use gpui::{Menu, MenuItem};
use gpui_component::{WindowExt, WindowsSurfaceLayer, layered_level_surface_color};

#[derive(Clone)]
pub(super) struct GlobalAppCloseState {
    pub guard: Rc<RefCell<AppCloseGuard>>,
}

impl gpui::Global for GlobalAppCloseState {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AppCloseDecision {
    Allow,
    ForceClose,
    Prompt,
    Ignore,
}

#[derive(Debug, Default)]
pub(super) struct AppCloseGuard {
    dialog_open: bool,
    force_closing: bool,
}

impl AppCloseGuard {
    pub fn on_close_requested(&mut self, has_running_tasks: bool) -> AppCloseDecision {
        if self.force_closing {
            return AppCloseDecision::ForceClose;
        }
        if self.dialog_open {
            return AppCloseDecision::Ignore;
        }
        if has_running_tasks {
            self.dialog_open = true;
            return AppCloseDecision::Prompt;
        }
        AppCloseDecision::Allow
    }

    /// 只读检查，不推进确认框状态机。
    pub fn on_close_requested_without_ui(&self, has_running_tasks: bool) -> AppCloseDecision {
        if self.force_closing {
            return AppCloseDecision::ForceClose;
        }
        if self.dialog_open {
            return AppCloseDecision::Ignore;
        }
        if has_running_tasks {
            return AppCloseDecision::Prompt;
        }
        AppCloseDecision::Allow
    }

    pub fn cancel_prompt(&mut self) {
        self.dialog_open = false;
    }

    pub fn begin_force_close(&mut self) {
        self.dialog_open = false;
        self.force_closing = true;
    }

    pub fn cancel_force_close(&mut self) {
        self.dialog_open = false;
        self.force_closing = false;
    }
}

pub(super) fn create_close_guard_state() -> GlobalAppCloseState {
    GlobalAppCloseState {
        guard: Rc::new(RefCell::new(AppCloseGuard::default())),
    }
}

fn force_close_tabs_then_quit(
    window_handle: AnyWindowHandle,
    tab_container: Entity<TabContainer>,
    guard: Rc<RefCell<AppCloseGuard>>,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        let close_task = window_handle.update(cx, |_, window, cx| {
            tab_container.update(cx, |tc, cx| tc.force_close_all_tabs(window, cx))
        });

        let can_quit = match close_task {
            Ok(task) => task.await,
            Err(_) => true,
        };

        let _ = cx.update(|cx| {
            if can_quit {
                tracing::info!("所有标签页已关闭，准备退出应用");
                cx.quit();
            } else {
                tracing::warn!("部分标签页关闭失败，取消退出");
                guard.borrow_mut().cancel_force_close();
            }
        });
    })
    .detach();
}

fn open_app_close_dialog(
    window: &mut Window,
    tab_container: Entity<TabContainer>,
    running_states: Vec<RunningState>,
    guard: Rc<RefCell<AppCloseGuard>>,
    cx: &mut App,
) {
    let border_color = cx.theme().border;
    let muted_foreground = cx.theme().muted_foreground;
    let error_color = cx.theme().red;
    let yellow = cx.theme().yellow;
    let green = cx.theme().green;
    let window_handle = window.window_handle();

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let tab_container = tab_container.clone();
        let guard_for_ok = guard.clone();
        let guard_for_cancel = guard.clone();

        dialog
            .title(t!("Common.running_process_close_title"))
            .confirm()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .text_sm()
                            .child(t!("Common.running_process_close_message")),
                    )
                    .child(div().h_px().bg(border_color))
                    .children(running_states.iter().map(|state| {
                        let icon = match state.kind {
                            RunningKind::Terminal => Icon::new(IconName::Terminal),
                            RunningKind::Ssh => Icon::new(IconName::Server),
                            RunningKind::Sftp => Icon::new(IconName::FolderOpen),
                            RunningKind::Db => Icon::new(IconName::Database),
                            RunningKind::DbPendingChanges => Icon::new(IconName::TriangleAlert),
                        };

                        let text_color = if matches!(state.kind, RunningKind::DbPendingChanges) {
                            match state.pending_change_level {
                                Some(PendingChangeLevel::Delete) => error_color,
                                Some(PendingChangeLevel::Modify) => yellow,
                                Some(PendingChangeLevel::Insert) => green,
                                None => muted_foreground,
                            }
                        } else {
                            muted_foreground
                        };

                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(icon.size_4().text_color(text_color))
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(div().text_sm().child(state.title.to_string())),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(text_color)
                                    .child(state.activity.to_string()),
                            )
                    })),
            )
            .button_props(
                gpui_component::dialog::DialogButtonProps::default()
                    .ok_text(t!("Common.close_anyway"))
                    .cancel_text(t!("Common.cancel")),
            )
            .on_ok(move |_, _window, cx| {
                guard_for_ok.borrow_mut().begin_force_close();
                force_close_tabs_then_quit(
                    window_handle,
                    tab_container.clone(),
                    guard_for_ok.clone(),
                    cx,
                );
                true
            })
            .on_cancel(move |_, _window, _cx| {
                guard_for_cancel.borrow_mut().cancel_prompt();
                true
            })
    });
}

pub(super) fn request_main_window_close(window: &mut Window, cx: &mut App) -> bool {
    let Some(tab_container) = cx
        .try_global::<GlobalTabContainer>()
        .map(|g| g.tab_container.clone())
    else {
        return true;
    };
    let Some(close_state) = cx.try_global::<GlobalAppCloseState>().cloned() else {
        return true;
    };

    let running_states = collect_running_states(&tab_container, cx);
    let guard = close_state.guard.clone();
    let decision = guard
        .borrow_mut()
        .on_close_requested(!running_states.is_empty());

    match decision {
        AppCloseDecision::Allow | AppCloseDecision::ForceClose => true,
        AppCloseDecision::Ignore => false,
        AppCloseDecision::Prompt => {
            tracing::info!("准备显示退出确认对话框，先保存当前标签状态");
            let state = with_recovery_snapshot_overrides(
                cx,
                None,
                Some(WINDOW_CLOSE_TERMINAL_RECOVERY_MAX_CHARS),
                |cx| tab_container.read(cx).dump(cx),
            );
            if let Err(err) = save_tab_state(&state) {
                tracing::error!("保存标签状态失败：{:?}", err);
            } else {
                tracing::info!("标签状态保存成功，共 {} 个标签", state.tabs.len());
            }
            AppSettings::save_global(cx);

            open_app_close_dialog(window, tab_container, running_states, guard, cx);
            false
        }
    }
}

/// 在没有窗口句柄时执行只读关闭检查。
pub(super) fn request_app_close_without_window(cx: &mut App) -> bool {
    let Some(close_state) = cx.try_global::<GlobalAppCloseState>().cloned() else {
        return true;
    };

    let running_states = cx
        .try_global::<GlobalTabContainer>()
        .map(|g| collect_running_states(&g.tab_container, cx))
        .unwrap_or_default();

    let decision = close_state
        .guard
        .borrow()
        .on_close_requested_without_ui(!running_states.is_empty());

    match decision {
        AppCloseDecision::Allow | AppCloseDecision::ForceClose => true,
        AppCloseDecision::Ignore => false,
        AppCloseDecision::Prompt => {
            tracing::warn!(
                "Cannot quit: {} running task(s) detected. Please close running tasks first.",
                running_states.len()
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppCloseDecision, AppCloseGuard};

    #[test]
    fn app_close_guard_在无活动任务时直接放行() {
        let mut guard = AppCloseGuard::default();

        assert_eq!(guard.on_close_requested(false), AppCloseDecision::Allow);
    }

    #[test]
    fn app_close_guard_在有活动任务时要求确认() {
        let mut guard = AppCloseGuard::default();

        assert_eq!(guard.on_close_requested(true), AppCloseDecision::Prompt);
    }

    #[test]
    fn app_close_guard_确认框打开时忽略重复请求() {
        let mut guard = AppCloseGuard::default();

        assert_eq!(guard.on_close_requested(true), AppCloseDecision::Prompt);
        assert_eq!(guard.on_close_requested(true), AppCloseDecision::Ignore);
    }

    #[test]
    fn app_close_guard_进入强制关闭阶段后直接放行() {
        let mut guard = AppCloseGuard::default();

        assert_eq!(guard.on_close_requested(true), AppCloseDecision::Prompt);
        guard.begin_force_close();

        assert_eq!(guard.on_close_requested(true), AppCloseDecision::ForceClose);
    }
}

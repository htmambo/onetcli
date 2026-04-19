use rust_i18n::t;
use std::collections::HashSet;

use gpui::{
    App, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement,
    Pixels, Render, Size, StatefulInteractiveElement as _, Styled, Window, WindowKind, div, px,
};
use gpui_component::{
    ActiveTheme, Disableable, Sizable, StyledExt, TitleBar, app_style,
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex, v_flex,
};
use one_core::{
    connection_restore::{
        ConnectionRestoreItem, ConnectionRestoreKind, ConnectionRestoreSnapshot,
        LocalTerminalRestoreState, SshTerminalRestoreState, clear_connection_restore_snapshot,
        load_connection_restore_snapshot, snapshot_from_tab_state,
    },
    popup_window::{
        CancelPopup, PopupWindowOptions, open_popup_window_with_should_close,
        request_popup_window_close,
    },
    storage::{StoredConnection, Workspace},
    tab_persistence::load_tab_state,
};

use crate::{home_tab::HomePage, onetcli_app::GlobalMainWindowHandle};

#[derive(Debug, Clone)]
pub struct ResolvedConnectionRestoreItem {
    pub snapshot_id: String,
    pub kind: ConnectionRestoreKind,
    pub title: String,
    pub subtitle: String,
    pub connection: Option<StoredConnection>,
    pub workspace: Option<Workspace>,
    pub active_connection_id: Option<i64>,
    pub local_terminal: Option<LocalTerminalRestoreState>,
    pub ssh_terminal: Option<SshTerminalRestoreState>,
}

pub fn load_pending_connection_restore_snapshot() -> Option<ConnectionRestoreSnapshot> {
    match load_connection_restore_snapshot() {
        Ok(mut snapshot) => {
            merge_tab_state_snapshot_items(&mut snapshot);
            (!snapshot.items.is_empty()).then_some(snapshot)
        }
        Err(error) => {
            tracing::warn!("读取连接恢复快照失败：{}", error);
            None
        }
    }
}

fn merge_tab_state_snapshot_items(snapshot: &mut ConnectionRestoreSnapshot) {
    let Ok(tab_state) = load_tab_state() else {
        return;
    };

    let tab_state_snapshot = snapshot_from_tab_state(&tab_state);
    if tab_state_snapshot.items.is_empty() {
        return;
    }

    let mut existing_snapshot_ids = snapshot
        .items
        .iter()
        .map(|item| item.snapshot_id.clone())
        .collect::<HashSet<_>>();

    for item in tab_state_snapshot.items {
        if existing_snapshot_ids.insert(item.snapshot_id.clone()) {
            snapshot.items.push(item);
        }
    }
}

pub fn clear_pending_connection_restore_snapshot() {
    if let Err(error) = clear_connection_restore_snapshot() {
        tracing::warn!("清理连接恢复快照失败：{}", error);
    }
}

pub fn resolve_restore_items(
    snapshot: &ConnectionRestoreSnapshot,
    connections: &[StoredConnection],
    workspaces: &[Workspace],
) -> Vec<ResolvedConnectionRestoreItem> {
    snapshot
        .items
        .iter()
        .filter_map(|item| resolve_restore_item(item, connections, workspaces))
        .collect()
}

fn resolve_restore_item(
    item: &ConnectionRestoreItem,
    connections: &[StoredConnection],
    workspaces: &[Workspace],
) -> Option<ResolvedConnectionRestoreItem> {
    if item.kind == ConnectionRestoreKind::LocalTerminal {
        let local_terminal = item.local_terminal.clone()?;
        let subtitle = if let Some(working_dir) = local_terminal
            .working_dir
            .as_deref()
            .filter(|dir| !dir.trim().is_empty())
        {
            format!(
                "{} · {}：{}",
                kind_label(item.kind),
                t!("ConnectionRestore.current_directory"),
                working_dir
            )
        } else {
            format!(
                "{} · {}",
                kind_label(item.kind),
                t!("ConnectionRestore.default_directory")
            )
        };

        return Some(ResolvedConnectionRestoreItem {
            snapshot_id: item.snapshot_id.clone(),
            kind: item.kind,
            title: item.title.clone(),
            subtitle,
            connection: None,
            workspace: None,
            active_connection_id: None,
            local_terminal: Some(local_terminal),
            ssh_terminal: None,
        });
    }

    if item.kind.is_workspace() {
        let workspace_id = item.workspace_id?;
        let connection_type = item.connection_type()?;
        let workspace = workspaces
            .iter()
            .find(|workspace| workspace.id == Some(workspace_id))
            .cloned()?;
        let mut candidates = connections
            .iter()
            .filter(|connection| {
                connection.workspace_id == Some(workspace_id)
                    && connection.connection_type == connection_type
            })
            .cloned()
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return None;
        }

        let preferred_connection = item
            .active_connection_id
            .and_then(|id| {
                candidates
                    .iter()
                    .find(|connection| connection.id == Some(id))
                    .cloned()
            })
            .or_else(|| {
                item.connection_id.and_then(|id| {
                    candidates
                        .iter()
                        .find(|connection| connection.id == Some(id))
                        .cloned()
                })
            })
            .or_else(|| candidates.drain(..).next())?;

        Some(ResolvedConnectionRestoreItem {
            snapshot_id: item.snapshot_id.clone(),
            kind: item.kind,
            title: item.title.clone(),
            subtitle: format!(
                "{} · {}",
                kind_label(item.kind),
                t!("ConnectionRestore.workspace_page")
            ),
            active_connection_id: item.active_connection_id.or(preferred_connection.id),
            connection: Some(preferred_connection),
            workspace: Some(workspace),
            local_terminal: None,
            ssh_terminal: None,
        })
    } else {
        let connection_id = item.connection_id?;
        let connection = connections
            .iter()
            .find(|connection| connection.id == Some(connection_id))
            .cloned()?;
        let workspace = connection.workspace_id.and_then(|workspace_id| {
            workspaces
                .iter()
                .find(|workspace| workspace.id == Some(workspace_id))
                .cloned()
        });
        let subtitle = if item.kind == ConnectionRestoreKind::SshTerminal {
            if let Some(working_dir) = item
                .ssh_terminal
                .as_ref()
                .and_then(|state| state.working_dir.as_deref())
                .filter(|dir| !dir.trim().is_empty())
            {
                format!(
                    "{} · {}：{}",
                    kind_label(item.kind),
                    t!("ConnectionRestore.current_directory"),
                    working_dir
                )
            } else {
                format!(
                    "{} · {}：{}",
                    kind_label(item.kind),
                    t!("ConnectionRestore.current_connection"),
                    connection.name
                )
            }
        } else {
            format!(
                "{} · {}：{}",
                kind_label(item.kind),
                t!("ConnectionRestore.current_connection"),
                connection.name
            )
        };

        Some(ResolvedConnectionRestoreItem {
            snapshot_id: item.snapshot_id.clone(),
            kind: item.kind,
            title: item.title.clone(),
            subtitle,
            active_connection_id: connection.id,
            connection: Some(connection),
            workspace,
            local_terminal: None,
            ssh_terminal: item.ssh_terminal.clone(),
        })
    }
}

fn kind_label(kind: ConnectionRestoreKind) -> String {
    match kind {
        ConnectionRestoreKind::LocalTerminal => t!("ConnectionRestore.local_terminal").to_string(),
        ConnectionRestoreKind::SshTerminal => t!("ConnectionRestore.ssh_terminal").to_string(),
        ConnectionRestoreKind::SerialTerminal => {
            t!("ConnectionRestore.serial_terminal").to_string()
        }
        ConnectionRestoreKind::Sftp => "SFTP".to_string(),
        ConnectionRestoreKind::Database => t!("ConnectionRestore.database_page").to_string(),
        ConnectionRestoreKind::DatabaseWorkspace => {
            t!("ConnectionRestore.database_workspace").to_string()
        }
        ConnectionRestoreKind::Redis => t!("ConnectionRestore.redis_page").to_string(),
        ConnectionRestoreKind::RedisWorkspace => {
            t!("ConnectionRestore.redis_workspace").to_string()
        }
        ConnectionRestoreKind::MongoDb => t!("ConnectionRestore.mongodb_page").to_string(),
        ConnectionRestoreKind::MongoDbWorkspace => {
            t!("ConnectionRestore.mongodb_workspace").to_string()
        }
    }
}

/// No-op on non-Unix platforms.
#[cfg(not(unix))]
pub fn probe_pty_sessions(_items: &mut [ResolvedConnectionRestoreItem]) {}

/// Probes active PTY sessions and updates item preferences accordingly.
#[cfg(unix)]
pub fn probe_pty_sessions(items: &mut [ResolvedConnectionRestoreItem]) {
    use std::time::Duration;
    use terminal::{LocalPtyClient, LocalPtyHostEvent, LocalPtyHostRequest};

    let has_sessions = items.iter().any(|item| {
        item.local_terminal
            .as_ref()
            .and_then(|l| l.pty_session_id.as_ref())
            .is_some()
    });
    if !has_sessions {
        return;
    }

    let Ok(mut client) = LocalPtyClient::connect() else {
        for item in items.iter_mut() {
            if let Some(ref mut local) = item.local_terminal {
                if local.pty_session_id.is_some() {
                    local.prefer_live_restore = Some(false);
                }
            }
        }
        return;
    };

    for item in items.iter_mut() {
        let Some(ref session_id) = item
            .local_terminal
            .as_ref()
            .and_then(|l| l.pty_session_id.clone())
        else {
            continue;
        };
        let _ = client.send_request(LocalPtyHostRequest::Query {
            session_id: session_id.clone(),
        });
        let alive = matches!(
            client.recv_event(Duration::from_millis(500)),
            Some(LocalPtyHostEvent::Attached { .. })
        );
        if let Some(ref mut local) = item.local_terminal {
            local.prefer_live_restore = Some(alive);
        }
    }
}

pub fn open_connection_restore_dialog(
    home_page: Entity<HomePage>,
    items: Vec<ResolvedConnectionRestoreItem>,
    window: &mut Window,
    cx: &mut App,
) {
    if items.is_empty() {
        return;
    }

    let layout = compute_connection_restore_popup_layout(window.viewport_size());
    let popup_items = items.clone();
    let home_for_close = home_page.clone();

    open_popup_window_with_should_close(
        window,
        PopupWindowOptions::new(t!("ConnectionRestore.title"))
            .size(f32::from(layout.width), f32::from(layout.height))
            .min_width(520.0)
            .min_height(420.0)
            .kind(WindowKind::Dialog),
        move |_window, cx| {
            let home_page = home_page.clone();
            let items = popup_items.clone();
            cx.new(|_| ConnectionRestorePopupView::new(home_page, items))
        },
        move |window, cx| {
            if let Some(main_window_handle) = cx.try_global::<GlobalMainWindowHandle>().copied() {
                let home_page = home_for_close.clone();
                let _ = cx.update_window(
                    main_window_handle.window_handle,
                    move |_, main_window, cx| {
                        home_page.update(cx, |home, cx| {
                            home.skip_pending_connection_restore(main_window, cx);
                        });
                    },
                );
            }
            request_popup_window_close(window, cx);
            false
        },
        cx,
    );
}

pub struct ConnectionRestorePopupView {
    home_page: Entity<HomePage>,
    items: Vec<ResolvedConnectionRestoreItem>,
    selected_snapshot_ids: HashSet<String>,
    restoring: bool,
}

impl ConnectionRestorePopupView {
    fn new(home_page: Entity<HomePage>, items: Vec<ResolvedConnectionRestoreItem>) -> Self {
        let selected_snapshot_ids = items
            .iter()
            .map(|item| item.snapshot_id.clone())
            .collect::<HashSet<_>>();

        Self {
            home_page,
            items,
            selected_snapshot_ids,
            restoring: false,
        }
    }

    fn selected_snapshot_ids(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|item| self.selected_snapshot_ids.contains(&item.snapshot_id))
            .map(|item| item.snapshot_id.clone())
            .collect()
    }

    fn all_selected(&self) -> bool {
        !self.items.is_empty()
            && self
                .items
                .iter()
                .all(|item| self.selected_snapshot_ids.contains(&item.snapshot_id))
    }

    fn on_skip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 防止重复点击
        if self.restoring {
            return;
        }
        self.restoring = true;

        let home_page = self.home_page.clone();

        // 使用 defer 延迟到下一个事件循环，避免在 update 上下文中嵌套 update
        window.defer(cx, move |_window, cx| {
            if let Some(main_window_handle) = cx.try_global::<GlobalMainWindowHandle>().copied() {
                let _ = cx.update_window(
                    main_window_handle.window_handle,
                    move |_, main_window, cx| {
                        home_page.update(cx, |home, cx| {
                            home.skip_pending_connection_restore(main_window, cx);
                        });
                    },
                );
            }
        });

        request_popup_window_close(window, cx);
    }

    fn on_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // 防止重复点击
        if self.restoring {
            return;
        }
        self.restoring = true;

        let Some(_main_window_handle) = cx.try_global::<GlobalMainWindowHandle>().copied() else {
            tracing::warn!("恢复连接弹窗未找到主窗口句柄，无法执行恢复操作");
            self.restoring = false;
            return;
        };

        let selected_snapshot_ids = self.selected_snapshot_ids();
        let home_page = self.home_page.clone();

        // 使用 defer 延迟到下一个事件循环，避免在 update 上下文中嵌套 update
        window.defer(cx, move |_window, cx| {
            if let Some(main_window_handle) = cx.try_global::<GlobalMainWindowHandle>().copied() {
                let _ = cx.update_window(
                    main_window_handle.window_handle,
                    move |_, main_window, cx| {
                        home_page.update(cx, |home, cx| {
                            home.restore_saved_connection_sessions(
                                &selected_snapshot_ids,
                                main_window,
                                cx,
                            );
                        });
                    },
                );
            }
        });

        request_popup_window_close(window, cx);
    }

    fn on_cancel_popup(&mut self, _: &CancelPopup, window: &mut Window, cx: &mut Context<Self>) {
        self.on_skip(window, cx);
    }
}

impl Render for ConnectionRestorePopupView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let all_selected = self.all_selected();
        let selected_count = self.selected_snapshot_ids.len();
        let total_count = self.items.len();
        let restoring = self.restoring;
        let item_views =
            self.items
                .iter()
                .cloned()
                .map(|item| {
                    let checked = self.selected_snapshot_ids.contains(&item.snapshot_id);
                    let snapshot_id = item.snapshot_id.clone();
                    let view_for_toggle = view.clone();
                    let type_color = match item.kind {
                        ConnectionRestoreKind::LocalTerminal
                        | ConnectionRestoreKind::SerialTerminal => cx.theme().muted_foreground,
                        ConnectionRestoreKind::SshTerminal | ConnectionRestoreKind::Sftp => {
                            cx.theme().warning
                        }
                        ConnectionRestoreKind::Database
                        | ConnectionRestoreKind::DatabaseWorkspace => cx.theme().info,
                        ConnectionRestoreKind::Redis | ConnectionRestoreKind::RedisWorkspace => {
                            cx.theme().success
                        }
                        ConnectionRestoreKind::MongoDb
                        | ConnectionRestoreKind::MongoDbWorkspace => cx.theme().danger,
                    };

                    h_flex()
                        .w_full()
                        .gap_3()
                        .items_start()
                        .p_3()
                        .block_mouse_except_scroll()
                        .bg(cx.theme().background)
                        .border_1()
                        .border_color(cx.theme().border)
                        .rounded_md()
                        .child(
                            Checkbox::new(format!("restore-connection-{}", snapshot_id))
                                .block_mouse_except_scroll()
                                .checked(checked)
                                .on_click(move |_, _, cx| {
                                    view_for_toggle.update(cx, |view, cx| {
                                        if !view.selected_snapshot_ids.insert(snapshot_id.clone()) {
                                            view.selected_snapshot_ids.remove(&snapshot_id);
                                        }
                                        cx.notify();
                                    });
                                }),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .gap_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(item.title),
                                )
                                .child(div().text_xs().text_color(type_color).child(item.subtitle)),
                        )
                })
                .collect::<Vec<_>>();

        v_flex()
            .id("connection-restore-popup")
            .size_full()
            .bg(cx.theme().background)
            .key_context("PopupWindow")
            .on_action(cx.listener(Self::on_cancel_popup))
            .child(
                v_flex()
                    .w_full()
                    .gap_4()
                    .flex_1()
                    .min_h_0()
                    .child(
                        TitleBar::new()
                            .refine_style(&app_style::title_bar_style())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .flex_1()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(t!("ConnectionRestore.title")),
                            ),
                    )
                    .child(
                        v_flex().w_full().gap_2().px_6().pt_4().pb_1().child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(t!("ConnectionRestore.description")),
                        ),
                    )
                    .child(
                        v_flex()
                            .w_full()
                            .flex_1()
                            .min_h_0()
                            .gap_3()
                            .px_6()
                            .pb_4()
                            .child(
                                h_flex()
                                    .w_full()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .items_center()
                                            .child({
                                                let view_for_select_all = view.clone();
                                                Checkbox::new("restore-select-all")
                                                    .checked(all_selected)
                                                    .on_click(move |_, _, cx| {
                                                        view_for_select_all.update(
                                                            cx,
                                                            |view, cx| {
                                                                if view.all_selected() {
                                                                    view.selected_snapshot_ids
                                                                        .clear();
                                                                } else {
                                                                    view.selected_snapshot_ids =
                                                                        view.items
                                                                            .iter()
                                                                            .map(|item| {
                                                                                item.snapshot_id
                                                                                    .clone()
                                                                            })
                                                                            .collect();
                                                                }
                                                                cx.notify();
                                                            },
                                                        );
                                                    })
                                            })
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .child(t!("ConnectionRestore.select_all")),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(
                                                t!("ConnectionRestore.selected_count")
                                                    .replace(
                                                        "%{selected}",
                                                        &selected_count.to_string(),
                                                    )
                                                    .replace("%{total}", &total_count.to_string()),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .id("connection-restore-list-scroll")
                                    .w_full()
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .child(v_flex().w_full().gap_2().children(item_views)),
                            ),
                    )
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .items_center()
                            .px_6()
                            .py_4()
                            .border_t_1()
                            .border_color(app_style::border())
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(t!("ConnectionRestore.partial_restore_hint")),
                            )
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(
                                        Button::new("connection-restore-skip")
                                            .small()
                                            .with_variant(app_style::secondary_button_variant(cx))
                                            .label(t!("ConnectionRestore.skip"))
                                            .disabled(restoring)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.on_skip(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("connection-restore-apply")
                                            .small()
                                            .with_variant(app_style::primary_button_variant(cx))
                                            .label(if restoring {
                                                t!("ConnectionRestore.restoring")
                                            } else {
                                                t!("ConnectionRestore.restore_selected")
                                            })
                                            .disabled(selected_count == 0 || restoring)
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.on_restore(window, cx);
                                            })),
                                    ),
                            ),
                    ),
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ConnectionRestorePopupLayout {
    width: Pixels,
    height: Pixels,
}

fn compute_connection_restore_popup_layout(
    viewport_size: Size<Pixels>,
) -> ConnectionRestorePopupLayout {
    ConnectionRestorePopupLayout {
        width: (viewport_size.width - px(48.0))
            .max(px(520.0))
            .min(px(760.0)),
        height: (viewport_size.height - px(72.0))
            .max(px(420.0))
            .min(px(680.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::compute_connection_restore_popup_layout;
    use gpui::{px, size};

    #[test]
    fn 恢复弹窗在较小主窗口中会收敛_popup_尺寸() {
        let layout = compute_connection_restore_popup_layout(size(px(640.0), px(600.0)));

        assert_eq!(layout.width, px(592.0));
        assert_eq!(layout.height, px(528.0));
    }

    #[test]
    fn 恢复弹窗在较大主窗口中保持_popup_上限尺寸() {
        let layout = compute_connection_restore_popup_layout(size(px(1280.0), px(900.0)));

        assert_eq!(layout.width, px(760.0));
        assert_eq!(layout.height, px(680.0));
    }
}

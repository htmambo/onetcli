use std::collections::HashSet;

use gpui::{
    App, AppContext, Context, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement as _, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, WindowExt, checkbox::Checkbox, dialog::DialogButtonProps, h_flex, v_flex,
};
use one_core::connection_restore::{
    ConnectionRestoreItem, ConnectionRestoreKind, ConnectionRestoreSnapshot,
    clear_connection_restore_snapshot, load_connection_restore_snapshot,
};
use one_core::storage::{ConnectionType, StoredConnection, Workspace};

use crate::home_tab::HomePage;

#[derive(Debug, Clone)]
pub struct ResolvedConnectionRestoreItem {
    pub snapshot_id: String,
    pub kind: ConnectionRestoreKind,
    pub title: String,
    pub subtitle: String,
    pub connection: StoredConnection,
    pub workspace: Option<Workspace>,
    pub active_connection_id: Option<i64>,
}

pub fn load_pending_connection_restore_snapshot() -> Option<ConnectionRestoreSnapshot> {
    match load_connection_restore_snapshot() {
        Ok(snapshot) if !snapshot.items.is_empty() => Some(snapshot),
        Ok(_) => None,
        Err(error) => {
            tracing::warn!("读取连接恢复快照失败：{}", error);
            None
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
    if item.kind.is_workspace() {
        let workspace_id = item.workspace_id?;
        let workspace = workspaces
            .iter()
            .find(|workspace| workspace.id == Some(workspace_id))
            .cloned()?;
        let mut candidates = connections
            .iter()
            .filter(|connection| {
                connection.workspace_id == Some(workspace_id)
                    && connection.connection_type == item.connection_type()
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
            subtitle: format!("{} · 工作区页", kind_label(item.kind)),
            active_connection_id: item.active_connection_id.or(preferred_connection.id),
            connection: preferred_connection,
            workspace: Some(workspace),
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

        Some(ResolvedConnectionRestoreItem {
            snapshot_id: item.snapshot_id.clone(),
            kind: item.kind,
            title: item.title.clone(),
            subtitle: format!("{} · 当前连接：{}", kind_label(item.kind), connection.name),
            active_connection_id: connection.id,
            connection,
            workspace,
        })
    }
}

fn kind_label(kind: ConnectionRestoreKind) -> &'static str {
    match kind {
        ConnectionRestoreKind::SshTerminal => "SSH 终端",
        ConnectionRestoreKind::SerialTerminal => "串口终端",
        ConnectionRestoreKind::Sftp => "SFTP",
        ConnectionRestoreKind::Database => "数据库连接页",
        ConnectionRestoreKind::DatabaseWorkspace => "数据库工作区页",
        ConnectionRestoreKind::Redis => "Redis 连接页",
        ConnectionRestoreKind::RedisWorkspace => "Redis 工作区页",
        ConnectionRestoreKind::MongoDb => "MongoDB 连接页",
        ConnectionRestoreKind::MongoDbWorkspace => "MongoDB 工作区页",
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

    let dialog_view = cx.new(|_| ConnectionRestoreDialogView::new(items));

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let home_for_ok = home_page.clone();
        let home_for_cancel = home_page.clone();
        let dialog_view_for_ok = dialog_view.clone();

        dialog
            .title("恢复连接".to_string())
            .w(px(620.0))
            .child(dialog_view.clone())
            .confirm()
            .button_props(
                DialogButtonProps::default()
                    .ok_text("恢复所选")
                    .cancel_text("跳过"),
            )
            .on_ok(move |_, window, cx| {
                let selected_snapshot_ids = dialog_view_for_ok.read(cx).selected_snapshot_ids();
                let _ = home_for_ok.update(cx, |home, cx| {
                    home.restore_saved_connection_sessions(&selected_snapshot_ids, window, cx);
                });
                true
            })
            .on_cancel(move |_, _window, cx| {
                let _ = home_for_cancel.update(cx, |home, cx| {
                    home.skip_pending_connection_restore(cx);
                });
                true
            })
    });
}

pub struct ConnectionRestoreDialogView {
    items: Vec<ResolvedConnectionRestoreItem>,
    selected_snapshot_ids: HashSet<String>,
}

impl ConnectionRestoreDialogView {
    fn new(items: Vec<ResolvedConnectionRestoreItem>) -> Self {
        let selected_snapshot_ids = items
            .iter()
            .map(|item| item.snapshot_id.clone())
            .collect::<HashSet<_>>();

        Self {
            items,
            selected_snapshot_ids,
        }
    }

    pub fn selected_snapshot_ids(&self) -> Vec<String> {
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
}

impl Render for ConnectionRestoreDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let all_selected = self.all_selected();
        let selected_count = self.selected_snapshot_ids.len();
        let total_count = self.items.len();
        let item_views = self
            .items
            .iter()
            .cloned()
            .map(|item| {
                let checked = self.selected_snapshot_ids.contains(&item.snapshot_id);
                let snapshot_id = item.snapshot_id.clone();
                let view_for_toggle = view.clone();
                let type_color = match item.connection.connection_type {
                    ConnectionType::Database => cx.theme().info,
                    ConnectionType::SshSftp => cx.theme().warning,
                    ConnectionType::Redis => cx.theme().success,
                    ConnectionType::MongoDB => cx.theme().danger,
                    ConnectionType::Serial => cx.theme().muted_foreground,
                    _ => cx.theme().muted_foreground,
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
            .w_full()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("检测到上次退出前仍有打开的连接页，请选择要恢复的项。"),
            )
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
                                        view_for_select_all.update(cx, |view, cx| {
                                            if view.all_selected() {
                                                view.selected_snapshot_ids.clear();
                                            } else {
                                                view.selected_snapshot_ids = view
                                                    .items
                                                    .iter()
                                                    .map(|item| item.snapshot_id.clone())
                                                    .collect();
                                            }
                                            cx.notify();
                                        });
                                    })
                            })
                            .child(div().text_sm().child("全选")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("已选择 {selected_count} / {total_count}")),
                    ),
            )
            .child(
                div()
                    .id("connection-restore-list-scroll")
                    .w_full()
                    .max_h(px(360.0))
                    .overflow_y_scroll()
                    .child(v_flex().w_full().gap_2().children(item_views)),
            )
            .child(
                h_flex().w_full().justify_end().child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("可取消勾选后仅恢复部分连接"),
                ),
            )
    }
}

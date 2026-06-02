use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use db_view::connection_form_window::{ConnectionFormWindow, ConnectionFormWindowConfig};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, AsyncApp, BorrowAppContext, Bounds, Context, DragMoveEvent,
    ElementId, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Pixels, Point, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Subscription, WeakEntity, Window, actions, div, px,
};
use gpui_component::menu::DropdownMenu;
use gpui_component::{
    ActiveTheme, Disableable, ElementExt, Icon, IconName, InteractiveElementExt, Sizable, Size,
    StyledExt, WindowExt, WindowsSurfaceLayer, app_style,
    button::{Button, ButtonCustomVariant, ButtonVariant, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputEvent, InputState},
    layered_level_surface_color,
    list::{List, ListState},
    menu::PopupMenuItem,
    popover::Popover,
    tokens::Radius,
    tooltip::Tooltip,
    v_flex,
};
use mongodb_view::{MongoFormWindow, MongoFormWindowConfig};
use one_core::cloud_sync::{
    BlobVault, CloudSyncService, ConflictResolution, GithubGistVault, GoogleDriveVault,
    OneDriveVault, SyncConflict, SyncEngine, UserInfo,
};
use one_core::connection_notifier::{ConnectionDataEvent, emit_connection_event, get_notifier};
use one_core::crypto;
use one_core::key_storage;
use one_core::popup_window::{PopupWindowOptions, open_popup_window};
use one_core::storage::traits::Repository;
use one_core::storage::{
    ActiveConnections, ConnectionRepository, ConnectionType, DatabaseType, GlobalStorageState,
    PendingCloudDeletionMetadata, PendingCloudDeletionRepository, RedisMode, StoredConnection,
    Workspace, WorkspaceRepository,
};
use one_core::tab_container::{TabContainer, TabContent, TabContentEvent};
use redis_view::{RedisFormWindow, RedisFormWindowConfig};
use rust_i18n::t;
use terminal_view::{SerialFormWindow, SerialFormWindowConfig};
use terminal_view::{SshFormWindow, SshFormWindowConfig};

use crate::auth::AuthService;
use crate::connection_restore::{
    ResolvedConnectionRestoreItem, load_pending_connection_restore_snapshot,
    open_connection_restore_dialog, resolve_restore_items,
};
use crate::home::home_connection_quick_open::ConnectionQuickOpenDelegate;
use crate::home::home_strategy::build_connection_open_strategy;
use crate::home::home_workspace_filter::{WorkspaceFilterDelegate, show_workspace_dialog};
use crate::new_connection::NewConnectionWindow;
use crate::setting_tab::{
    AppSettings, ConnectionListSortField, ConnectionListSortOrder, ConnectionListViewMode,
    GlobalCurrentUser,
};
use one_core::connection_restore::{ConnectionRestoreKind, ConnectionRestoreSnapshot};

actions!(home_tab, [OpenConnectionQuickOpen, NewConnectionShortcut]);

fn pending_connection_restore_snapshot(
    settings: &AppSettings,
) -> Option<ConnectionRestoreSnapshot> {
    if !settings.restore_connections_on_startup {
        crate::connection_restore::clear_pending_connection_restore_snapshot();
        return None;
    }

    load_pending_connection_restore_snapshot()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncFeedbackLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SyncFeedback {
    level: SyncFeedbackLevel,
    message: String,
}

struct SyncResultNotification;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManualInsertPosition {
    Before,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManualDropIndicatorEdge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkspaceDropPreview {
    target_workspace_id: i64,
    position: ManualInsertPosition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConnectionDropPreview {
    workspace_id: Option<i64>,
    target_connection_id: i64,
    position: ManualInsertPosition,
    edge: ManualDropIndicatorEdge,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct DragPreviewSize {
    width: f32,
    height: f32,
}

#[derive(Clone)]
struct DragWorkspace {
    workspace_id: i64,
    name: SharedString,
    preview_size: Option<DragPreviewSize>,
}

impl Render for DragWorkspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let preview_size = self.preview_size.unwrap_or(DragPreviewSize {
            width: 280.0,
            height: 40.0,
        });
        div()
            .id("drag-workspace")
            .cursor_grabbing()
            .flex()
            .items_center()
            .w(px(preview_size.width))
            .h(px(preview_size.height))
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().drag_border)
            .bg(cx.theme().drop_target.opacity(0.95))
            .text_color(cx.theme().foreground)
            .shadow_lg()
            .child(self.name.clone())
    }
}

#[derive(Clone)]
struct DragConnection {
    connection_id: i64,
    workspace_id: Option<i64>,
    name: SharedString,
    preview_size: Option<DragPreviewSize>,
}

impl Render for DragConnection {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let preview_size = self.preview_size.unwrap_or(DragPreviewSize {
            width: 320.0,
            height: 58.0,
        });
        div()
            .id("drag-connection")
            .cursor_grabbing()
            .flex()
            .items_center()
            .w(px(preview_size.width))
            .h(px(preview_size.height))
            .px_3()
            .py_2()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().drag_border)
            .bg(cx.theme().drop_target.opacity(0.95))
            .text_color(cx.theme().foreground)
            .shadow_lg()
            .child(self.name.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConnectionWorkspaceMovePlan {
    source_workspace_id: Option<i64>,
    target_workspace_id: Option<i64>,
    source_connection_ids: Vec<i64>,
    target_connection_ids: Vec<i64>,
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-o", OpenConnectionQuickOpen, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-o", OpenConnectionQuickOpen, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-n", NewConnectionShortcut, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-n", NewConnectionShortcut, None),
    ]);
}

// HomePage Entity - 管理 home 页面的所有状态

pub struct HomePage {
    focus_handle: FocusHandle,
    selected_filter: ConnectionType,
    pub(crate) workspaces: Vec<Workspace>,
    pub(crate) connections: Vec<StoredConnection>,
    pub(crate) tab_container: Entity<TabContainer>,
    pub(crate) terminal_views: Vec<WeakEntity<terminal_view::TerminalView>>,
    search_input: Entity<InputState>,
    search_query: Entity<String>,
    pub(crate) editing_connection_id: Option<i64>,
    selected_connection_id: Option<i64>,
    pub(crate) filtered_workspace_ids: HashSet<i64>,
    collapsed_workspaces: HashSet<i64>,
    pub(crate) workspace_filter_open: bool,
    workspace_filter_list: Option<Entity<ListState<WorkspaceFilterDelegate>>>,
    pub(crate) _subscriptions: Vec<Subscription>,
    /// 云同步服务
    cloud_sync_service: Arc<std::sync::RwLock<CloudSyncService>>,
    /// 云端加载错误信息
    cloud_error: Option<String>,
    /// 最近一次同步反馈，用于主界面显式展示
    sync_feedback: Option<SyncFeedback>,
    /// 手动排序时工作区的当前插入预览位置
    workspace_drop_preview: Option<WorkspaceDropPreview>,
    /// 手动排序时连接项的当前插入预览位置
    connection_drop_preview: Option<ConnectionDropPreview>,
    /// 手动排序时跨工作区移动连接的目标工作区
    connection_workspace_drop_target: Option<i64>,
    /// 当前处于拖拽中的连接 ID，仅用于渲染层隐藏源卡片
    dragging_connection_id: Option<i64>,
    /// 工作区拖拽预览的实际尺寸缓存
    workspace_drag_preview_sizes: HashMap<i64, DragPreviewSize>,
    /// 连接列表项拖拽预览的实际尺寸缓存
    connection_list_drag_preview_sizes: HashMap<i64, DragPreviewSize>,
    /// 连接卡片拖拽预览的实际尺寸缓存
    connection_card_drag_preview_sizes: HashMap<i64, DragPreviewSize>,
    /// 连接卡片最近一次渲染的实际边界
    connection_card_bounds: HashMap<i64, Bounds<Pixels>>,
    /// 各工作区连接卡片网格最近一次渲染的实际边界
    connection_grid_bounds: HashMap<Option<i64>, Bounds<Pixels>>,
    /// 是否正在同步
    syncing: bool,
    /// 同步期间收到的新同步请求
    sync_requested: bool,
    /// 待处理的同步冲突
    pending_conflicts: Vec<SyncConflict>,
    /// 认证服务
    auth_service: Arc<AuthService>,
    /// 当前登录用户
    current_user: Option<UserInfo>,
    /// 是否正在登录
    logging_in: bool,
    /// 认证错误消息（登录/注册失败时设置）
    auth_error: Option<String>,
    /// 待处理的连接恢复快照
    pending_connection_restore_snapshot: Option<ConnectionRestoreSnapshot>,
    /// 恢复标签时保存的原始活动标签索引（用于跳过恢复后恢复该标签）
    saved_active_tab_index: Option<usize>,
    /// 工作区是否已完成初次加载
    workspaces_loaded: bool,
    /// 连接是否已完成初次加载
    connections_loaded: bool,
    /// 恢复提示是否已经弹出
    connection_restore_prompt_opened: bool,
    /// 首页内容滚动位置
    scroll_handle: ScrollHandle,
}

impl HomePage {
    pub fn new(
        tab_container: Entity<TabContainer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let settings = AppSettings::global(cx).clone();
        let search_query = cx.new(|_| String::new());
        let search_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("Home.search_placeholder"))
                .clean_on_escape()
        });

        // 订阅搜索输入变化
        let query_clone = search_query.clone();
        cx.subscribe_in(
            &search_input,
            window,
            move |_this, _input, event, _window, cx| {
                if let InputEvent::Change = event {
                    query_clone.update(cx, |q, cx| {
                        *q = _input.read(cx).text().to_string();
                        cx.notify();
                    });
                    cx.notify();
                }
            },
        )
        .detach();

        let mut page = Self {
            focus_handle: cx.focus_handle(),
            selected_filter: ConnectionType::All,
            workspaces: Vec::new(),
            connections: Vec::new(),
            tab_container,
            terminal_views: Vec::new(),
            search_input,
            search_query,
            editing_connection_id: None,
            selected_connection_id: None,
            filtered_workspace_ids: HashSet::new(),
            collapsed_workspaces: HashSet::new(),
            workspace_filter_open: false,
            workspace_filter_list: None,
            _subscriptions: Vec::new(),
            cloud_sync_service: Arc::new(std::sync::RwLock::new(CloudSyncService::new())),
            cloud_error: None,
            sync_feedback: None,
            workspace_drop_preview: None,
            connection_drop_preview: None,
            connection_workspace_drop_target: None,
            dragging_connection_id: None,
            workspace_drag_preview_sizes: HashMap::new(),
            connection_list_drag_preview_sizes: HashMap::new(),
            connection_card_drag_preview_sizes: HashMap::new(),
            connection_card_bounds: HashMap::new(),
            connection_grid_bounds: HashMap::new(),
            syncing: false,
            sync_requested: false,
            pending_conflicts: Vec::new(),
            auth_service: crate::auth::get_auth_service(cx),
            current_user: None,
            logging_in: false,
            auth_error: None,
            pending_connection_restore_snapshot: pending_connection_restore_snapshot(&settings),
            saved_active_tab_index: None,
            workspaces_loaded: false,
            connections_loaded: false,
            connection_restore_prompt_opened: false,
            scroll_handle: ScrollHandle::new(),
        };

        // 异步加载工作区
        page.load_workspaces(cx);

        // 尝试从存储后端恢复主密钥
        let key_restored = crypto::try_restore_master_key();
        if key_restored {
            tracing::info!("已恢复主密钥");
        } else if crypto::has_repo_password_set() {
            // 有验证文件但恢复失败，提示用户需要重新输入密钥
            tracing::warn!("密钥恢复失败，需要用户重新输入主密钥");
        } else {
            tracing::info!("首次使用，需要设置主密钥");
        }

        // 在恢复主密钥后再加载连接，避免解密阶段出现空密码
        page.load_connections(cx);

        // 尝试恢复登录会话
        page.try_restore_session(cx);

        // 订阅全局连接事件，当连接创建/更新时刷新列表并自动同步
        if let Some(notifier) = get_notifier(cx) {
            cx.subscribe(
                &notifier,
                |this, _, event: &ConnectionDataEvent, cx| match event {
                    ConnectionDataEvent::ConnectionCreated { connection } => {
                        // 立即将新连接添加到列表，避免异步加载的时序问题
                        this.connections.push(connection.clone());
                        cx.notify();
                        // 然后异步重新加载以确保数据一致性
                        this.load_connections(cx);
                        // 如果已登录且密钥已解锁，自动触发同步
                        if this.current_user.is_some() && crypto::has_master_key() {
                            tracing::info!("连接数据变化，自动触发云同步");
                            this.trigger_sync(cx);
                        }
                    }
                    ConnectionDataEvent::ConnectionUpdated { connection } => {
                        // 立即更新列表中的连接，避免异步加载的时序问题
                        if let Some(pos) =
                            this.connections.iter().position(|c| c.id == connection.id)
                        {
                            this.connections[pos] = connection.clone();
                        } else {
                            // 如果找不到，添加到列表
                            this.connections.push(connection.clone());
                        }
                        cx.notify();
                        // 然后异步重新加载以确保数据一致性
                        this.load_connections(cx);
                        // 如果已登录且密钥已解锁，自动触发同步
                        if this.current_user.is_some() && crypto::has_master_key() {
                            tracing::info!("连接数据变化，自动触发云同步");
                            this.trigger_sync(cx);
                        }
                    }
                    ConnectionDataEvent::ConnectionDeleted { connection_id } => {
                        // 立即从列表中移除连接
                        this.connections.retain(|c| c.id != Some(*connection_id));
                        cx.notify();
                        // 然后异步重新加载以确保数据一致性
                        this.load_connections(cx);
                        // 如果已登录且密钥已解锁，自动触发同步
                        if this.current_user.is_some() && crypto::has_master_key() {
                            tracing::info!("连接数据变化，自动触发云同步");
                            this.trigger_sync(cx);
                        }
                    }
                    ConnectionDataEvent::WorkspaceCreated { .. }
                    | ConnectionDataEvent::WorkspaceUpdated { .. }
                    | ConnectionDataEvent::WorkspaceDeleted { .. } => {
                        this.load_workspaces(cx);
                        // 如果已登录且密钥已解锁，自动触发同步
                        if this.current_user.is_some() && crypto::has_master_key() {
                            tracing::info!("工作区数据变化，自动触发云同步");
                            this.trigger_sync(cx);
                        }
                    }
                    ConnectionDataEvent::SchemaChanged { .. } => {
                        // SchemaChanged 由 db_tree_view 处理，此处无需操作
                    }
                },
            )
            .detach();
        }

        page
    }

    fn load_workspaces(&mut self, cx: &mut Context<Self>) {
        let storage = cx.global::<GlobalStorageState>().storage.clone();
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = (|| {
                let repo = storage
                    .get::<WorkspaceRepository>()
                    .ok_or_else(|| anyhow::anyhow!("WorkspaceRepository not found"))?;
                repo.list()
            })();

            match result {
                Ok(workspaces) => {
                    _ = this.update(cx, |this, cx| {
                        this.workspaces = workspaces;
                        this.workspaces_loaded = true;
                        cx.notify();
                    });
                }
                Err(e) => {
                    tracing::error!("Task join error: {}", e);
                }
            }
        })
        .detach();
    }

    fn load_connections(&mut self, cx: &mut Context<Self>) {
        let storage = cx.global::<GlobalStorageState>().storage.clone();
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = (|| {
                let repo = storage
                    .get::<ConnectionRepository>()
                    .ok_or_else(|| anyhow::anyhow!("ConnectionRepository not found"))?;
                repo.list()
            })();

            match result {
                Ok(connections) => {
                    _ = this.update(cx, |this, cx| {
                        this.connections = connections;
                        this.connections_loaded = true;
                        cx.notify();
                    });
                }
                Err(e) => {
                    tracing::error!("Task join error: {}", e);
                }
            }
        })
        .detach();
    }

    fn refresh_local_home_data(&mut self, cx: &mut Context<Self>) {
        self.load_workspaces(cx);
        self.load_connections(cx);
        let settings = crate::setting_tab::AppSettings::reload_global_from_disk(cx);

        cx.defer(move |cx| {
            let Some(home) = cx.try_global::<crate::onetcli_app::GlobalHomePage>() else {
                return;
            };
            let Some(window_id) = cx.active_window() else {
                return;
            };

            let home_page = home.home_page.clone();
            let _ = cx.update_window(window_id, move |_, window, cx| {
                home_page.update(cx, |home_page, cx| {
                    home_page.apply_app_settings(&settings, window, cx);
                });
            });
        });
    }

    fn maybe_prompt_connection_restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.connection_restore_prompt_opened
            || !self.workspaces_loaded
            || !self.connections_loaded
        {
            return;
        }

        let Some(snapshot) = self.pending_connection_restore_snapshot.clone() else {
            return;
        };

        let mut resolved_items =
            resolve_restore_items(&snapshot, &self.connections, &self.workspaces);
        crate::connection_restore::probe_pty_sessions(&mut resolved_items);
        if resolved_items.is_empty() {
            // 避免在 render 阶段直接清理状态，延后到窗口事件循环中执行。
            self.connection_restore_prompt_opened = true;
            let home_page = cx.entity();
            window.defer(cx, move |window, cx| {
                let _ = home_page.update(cx, |home, cx| {
                    home.skip_pending_connection_restore(window, cx);
                });
            });
            return;
        }

        self.connection_restore_prompt_opened = true;
        let home_page = cx.entity();
        window.defer(cx, move |window, cx| {
            open_connection_restore_dialog(home_page, resolved_items, window, cx);
        });
    }

    /// 仅清理连接恢复快照状态，不涉及标签页操作
    fn clear_connection_restore_state(&mut self) {
        self.pending_connection_restore_snapshot = None;
        self.connection_restore_prompt_opened = false;
        crate::connection_restore::clear_pending_connection_restore_snapshot();
    }

    pub(crate) fn skip_pending_connection_restore(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ref snapshot) = self.pending_connection_restore_snapshot {
            self.kill_skipped_pty_sessions(snapshot, None);
        }
        self.clear_connection_restore_state();

        // 跳过时恢复到标签恢复前的原始活动标签
        if let Some(index) = self.saved_active_tab_index {
            self.tab_container
                .update(cx, |tc, cx| tc.set_active_index(index, window, cx));
        }

        cx.notify();
    }

    /// 检查是否存在待处理的连接恢复快照
    pub fn has_pending_connection_restore_snapshot(&self) -> bool {
        self.pending_connection_restore_snapshot.is_some()
    }

    pub(crate) fn set_saved_active_tab_index(&mut self, index: Option<usize>) {
        self.saved_active_tab_index = index;
    }

    pub(crate) fn restore_saved_connection_sessions(
        &mut self,
        selected_snapshot_ids: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = self.pending_connection_restore_snapshot.clone() else {
            return;
        };

        let selected_snapshot_ids = selected_snapshot_ids
            .iter()
            .cloned()
            .collect::<HashSet<_>>();

        self.kill_skipped_pty_sessions(&snapshot, Some(&selected_snapshot_ids));

        let resolved_items = resolve_restore_items(&snapshot, &self.connections, &self.workspaces);

        // 仅清理状态，不涉及标签页操作（避免嵌套 update 导致 panic）
        self.clear_connection_restore_state();

        for item in resolved_items
            .into_iter()
            .filter(|item| selected_snapshot_ids.contains(&item.snapshot_id))
        {
            self.restore_connection_restore_item(item, window, cx);
        }

        cx.notify();
    }

    fn kill_skipped_pty_sessions(
        &self,
        snapshot: &one_core::connection_restore::ConnectionRestoreSnapshot,
        selected_ids: Option<&std::collections::HashSet<String>>,
    ) {
        use one_core::connection_restore::ConnectionRestoreKind;
        let session_ids: Vec<String> = snapshot
            .items
            .iter()
            .filter(|item| item.kind == ConnectionRestoreKind::LocalTerminal)
            .filter(|item| selected_ids.map_or(true, |ids| !ids.contains(&item.snapshot_id)))
            .filter_map(|item| item.local_terminal.as_ref()?.pty_session_id.clone())
            .collect();
        if !session_ids.is_empty() {
            terminal::kill_detached_sessions(session_ids);
        }
    }

    fn restore_connection_without_session(
        &mut self,
        item: ResolvedConnectionRestoreItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match item.kind {
            ConnectionRestoreKind::LocalTerminal => {
                let _ = item.local_terminal;
                self.open_local_terminal(None, "home", window, cx);
            }
            ConnectionRestoreKind::SshTerminal => {
                if let Some(connection) = item.connection {
                    self.open_ssh_terminal(connection, window, cx);
                } else {
                    tracing::warn!("恢复 SSH 终端时缺少连接信息");
                }
            }
            ConnectionRestoreKind::SerialTerminal => {
                if let Some(connection) = item.connection {
                    self.open_serial_terminal(connection, window, cx);
                } else {
                    tracing::warn!("恢复串口终端时缺少连接信息");
                }
            }
            ConnectionRestoreKind::Sftp => {
                if let Some(connection) = item.connection {
                    self.open_sftp_view(connection, window, cx);
                } else {
                    tracing::warn!("恢复 SFTP 时缺少连接信息");
                }
            }
            ConnectionRestoreKind::Database => {
                if let Some(connection) = item.connection {
                    self.add_item_to_tab(&connection, None, window, cx);
                } else {
                    tracing::warn!("恢复数据库页时缺少连接信息");
                }
            }
            ConnectionRestoreKind::DatabaseWorkspace => {
                if let Some(connection) = item.connection {
                    self.add_item_to_tab(&connection, item.workspace, window, cx);
                } else {
                    tracing::warn!("恢复数据库工作区时缺少连接信息");
                }
            }
            ConnectionRestoreKind::Redis => {
                if let Some(connection) = item.connection {
                    self.open_redis_tab(connection, None, window, cx);
                } else {
                    tracing::warn!("恢复 Redis 页时缺少连接信息");
                }
            }
            ConnectionRestoreKind::RedisWorkspace => {
                if let Some(connection) = item.connection {
                    self.open_redis_tab(connection, item.workspace, window, cx);
                } else {
                    tracing::warn!("恢复 Redis 工作区时缺少连接信息");
                }
            }
            ConnectionRestoreKind::MongoDb => {
                if let Some(connection) = item.connection {
                    self.open_mongodb_tab(connection, None, window, cx);
                } else {
                    tracing::warn!("恢复 MongoDB 页时缺少连接信息");
                }
            }
            ConnectionRestoreKind::MongoDbWorkspace => {
                if let Some(connection) = item.connection {
                    self.open_mongodb_tab(connection, item.workspace, window, cx);
                } else {
                    tracing::warn!("恢复 MongoDB 工作区时缺少连接信息");
                }
            }
        }
    }

    fn restore_connection_restore_item(
        &mut self,
        item: ResolvedConnectionRestoreItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !AppSettings::global(cx).restore_session_content {
            self.restore_connection_without_session(item, window, cx);
            return;
        }

        let ResolvedConnectionRestoreItem {
            kind,
            connection,
            workspace,
            active_connection_id,
            local_terminal,
            ssh_terminal,
            ..
        } = item;

        match kind {
            ConnectionRestoreKind::LocalTerminal => {
                if let Some(local_terminal) = local_terminal {
                    self.restore_local_terminal(local_terminal, window, cx);
                } else {
                    tracing::warn!("恢复本地终端时缺少本地终端状态");
                }
            }
            ConnectionRestoreKind::SshTerminal => {
                if let Some(connection) = connection {
                    self.open_ssh_terminal_with_state(
                        connection,
                        ssh_terminal.as_ref(),
                        window,
                        cx,
                    );
                } else {
                    tracing::warn!("恢复 SSH 终端时缺少连接信息");
                }
            }
            ConnectionRestoreKind::SerialTerminal => {
                if let Some(connection) = connection {
                    self.open_serial_terminal(connection, window, cx);
                } else {
                    tracing::warn!("恢复串口终端时缺少连接信息");
                }
            }
            ConnectionRestoreKind::Sftp => {
                if let Some(connection) = connection {
                    self.open_sftp_view(connection, window, cx);
                } else {
                    tracing::warn!("恢复 SFTP 时缺少连接信息");
                }
            }
            ConnectionRestoreKind::Database => {
                if let Some(connection) = connection {
                    self.restore_database_tab(
                        &connection,
                        None,
                        false,
                        active_connection_id,
                        window,
                        cx,
                    );
                } else {
                    tracing::warn!("恢复数据库页时缺少连接信息");
                }
            }
            ConnectionRestoreKind::DatabaseWorkspace => {
                if let Some(connection) = connection {
                    self.restore_database_tab(
                        &connection,
                        workspace,
                        true,
                        active_connection_id,
                        window,
                        cx,
                    );
                } else {
                    tracing::warn!("恢复数据库工作区时缺少连接信息");
                }
            }
            ConnectionRestoreKind::Redis => {
                if let Some(connection) = connection {
                    self.restore_redis_tab(
                        connection,
                        None,
                        false,
                        active_connection_id,
                        window,
                        cx,
                    );
                } else {
                    tracing::warn!("恢复 Redis 页时缺少连接信息");
                }
            }
            ConnectionRestoreKind::RedisWorkspace => {
                if let Some(connection) = connection {
                    self.restore_redis_tab(
                        connection,
                        workspace,
                        true,
                        active_connection_id,
                        window,
                        cx,
                    );
                } else {
                    tracing::warn!("恢复 Redis 工作区时缺少连接信息");
                }
            }
            ConnectionRestoreKind::MongoDb => {
                if let Some(connection) = connection {
                    self.restore_mongodb_tab(
                        connection,
                        None,
                        false,
                        active_connection_id,
                        window,
                        cx,
                    );
                } else {
                    tracing::warn!("恢复 MongoDB 页时缺少连接信息");
                }
            }
            ConnectionRestoreKind::MongoDbWorkspace => {
                if let Some(connection) = connection {
                    self.restore_mongodb_tab(
                        connection,
                        workspace,
                        true,
                        active_connection_id,
                        window,
                        cx,
                    );
                } else {
                    tracing::warn!("恢复 MongoDB 工作区时缺少连接信息");
                }
            }
        }
    }

    fn queue_pending_cloud_deletion(
        storage: &one_core::storage::StorageManager,
        cloud_id: Option<&str>,
        entity_type: &str,
    ) {
        let Some(cloud_id) = cloud_id else {
            return;
        };

        let Some(pending_repo) = storage.get::<PendingCloudDeletionRepository>() else {
            tracing::error!(
                "[删除] 无法记录待删除{}：PendingCloudDeletionRepository 不存在",
                entity_type
            );
            return;
        };

        match pending_repo.add(cloud_id, entity_type) {
            Ok(()) => {
                tracing::info!(
                    "[删除] 已登记待删除{}，等待同步引擎处理: {}",
                    entity_type,
                    cloud_id
                );
            }
            Err(error) => {
                tracing::error!(
                    "[删除] 记录待删除{}失败: {} - {}",
                    entity_type,
                    cloud_id,
                    error
                );
            }
        }
    }

    fn queue_pending_workspace_deletion(
        storage: &one_core::storage::StorageManager,
        workspace: &Workspace,
        affected_connections: &[StoredConnection],
    ) {
        let Some(cloud_id) = workspace.cloud_id.as_deref() else {
            return;
        };

        let Some(pending_repo) = storage.get::<PendingCloudDeletionRepository>() else {
            tracing::error!("[删除] 无法记录待删除工作空间：PendingCloudDeletionRepository 不存在");
            return;
        };

        let metadata = PendingCloudDeletionMetadata {
            workspace_local_id: workspace.id,
            affected_connection_ids: affected_connections
                .iter()
                .filter_map(|connection| connection.id)
                .collect(),
        };

        match pending_repo.add_with_context(
            cloud_id,
            "workspace",
            workspace.last_synced_at,
            Some(&metadata),
        ) {
            Ok(()) => {
                tracing::info!(
                    "[删除] 已登记待删除工作空间，等待同步引擎处理: {}",
                    cloud_id
                );
            }
            Err(error) => {
                tracing::error!("[删除] 记录待删除工作空间失败: {} - {}", cloud_id, error);
            }
        }
    }

    fn clear_auth_related_state(&mut self, cx: &mut Context<Self>) {
        self.current_user = None;
        self.logging_in = false;
        self.auth_error = None;
        self.cloud_error = None;
        self.sync_feedback = None;
        self.pending_conflicts.clear();
        self.syncing = false;
        self.sync_requested = false;
        GlobalCurrentUser::set_user(None, cx);

        if let Ok(mut service) = self.cloud_sync_service.write() {
            service.logout();
        } else {
            tracing::warn!("同步地址变更后重置云同步状态失败：无法获取写锁");
        }

        cx.notify();
    }

    pub(crate) fn handle_auth_state_cleared(&mut self, cx: &mut Context<Self>) {
        self.clear_auth_related_state(cx);
    }

    pub(crate) fn handle_auth_state_restored(&mut self, user: UserInfo, cx: &mut Context<Self>) {
        self.current_user = Some(user.clone());
        self.logging_in = false;
        self.auth_error = None;
        self.cloud_error = None;
        GlobalCurrentUser::set_user(Some(user), cx);
        cx.notify();
    }

    pub(crate) fn handle_sync_server_url_changed(&mut self, cx: &mut Context<Self>) {
        self.clear_auth_related_state(cx);
    }

    fn set_sync_feedback(&mut self, level: SyncFeedbackLevel, message: impl Into<String>) {
        self.sync_feedback = Some(SyncFeedback {
            level,
            message: message.into(),
        });
    }

    fn summarize_sync_result(result: &one_core::cloud_sync::SyncResult) -> SyncFeedback {
        let mut summary_parts = Vec::new();
        if result.uploaded > 0 {
            summary_parts.push(format!("上传 {} 项", result.uploaded));
        }
        if result.downloaded > 0 {
            summary_parts.push(format!("下载 {} 项", result.downloaded));
        }
        if result.deleted > 0 {
            summary_parts.push(format!("删除 {} 项", result.deleted));
        }

        let summary = if summary_parts.is_empty() {
            t!("Home.sync_completed_no_changes").to_string()
        } else {
            t!(
                "Home.sync_completed_summary",
                summary = summary_parts.join("，")
            )
            .to_string()
        };

        let mut issues = Vec::new();
        if !result.conflicts.is_empty() {
            issues
                .push(t!("Home.sync_issues_conflicts", count = result.conflicts.len()).to_string());
        }
        if !result.errors.is_empty() {
            issues.push(
                t!(
                    "Home.sync_issues_errors",
                    count = result.errors.len(),
                    error = result.errors[0].as_str()
                )
                .to_string(),
            );
        }

        if issues.is_empty() {
            let level = if summary_parts.is_empty() {
                SyncFeedbackLevel::Info
            } else {
                SyncFeedbackLevel::Success
            };
            SyncFeedback {
                level,
                message: summary,
            }
        } else {
            SyncFeedback {
                level: SyncFeedbackLevel::Warning,
                message: t!(
                    "Home.sync_completed_with_issues",
                    summary = summary,
                    issues = issues.join("；")
                )
                .to_string(),
            }
        }
    }

    fn build_conflict_resolution_feedback(
        result: &one_core::cloud_sync::SyncResult,
    ) -> SyncFeedback {
        if result.errors.is_empty() {
            SyncFeedback {
                level: SyncFeedbackLevel::Success,
                message: t!("Home.sync_conflicts_resolved").to_string(),
            }
        } else {
            SyncFeedback {
                level: SyncFeedbackLevel::Warning,
                message: t!(
                    "Home.sync_conflicts_resolved_with_issues",
                    count = result.errors.len(),
                    error = result.errors[0].as_str()
                )
                .to_string(),
            }
        }
    }

    fn push_sync_notification(feedback: &SyncFeedback, cx: &mut Context<Self>) {
        let notification = match feedback.level {
            SyncFeedbackLevel::Info => {
                gpui_component::notification::Notification::info(feedback.message.clone())
            }
            SyncFeedbackLevel::Success => {
                gpui_component::notification::Notification::success(feedback.message.clone())
            }
            SyncFeedbackLevel::Warning => {
                // 同步警告通常伴随潜在问题，除了通知外还在日志中显式输出以引起注意
                // tracing::warn!(target: "sync", "{}", feedback.message);
                gpui_component::notification::Notification::warning(feedback.message.clone())
            }
            SyncFeedbackLevel::Error => {
                // 同步错误通常伴随严重问题，除了通知外还在日志中显式输出以引起注意
                // tracing::error!(target: "sync", "{}", feedback.message);
                gpui_component::notification::Notification::error(feedback.message.clone())
            }
        }
        .title(t!("Home.sync"))
        .id::<SyncResultNotification>();

        if let Some(window_id) = cx.active_window() {
            let _ = cx.update_window(window_id, move |_, window, cx| {
                window.push_notification(notification, cx);
            });
        }
    }

    fn github_gist_vault(settings: &AppSettings, cx: &App) -> Option<Arc<dyn BlobVault>> {
        let gist_cfg = settings.gist_config.as_ref()?;
        if gist_cfg.client_id.is_empty() {
            return None;
        }

        let mut vault = GithubGistVault::new(cx.http_client());
        if let Some(gist_id) = gist_cfg.gist_id.clone().filter(|id| !id.is_empty()) {
            vault = vault.with_gist_id(gist_id);
        }
        if let Some(tokens) = gist_cfg.tokens.clone() {
            vault = vault.with_tokens(tokens);
        }

        Some(Arc::new(vault))
    }

    fn google_drive_vault(settings: &AppSettings, cx: &App) -> Option<Arc<dyn BlobVault>> {
        let gd_cfg = settings.google_drive_config.as_ref()?;
        if gd_cfg.client_id.is_empty() || gd_cfg.client_secret.is_empty() {
            return None;
        }

        let vault = GoogleDriveVault::new(cx.http_client());
        let vault: Arc<GoogleDriveVault> = if let Some(tokens) = gd_cfg.tokens.clone() {
            vault.with_tokens(tokens)
        } else {
            Arc::new(vault)
        };
        if let Some(folder_id) = gd_cfg.folder_id.clone() {
            let wrapped = (*vault).with_folder_id(folder_id);
            return Some(wrapped);
        }
        Some(vault)
    }

    fn onedrive_vault(settings: &AppSettings, cx: &App) -> Option<Arc<dyn BlobVault>> {
        let od_cfg = settings.onedrive_config.as_ref()?;
        if od_cfg.client_id.is_empty() {
            return None;
        }

        let inner = OneDriveVault::new(cx.http_client());
        if let Some(tokens) = od_cfg.tokens.clone() {
            Some(inner.with_tokens(tokens))
        } else {
            Some(Arc::new(inner))
        }
    }

    /// 触发云端同步
    ///
    /// 使用 SyncEngine 执行同步，包括：
    /// 1. 检查密钥状态，如果未解锁则自动弹出输入对话框
    /// 2. 计算同步计划（上传、下载、冲突检测）
    /// 3. 执行同步操作
    /// 4. 更新本地状态
    fn trigger_sync(&mut self, cx: &mut Context<Self>) {
        let backend_type = AppSettings::global(cx).sync_backend_type.clone();

        // sync_server 后端需要校验 URL 和登录状态
        if backend_type == "sync_server" {
            if !self.auth_service.has_valid_sync_server_url() {
                let message = t!("Home.sync_server_url_required").to_string();
                self.cloud_error = Some(message.clone());
                self.set_sync_feedback(SyncFeedbackLevel::Warning, message);
                if let Some(feedback) = &self.sync_feedback {
                    Self::push_sync_notification(feedback, cx);
                }
                cx.notify();
                return;
            }

            if self.current_user.is_none() {
                let message = t!("Home.cloud_need_login").to_string();
                self.cloud_error = Some(message.clone());
                self.set_sync_feedback(SyncFeedbackLevel::Warning, message);
                if let Some(feedback) = &self.sync_feedback {
                    Self::push_sync_notification(feedback, cx);
                }
                cx.notify();
                return;
            }
        }

        if !self.pending_conflicts.is_empty() {
            let message = t!(
                "Home.conflict_tooltip",
                count = self.pending_conflicts.len()
            )
            .to_string();
            self.cloud_error = Some(message.clone());
            self.set_sync_feedback(SyncFeedbackLevel::Warning, message);
            if let Some(feedback) = &self.sync_feedback {
                Self::push_sync_notification(feedback, cx);
            }
            cx.notify();
            return;
        }

        let storage = cx.global::<GlobalStorageState>().storage.clone();
        self.log_sync_decrypt_health(&storage, "常规同步");

        if self.syncing {
            self.sync_requested = true;
            return;
        }

        self.syncing = true;
        self.sync_requested = false;
        self.cloud_error = None;
        self.set_sync_feedback(SyncFeedbackLevel::Info, t!("Home.syncing").to_string());
        cx.notify();

        let cloud_client = self.auth_service.cloud_client();
        let sync_service = self.cloud_sync_service.clone();

        if let Some(user) = &self.current_user {
            if let Ok(mut service) = sync_service.write() {
                service.set_logged_in(user.id.clone());
            } else {
                tracing::warn!("同步前设置用户ID失败：无法获取云同步服务写锁");
            }
        }

        // 创建同步引擎
        let settings = AppSettings::global(cx);
        let backend_type = settings.sync_backend_type.clone();
        let backend = one_core::cloud_sync::create_backend(&backend_type);
        let engine = match backend_type.as_str() {
            "github_gist" => {
                let vault = Self::github_gist_vault(&settings, cx);
                SyncEngine::new(cloud_client, sync_service, storage)
                    .with_backend(backend)
                    .with_opt_blob_vault(vault)
            }
            "google_drive" => {
                let vault = Self::google_drive_vault(&settings, cx);
                SyncEngine::new(cloud_client, sync_service, storage)
                    .with_backend(backend)
                    .with_opt_blob_vault(vault)
            }
            "onedrive" => {
                let vault = Self::onedrive_vault(&settings, cx);
                SyncEngine::new(cloud_client, sync_service, storage)
                    .with_backend(backend)
                    .with_opt_blob_vault(vault)
            }
            "webdav" => SyncEngine::new(cloud_client, sync_service, storage).with_backend(backend),
            _ => SyncEngine::new(cloud_client, sync_service, storage).with_backend(backend),
        };

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = engine.sync().await;

            _ = this.update(cx, |this, cx| {
                this.syncing = false;
                let sync_requested = this.sync_requested;
                match result {
                    Ok(stats) => {
                        let feedback = Self::summarize_sync_result(&stats);
                        tracing::info!(
                            "同步完成：上传 {} 个，下载 {} 个，冲突 {} 个",
                            stats.uploaded,
                            stats.downloaded,
                            stats.conflicts.len()
                        );
                        this.cloud_error = None;

                        // 如果有冲突，保存并显示冲突解决对话框
                        if !stats.conflicts.is_empty() {
                            tracing::warn!("同步存在 {} 个冲突需要处理", stats.conflicts.len());
                            this.pending_conflicts = stats.conflicts;
                        }

                        // 如果有错误，显示第一个错误
                        if !stats.errors.is_empty() {
                            this.cloud_error = Some(stats.errors.join("; "));
                        }

                        this.sync_feedback = Some(feedback.clone());
                        Self::push_sync_notification(&feedback, cx);

                        // 刷新首页本地数据，确保部分失败时界面仍与已落库数据一致
                        this.refresh_local_home_data(cx);
                    }
                    Err(e) => {
                        tracing::error!("同步失败: {}", e);
                        let message = format!("{}：{}", t!("Home.sync_failed"), e);
                        this.cloud_error = Some(e.to_string());
                        this.set_sync_feedback(SyncFeedbackLevel::Error, message);
                        if let Some(feedback) = &this.sync_feedback {
                            Self::push_sync_notification(feedback, cx);
                        }
                    }
                }
                if sync_requested && this.pending_conflicts.is_empty() && this.cloud_error.is_none()
                {
                    this.sync_requested = false;
                    this.trigger_sync(cx);
                } else {
                    this.sync_requested = false;
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// 同步前记录本地连接解密状态，用于提示哪些连接会被引擎按连接粒度跳过。
    fn log_sync_decrypt_health(&self, storage: &one_core::storage::StorageManager, scene: &str) {
        if let Some(repo) = storage.get::<ConnectionRepository>() {
            match repo.list_sync_decrypt_failures() {
                Ok(failures) if !failures.is_empty() => {
                    let preview = failures
                        .iter()
                        .take(5)
                        .map(|(id, name)| format!("{}:{}", id, name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    tracing::warn!(
                        "{}检测到 {} 个连接解密失败：将由同步引擎跳过这些连接，其它连接继续同步和拉取。失败连接: {}",
                        scene,
                        failures.len(),
                        preview
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("{}前解密状态检查失败，将继续执行同步流程: {}", scene, e);
                }
            }
        } else {
            tracing::warn!(
                "{}前解密状态检查失败：ConnectionRepository 不存在，将继续执行同步流程",
                scene
            );
        }
    }

    /// 显示冲突解决对话框
    fn show_conflict_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_conflicts.is_empty() {
            return;
        }

        let conflicts = self.pending_conflicts.clone();
        let view = cx.entity().clone();

        // 为每个冲突创建默认策略（使用建议的策略）
        let mut default_strategies = std::collections::HashMap::new();
        for conflict in &conflicts {
            let suggested = match conflict.conflict_type {
                one_core::cloud_sync::ConflictType::BothModified => ConflictResolution::KeepBoth,
                one_core::cloud_sync::ConflictType::LocalDeletedCloudModified => {
                    ConflictResolution::UseCloud
                }
                one_core::cloud_sync::ConflictType::LocalModifiedCloudDeleted => {
                    ConflictResolution::UseLocal
                }
            };
            default_strategies.insert(conflict.cloud.id.clone(), suggested);
        }

        // 创建策略选择状态
        let strategies = cx.new(|_| default_strategies);

        window.open_dialog(cx, move |dialog, _window, cx| {
            let conflicts_count = conflicts.len();
            let conflict_items: Vec<AnyElement> = conflicts
                .iter()
                .map(|conflict| {
                    let local_name = conflict.local.name.clone();
                    let conflict_type = format!("{}", conflict.conflict_type);
                    let cloud_id = conflict.cloud.id.clone();
                    let strategies_clone = strategies.clone();

                    // 获取当前选择的策略
                    let current_strategy = strategies
                        .read(cx)
                        .get(&cloud_id)
                        .copied()
                        .unwrap_or(ConflictResolution::UseCloud);

                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .p_3()
                        .bg(gpui::hsla(0.0, 0.0, 0.5, 0.1))
                        .rounded_md()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("📄 {}", local_name)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(gpui::hsla(0.0, 0.0, 0.5, 1.0))
                                .child(
                                    t!("Home.sync_conflict_type", conflict_type = conflict_type)
                                        .to_string(),
                                ),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .mt_2()
                                .child(
                                    Button::new(ElementId::Name(
                                        format!("use_cloud_{}", cloud_id).into(),
                                    ))
                                    .label(t!("Home.sync_conflict_use_cloud"))
                                    .with_variant(
                                        if current_strategy == ConflictResolution::UseCloud {
                                            ButtonVariant::Primary
                                        } else {
                                            ButtonVariant::Ghost
                                        },
                                    )
                                    .xsmall()
                                    .on_click({
                                        let cloud_id = cloud_id.clone();
                                        let strategies = strategies_clone.clone();
                                        move |_, _, cx| {
                                            strategies.update(cx, |s, cx| {
                                                s.insert(
                                                    cloud_id.clone(),
                                                    ConflictResolution::UseCloud,
                                                );
                                                cx.notify();
                                            });
                                        }
                                    }),
                                )
                                .child(
                                    Button::new(ElementId::Name(
                                        format!("use_local_{}", cloud_id).into(),
                                    ))
                                    .label(t!("Home.sync_conflict_use_local"))
                                    .with_variant(
                                        if current_strategy == ConflictResolution::UseLocal {
                                            ButtonVariant::Primary
                                        } else {
                                            ButtonVariant::Ghost
                                        },
                                    )
                                    .xsmall()
                                    .on_click({
                                        let cloud_id = cloud_id.clone();
                                        let strategies = strategies_clone.clone();
                                        move |_, _, cx| {
                                            strategies.update(cx, |s, cx| {
                                                s.insert(
                                                    cloud_id.clone(),
                                                    ConflictResolution::UseLocal,
                                                );
                                                cx.notify();
                                            });
                                        }
                                    }),
                                )
                                .child(
                                    Button::new(ElementId::Name(
                                        format!("keep_both_{}", cloud_id).into(),
                                    ))
                                    .label(t!("Home.sync_conflict_keep_both"))
                                    .with_variant(
                                        if current_strategy == ConflictResolution::KeepBoth {
                                            ButtonVariant::Primary
                                        } else {
                                            ButtonVariant::Ghost
                                        },
                                    )
                                    .xsmall()
                                    .on_click({
                                        let strategies = strategies_clone.clone();
                                        move |_, _, cx| {
                                            strategies.update(cx, |s, cx| {
                                                s.insert(
                                                    cloud_id.clone(),
                                                    ConflictResolution::KeepBoth,
                                                );
                                                cx.notify();
                                            });
                                        }
                                    }),
                                ),
                        )
                        .into_any_element()
                })
                .collect();

            let view_clone = view.clone();
            let strategies_for_ok = strategies.clone();

            dialog
                .title(
                    t!("Home.sync_conflict_dialog_title", count = conflicts_count)
                        .to_string()
                        .into_any_element(),
                )
                .child(
                    div()
                        .id("conflict_items")
                        .flex()
                        .flex_col()
                        .gap_3()
                        .max_h(px(400.0))
                        .overflow_y_scroll()
                        .children(conflict_items)
                        .into_any_element(),
                )
                .confirm()
                .button_props(
                    gpui_component::dialog::DialogButtonProps::default()
                        .ok_text(t!("Home.sync_conflict_apply_strategy")),
                )
                .on_ok(move |_event, _window, cx| {
                    let selected_strategies = strategies_for_ok.read(cx).clone();
                    view_clone.update(cx, |this, cx| {
                        this.resolve_conflicts_individually(selected_strategies, cx);
                    });
                    true
                })
        });
    }

    /// 使用单独的策略解决每个冲突
    fn resolve_conflicts_individually(
        &mut self,
        strategies: std::collections::HashMap<String, ConflictResolution>,
        cx: &mut Context<Self>,
    ) {
        if self.pending_conflicts.is_empty() {
            return;
        }

        tracing::info!("使用单独策略解决 {} 个冲突", self.pending_conflicts.len());

        if self.syncing {
            self.sync_requested = true;
            return;
        }

        let conflicts = self.pending_conflicts.clone();
        let cloud_client = self.auth_service.cloud_client();
        let sync_service = self.cloud_sync_service.clone();

        if let Some(user) = &self.current_user {
            if let Ok(mut service) = sync_service.write() {
                service.set_logged_in(user.id.clone());
            } else {
                tracing::warn!("冲突解决前设置用户ID失败：无法获取云同步服务写锁");
            }
        }

        let storage = cx.global::<GlobalStorageState>().storage.clone();
        self.log_sync_decrypt_health(&storage, "单独冲突解决");
        self.syncing = true;
        self.sync_requested = false;
        self.cloud_error = None;
        cx.notify();

        // 创建同步引擎（复用同步设置的 blob vault 配置）
        let settings = AppSettings::global(cx);
        let backend_type = settings.sync_backend_type.clone();
        let backend = one_core::cloud_sync::create_backend(&backend_type);
        let engine = match backend_type.as_str() {
            "github_gist" => {
                let vault = Self::github_gist_vault(&settings, cx);
                SyncEngine::new(cloud_client, sync_service, storage)
                    .with_backend(backend)
                    .with_opt_blob_vault(vault)
            }
            "google_drive" => {
                let vault = Self::google_drive_vault(&settings, cx);
                SyncEngine::new(cloud_client, sync_service, storage)
                    .with_backend(backend)
                    .with_opt_blob_vault(vault)
            }
            "onedrive" => {
                let vault = Self::onedrive_vault(&settings, cx);
                SyncEngine::new(cloud_client, sync_service, storage)
                    .with_backend(backend)
                    .with_opt_blob_vault(vault)
            }
            "webdav" => SyncEngine::new(cloud_client, sync_service, storage).with_backend(backend),
            _ => SyncEngine::new(cloud_client, sync_service, storage).with_backend(backend),
        };

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            // 使用策略映射应用冲突解决方案
            let result = engine
                .apply_conflict_resolutions(conflicts, strategies)
                .await;

            _ = this.update(cx, |this, cx| {
                this.syncing = false;
                let sync_requested = this.sync_requested;
                match result {
                    Ok(stats) => {
                        let feedback = Self::build_conflict_resolution_feedback(&stats);
                        if stats.errors.is_empty() && stats.conflicts.is_empty() {
                            tracing::info!("冲突解决完成");
                            this.pending_conflicts.clear();
                        } else {
                            tracing::warn!(
                                "冲突解决未完全生效：剩余 {} 个未解决冲突，{} 个错误",
                                stats.conflicts.len(),
                                stats.errors.len()
                            );
                            this.pending_conflicts = stats.conflicts.clone();
                        }
                        this.cloud_error = if stats.errors.is_empty() {
                            None
                        } else {
                            Some(stats.errors.join("; "))
                        };
                        this.sync_feedback = Some(feedback.clone());
                        Self::push_sync_notification(&feedback, cx);
                        this.refresh_local_home_data(cx);
                    }
                    Err(e) => {
                        tracing::error!("冲突解决失败: {}", e);
                        let message = format!("{}：{}", t!("Home.sync_failed"), e);
                        this.cloud_error = Some(e.to_string());
                        this.set_sync_feedback(SyncFeedbackLevel::Error, message);
                        if let Some(feedback) = &this.sync_feedback {
                            Self::push_sync_notification(feedback, cx);
                        }
                    }
                }
                if sync_requested && this.pending_conflicts.is_empty() && this.cloud_error.is_none()
                {
                    this.sync_requested = false;
                    this.trigger_sync(cx);
                } else {
                    this.sync_requested = false;
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ========================================================================
    // 用户认证
    // ========================================================================

    /// 尝试从本地存储恢复会话
    fn try_restore_session(&mut self, cx: &mut Context<Self>) {
        let auth = self.auth_service.clone();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            if let Some(user) = auth.try_restore_session().await {
                _ = this.update(cx, |this, cx| {
                    this.current_user = Some(user.clone());
                    // 更新全局用户状态
                    GlobalCurrentUser::set_user(Some(user.clone()), cx);

                    cx.notify();

                    // 如果密钥已解锁，自动触发同步
                    if crypto::has_master_key() {
                        tracing::info!("会话已恢复且密钥已解锁，自动触发云同步");
                        this.trigger_sync(cx);
                    }
                });
            }
        })
        .detach();
    }

    /// 使用邮箱密码登录或注册
    /// 显示登录对话框
    fn show_login_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.auth_service.has_valid_sync_server_url() {
            let message = self.auth_service.sync_server_url_required_message();
            let view = cx.entity();
            window.open_dialog(cx, move |dialog, _window, _cx| {
                let view_for_ok = view.clone();
                dialog
                    .title(t!("Common.settings").to_string())
                    .child(message.clone().into_any_element())
                    .alert()
                    .on_ok(move |_, window, cx| {
                        _ = view_for_ok.update(cx, |this, cx| {
                            this.add_settings_tab(window, cx);
                        });
                        true
                    })
            });
            return;
        }

        // 引导用户前往设置页面的同步分组完成登录
        self.add_settings_tab(window, cx);
    }

    fn confirm_edit_connection(
        &mut self,
        conn_id: i64,
        conn_name: String,
        db_type: Option<DatabaseType>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_active = cx.global::<ActiveConnections>().is_active(conn_id);

        if is_active {
            window.open_dialog(cx, move |dialog, _window, _cx| {
                dialog
                    .title(t!("Connection.in_use_title").to_string().into_any_element())
                    .child(
                        t!("Connection.in_use_cannot_edit", conn_name = conn_name)
                            .to_string()
                            .into_any_element(),
                    )
                    .alert()
            });
        } else if let Some(db_type) = db_type {
            self.editing_connection_id = Some(conn_id);
            self.show_connection_form(db_type, window, cx);
        }
    }

    fn confirm_delete_connection(
        &mut self,
        conn_id: i64,
        conn_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_active = cx.global::<ActiveConnections>().is_active(conn_id);
        let view = cx.entity().clone();

        if is_active {
            window.open_dialog(cx, move |dialog, _window, _cx| {
                dialog
                    .title(t!("Connection.in_use_title").to_string().into_any_element())
                    .child(
                        t!("Connection.in_use_cannot_delete", conn_name = conn_name)
                            .to_string()
                            .into_any_element(),
                    )
                    .alert()
            });
        } else {
            window.open_dialog(cx, move |dialog, _window, _cx| {
                let view_clone = view.clone();
                dialog
                    .title(t!("Common.delete").to_string().into_any_element())
                    .child(
                        t!("Connection.delete_confirm", conn_name = conn_name)
                            .to_string()
                            .into_any_element(),
                    )
                    .confirm()
                    .on_ok(move |_, _, cx| {
                        let _ = view_clone.update(cx, |this, cx| {
                            this.delete_connection(conn_id, cx);
                        });
                        true
                    })
            });
        }
    }

    fn duplicate_connection_and_open_editor(
        &mut self,
        source: &StoredConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.ensure_master_key_ready_for_new_connection(window, cx) {
            return;
        }

        let storage = cx.global::<GlobalStorageState>().storage.clone();
        let Some(repo) = storage.get::<ConnectionRepository>() else {
            window.open_dialog(cx, move |dialog, _window, _cx| {
                dialog
                    .title(t!("Common.error_info").to_string().into_any_element())
                    .child(
                        t!(
                            "Home.duplicate_connection_failed",
                            error = "无法获取连接存储库"
                        )
                        .to_string()
                        .into_any_element(),
                    )
                    .alert()
            });
            return;
        };

        let mut duplicated = source.clone();
        duplicated.id = None;
        duplicated.sort_order = None;
        duplicated.cloud_id = None;
        duplicated.last_synced_at = None;
        duplicated.created_at = None;
        duplicated.updated_at = None;
        duplicated.owner_id = self
            .current_user
            .as_ref()
            .map(|user| user.id.clone())
            .or_else(|| source.owner_id.clone());

        match repo.insert(&mut duplicated) {
            Ok(_) => {
                self.connections.push(duplicated.clone());
                self.selected_connection_id = duplicated.id;
                self.load_connections(cx);
                cx.notify();
                self.open_existing_connection_editor(duplicated, window, cx);
            }
            Err(error) => {
                let error_message = t!(
                    "Home.duplicate_connection_failed",
                    error = error.to_string()
                )
                .to_string();
                window.open_dialog(cx, move |dialog, _window, _cx| {
                    dialog
                        .title(t!("Common.error_info").to_string().into_any_element())
                        .child(error_message.clone().into_any_element())
                        .alert()
                });
            }
        }
    }

    fn open_existing_connection_editor(
        &mut self,
        connection: StoredConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match connection.connection_type {
            ConnectionType::SshSftp => {
                let config = SshFormWindowConfig {
                    editing_connection: Some(connection),
                    workspaces: self.workspaces.clone(),
                };

                open_popup_window(
                    window,
                    PopupWindowOptions::new(t!("SSH.edit").to_string()).size(700.0, 650.0),
                    move |window, cx| cx.new(|cx| SshFormWindow::new(config, window, cx)),
                    cx,
                );
            }
            ConnectionType::Database => {
                let Some(db_type) = connection.to_db_connection().ok().map(|p| p.database_type)
                else {
                    window.open_dialog(cx, move |dialog, _window, _cx| {
                        dialog
                            .title(t!("Common.error_info").to_string().into_any_element())
                            .child(
                                t!(
                                    "Home.duplicate_connection_failed",
                                    error = "无法识别数据库连接类型"
                                )
                                .to_string()
                                .into_any_element(),
                            )
                            .alert()
                    });
                    return;
                };

                let config = ConnectionFormWindowConfig {
                    db_type,
                    external_driver_id: None,
                    editing_connection: Some(connection),
                    workspaces: self.workspaces.clone(),
                };

                open_popup_window(
                    window,
                    PopupWindowOptions::new(
                        t!("Connection.edit", db_type = db_type.as_str()).to_string(),
                    )
                    .size(700.0, 650.0),
                    move |window, cx| cx.new(|cx| ConnectionFormWindow::new(config, window, cx)),
                    cx,
                );
            }
            ConnectionType::Redis => {
                let config = RedisFormWindowConfig {
                    editing_connection: Some(connection),
                    workspaces: self.workspaces.clone(),
                };

                open_popup_window(
                    window,
                    PopupWindowOptions::new(t!("Connection.edit", db_type = "Redis").to_string())
                        .size(700.0, 650.0),
                    move |window, cx| cx.new(|cx| RedisFormWindow::new(config, window, cx)),
                    cx,
                );
            }
            ConnectionType::MongoDB => {
                let config = MongoFormWindowConfig {
                    editing_connection: Some(connection),
                    workspaces: self.workspaces.clone(),
                };

                open_popup_window(
                    window,
                    PopupWindowOptions::new(t!("Connection.edit", db_type = "MongoDB").to_string())
                        .size(700.0, 520.0),
                    move |window, cx| cx.new(|cx| MongoFormWindow::new(config, window, cx)),
                    cx,
                );
            }
            ConnectionType::Serial => {
                let config = SerialFormWindowConfig {
                    editing_connection: Some(connection),
                    workspaces: self.workspaces.clone(),
                };

                open_popup_window(
                    window,
                    PopupWindowOptions::new(t!("Serial.edit").to_string()).size(700.0, 650.0),
                    move |window, cx| cx.new(|cx| SerialFormWindow::new(config, window, cx)),
                    cx,
                );
            }
            _ => {}
        }
    }

    fn delete_connection(&mut self, conn_id: i64, cx: &mut Context<Self>) {
        let storage = cx.global::<GlobalStorageState>().storage.clone();

        let cloud_id = self
            .connections
            .iter()
            .find(|c| c.id == Some(conn_id))
            .and_then(|c| c.cloud_id.clone());

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = (|| {
                let repo = storage
                    .get::<ConnectionRepository>()
                    .ok_or_else(|| anyhow::anyhow!("ConnectionRepository not found"))?;
                repo.delete(conn_id)
            })();

            match result {
                Ok(_) => {
                    Self::queue_pending_cloud_deletion(&storage, cloud_id.as_deref(), "connection");
                    _ = this.update(cx, |this, cx| {
                        this.connections.retain(|c| c.id != Some(conn_id));
                        if this.selected_connection_id == Some(conn_id) {
                            this.selected_connection_id = None;
                        }
                        emit_connection_event(
                            ConnectionDataEvent::ConnectionDeleted {
                                connection_id: conn_id,
                            },
                            cx,
                        );
                        cx.notify();
                    });
                }
                Err(e) => {
                    tracing::error!("Failed to delete connection: {}", e);
                }
            }
        })
        .detach();
    }

    pub(crate) fn show_connection_quick_open(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let parent = cx.entity();
        let connections = self.connections.clone();
        let list = cx.new(|cx| {
            let mut delegate = ConnectionQuickOpenDelegate::new(parent);
            delegate.update_items(&connections);
            ListState::new(delegate, window, cx).searchable(true)
        });

        let list_for_focus = list.clone();
        window.open_dialog(cx, move |dialog, _window, cx| {
            dialog
                .title("打开连接".to_string())
                .w(px(520.0))
                .child(
                    v_flex().gap_2().child(
                        List::new(&list)
                            .w_full()
                            .max_h(px(360.0))
                            .p(px(8.0))
                            .bg(cx.theme().list)
                            .border_1()
                            .border_color(cx.theme().border)
                            .rounded(cx.theme().radius),
                    ),
                )
                .alert()
                .button_props(
                    gpui_component::dialog::DialogButtonProps::default()
                        .ok_text(t!("Common.close")),
                )
        });
        // 将焦点设置到 List 搜索框，使上下键和 Enter 键可用
        list_for_focus.update(cx, |state, cx| {
            state.focus(window, cx);
        });
    }

    pub(crate) fn show_new_connection_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editing_connection_id = None;

        if !self.ensure_master_key_ready_for_new_connection(window, cx) {
            return;
        }

        let parent = cx.entity();
        let parent_window = window.window_handle();
        open_popup_window(
            window,
            PopupWindowOptions::new(t!("Home.new_connection").to_string()).size(1100.0, 700.0),
            move |win, app| {
                app.new(|app| NewConnectionWindow::new(parent, parent_window, win, app))
            },
            cx,
        );
    }

    pub(crate) fn open_connection_from_quick(
        &mut self,
        connection: &StoredConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = connection
            .workspace_id
            .and_then(|id| self.workspaces.iter().find(|w| w.id == Some(id)).cloned());
        let strategy = build_connection_open_strategy(connection.clone(), workspace);
        strategy.open(self, window, cx);
        cx.notify();
    }

    pub(crate) fn handle_save_workspace(
        &mut self,
        workspace_id: Option<i64>,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let storage = cx.global::<GlobalStorageState>().storage.clone();
        let editing_id = workspace_id;

        let mut workspace = if let Some(id) = editing_id {
            // 编辑模式：从现有工作区更新
            let mut ws = self
                .workspaces
                .iter()
                .find(|w| w.id == Some(id))
                .cloned()
                .unwrap_or_else(|| Workspace::new(name.clone()));
            ws.name = name;
            ws
        } else {
            // 新建模式
            Workspace::new(name)
        };

        let result: anyhow::Result<Workspace> = (|| {
            let repo = storage
                .get::<WorkspaceRepository>()
                .ok_or_else(|| anyhow::anyhow!("WorkspaceRepository not found"))?;

            if editing_id.is_some() {
                repo.update(&mut workspace)?;
            } else {
                repo.insert(&mut workspace)?;
            }

            Ok(workspace)
        })();

        cx.spawn(async move |this, cx| match result {
            Ok(workspace) => {
                _ = this.update(cx, |this, cx| {
                    let workspace_id = workspace.id.unwrap_or(0);
                    if let Some(editing_id) = editing_id {
                        if let Some(pos) = this
                            .workspaces
                            .iter()
                            .position(|w| w.id == Some(editing_id))
                        {
                            this.workspaces[pos] = workspace;
                        }
                        emit_connection_event(
                            ConnectionDataEvent::WorkspaceUpdated { workspace_id },
                            cx,
                        );
                    } else {
                        this.workspaces.push(workspace);
                        emit_connection_event(
                            ConnectionDataEvent::WorkspaceCreated { workspace_id },
                            cx,
                        );
                    }
                    // 兜底触发一次自动同步，避免当前页对自身工作区事件未回流时漏同步。
                    if this.current_user.is_some() && crypto::has_master_key() {
                        tracing::info!("本地工作区保存成功，自动触发云同步");
                        this.trigger_sync(cx);
                    }
                    cx.notify();
                });
            }
            Err(e) => {
                tracing::error!("Failed to save workspace: {}", e);
            }
        })
        .detach();
    }

    pub(crate) fn delete_workspace(
        &mut self,
        workspace_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self
            .workspaces
            .iter()
            .find(|w| w.id == Some(workspace_id))
            .cloned();
        let Some(workspace) = workspace else {
            return;
        };
        let workspace_name = workspace.name.clone();
        let workspace_connections: Vec<StoredConnection> = self
            .connections
            .iter()
            .filter(|connection| connection.workspace_id == Some(workspace_id))
            .cloned()
            .collect();

        let view = cx.entity().clone();
        if workspace_connections.is_empty() {
            window.open_dialog(cx, move |dialog, _window, _cx| {
                let view_clone = view.clone();
                dialog
                    .title(t!("Workspace.delete").to_string().into_any_element())
                    .child(
                        t!("Workspace.delete_confirm", workspace_name = workspace_name)
                            .to_string()
                            .into_any_element(),
                    )
                    .confirm()
                    .on_ok(move |_, _window, cx| {
                        let _ = view_clone.update(cx, |this, cx| {
                            this.handle_delete_workspace(workspace_id, cx);
                        });
                        true
                    })
            });
            return;
        }

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let view_for_confirm = view.clone();
            dialog
                .title(t!("Workspace.delete").to_string().into_any_element())
                .child(
                    v_flex().gap_2().child(
                        t!(
                            "Workspace.delete_has_connections",
                            workspace_name = workspace_name,
                            count = workspace_connections.len()
                        )
                        .to_string()
                        .into_any_element(),
                    ),
                )
                .footer(move |_ok, cancel, window, cx| {
                    vec![
                        cancel(window, cx),
                        Button::new(format!("workspace-delete-{}", workspace_id))
                            .label(t!("Workspace.delete_move_to_unassigned").to_string())
                            .with_variant(ButtonVariant::Primary)
                            .on_click({
                                let view_for_confirm = view_for_confirm.clone();
                                move |_, window, cx| {
                                    window.close_dialog(cx);
                                    let _ = view_for_confirm.update(cx, |this, cx| {
                                        this.handle_delete_workspace(workspace_id, cx);
                                    });
                                }
                            })
                            .into_any_element(),
                    ]
                })
                .overlay_closable(false)
                .close_button(true)
        });
    }

    fn handle_delete_workspace(&mut self, workspace_id: i64, cx: &mut Context<Self>) {
        let storage = cx.global::<GlobalStorageState>().storage.clone();
        let workspace = self
            .workspaces
            .iter()
            .find(|item| item.id == Some(workspace_id))
            .cloned();
        let Some(workspace) = workspace else {
            return;
        };
        let workspace_connections: Vec<StoredConnection> = self
            .connections
            .iter()
            .filter(|connection| connection.workspace_id == Some(workspace_id))
            .cloned()
            .collect();

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let Some(connection_repo) = storage.get::<ConnectionRepository>() else {
                tracing::error!("Failed to delete workspace: ConnectionRepository not found");
                return;
            };
            let Some(workspace_repo) = storage.get::<WorkspaceRepository>() else {
                tracing::error!("Failed to delete workspace: WorkspaceRepository not found");
                return;
            };

            let mut updated_connections = Vec::new();
            for connection in &workspace_connections {
                let mut updated_connection = connection.clone();
                updated_connection.workspace_id = None;
                updated_connection.sort_order = None;
                if let Err(e) = connection_repo.update(&updated_connection) {
                    tracing::error!("Failed to move connection to unassigned: {}", e);
                    return;
                }
                updated_connections.push(updated_connection);
            }

            if let Err(e) = workspace_repo.delete(workspace_id) {
                tracing::error!("Failed to delete workspace: {}", e);
                return;
            }
            Self::queue_pending_workspace_deletion(&storage, &workspace, &workspace_connections);

            _ = this.update(cx, |this, cx| {
                this.workspaces.retain(|w| w.id != Some(workspace_id));
                this.filtered_workspace_ids.remove(&workspace_id);

                for updated_connection in updated_connections {
                    if let Some(position) = this
                        .connections
                        .iter()
                        .position(|connection| connection.id == updated_connection.id)
                    {
                        this.connections[position] = updated_connection.clone();
                    }
                    emit_connection_event(
                        ConnectionDataEvent::ConnectionUpdated {
                            connection: updated_connection,
                        },
                        cx,
                    );
                }

                emit_connection_event(ConnectionDataEvent::WorkspaceDeleted { workspace_id }, cx);
                if this.current_user.is_some() && crypto::has_master_key() {
                    tracing::info!("本地工作区删除成功，自动触发云同步");
                    this.trigger_sync(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn show_connection_form(
        &mut self,
        db_type: DatabaseType,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.editing_connection_id.is_none() && !self.is_master_key_ready_for_new_connection() {
            return;
        }

        let editing_conn = self
            .editing_connection_id
            .and_then(|id| self.connections.iter().find(|c| c.id == Some(id)).cloned());

        let config = ConnectionFormWindowConfig {
            db_type,
            external_driver_id: None,
            editing_connection: editing_conn,
            workspaces: self.workspaces.clone(),
        };

        self.editing_connection_id = None;

        open_popup_window(
            window,
            PopupWindowOptions::new(if config.editing_connection.is_some() {
                t!("Connection.edit", db_type = db_type.as_str()).to_string()
            } else {
                t!("Connection.new", db_type = db_type.as_str()).to_string()
            })
            .size(700.0, 650.0),
            move |window, cx| cx.new(|cx| ConnectionFormWindow::new(config, window, cx)),
            cx,
        );
    }

    pub(crate) fn show_ssh_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_connection_id.is_none() && !self.is_master_key_ready_for_new_connection() {
            return;
        }

        let editing_conn = self.editing_connection_id.and_then(|id| {
            self.connections
                .iter()
                .find(|c| c.id == Some(id) && c.connection_type == ConnectionType::SshSftp)
                .cloned()
        });

        let config = SshFormWindowConfig {
            editing_connection: editing_conn,
            workspaces: self.workspaces.clone(),
        };

        self.editing_connection_id = None;

        open_popup_window(
            window,
            PopupWindowOptions::new(if config.editing_connection.is_some() {
                t!("SSH.edit").to_string()
            } else {
                t!("SSH.new").to_string()
            })
            .size(700.0, 650.0),
            move |window, cx| cx.new(|cx| SshFormWindow::new(config, window, cx)),
            cx,
        );
    }

    pub(crate) fn show_redis_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_connection_id.is_none() && !self.is_master_key_ready_for_new_connection() {
            return;
        }

        let editing_conn = self.editing_connection_id.and_then(|id| {
            self.connections
                .iter()
                .find(|c| c.id == Some(id) && c.connection_type == ConnectionType::Redis)
                .cloned()
        });

        let config = RedisFormWindowConfig {
            editing_connection: editing_conn,
            workspaces: self.workspaces.clone(),
        };

        self.editing_connection_id = None;

        open_popup_window(
            window,
            PopupWindowOptions::new(if config.editing_connection.is_some() {
                t!("Connection.edit", db_type = "Redis").to_string()
            } else {
                t!("Connection.new", db_type = "Redis").to_string()
            })
            .size(700.0, 650.0),
            move |window, cx| cx.new(|cx| RedisFormWindow::new(config, window, cx)),
            cx,
        );
    }

    pub(crate) fn show_mongodb_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_connection_id.is_none() && !self.is_master_key_ready_for_new_connection() {
            return;
        }

        let editing_conn = self.editing_connection_id.and_then(|id| {
            self.connections
                .iter()
                .find(|c| c.id == Some(id) && c.connection_type == ConnectionType::MongoDB)
                .cloned()
        });

        let config = MongoFormWindowConfig {
            editing_connection: editing_conn,
            workspaces: self.workspaces.clone(),
        };

        self.editing_connection_id = None;

        open_popup_window(
            window,
            PopupWindowOptions::new(if config.editing_connection.is_some() {
                t!("Connection.edit", db_type = "MongoDB").to_string()
            } else {
                t!("Connection.new", db_type = "MongoDB").to_string()
            })
            .size(700.0, 520.0),
            move |window, cx| cx.new(|cx| MongoFormWindow::new(config, window, cx)),
            cx,
        );
    }

    pub(crate) fn show_serial_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_connection_id.is_none() && !self.is_master_key_ready_for_new_connection() {
            return;
        }

        let editing_conn = self.editing_connection_id.and_then(|id| {
            self.connections
                .iter()
                .find(|c| c.id == Some(id) && c.connection_type == ConnectionType::Serial)
                .cloned()
        });

        let config = SerialFormWindowConfig {
            editing_connection: editing_conn,
            workspaces: self.workspaces.clone(),
        };

        self.editing_connection_id = None;

        open_popup_window(
            window,
            PopupWindowOptions::new(if config.editing_connection.is_some() {
                t!("Serial.edit").to_string()
            } else {
                t!("Serial.new").to_string()
            })
            .size(700.0, 600.0),
            move |window, cx| cx.new(|cx| SerialFormWindow::new(config, window, cx)),
            cx,
        );
    }

    pub(crate) fn ensure_master_key_ready_for_new_connection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.is_master_key_ready_for_new_connection() {
            return true;
        }

        self.show_encryption_key_dialog(window, cx);
        false
    }

    pub(crate) fn is_master_key_ready_for_new_connection(&self) -> bool {
        crypto::has_master_key()
    }

    fn show_encryption_key_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity();
        let has_password_set = crypto::has_repo_password_set();
        let has_key_in_memory = crypto::has_master_key();
        let is_first_setup = !has_password_set;
        let is_change_mode = has_password_set && has_key_in_memory;
        let initial_master_key = crypto::get_raw_master_key().or_else(|| {
            let storage = key_storage::get_key_storage();
            storage.load()
        });

        let key_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("Encryption.repo_password_placeholder"))
                .masked(true);

            if let Some(ref value) = initial_master_key {
                state = state.default_value(value);
            }

            state
        });

        let error_message = cx.new(|_| Option::<String>::None);

        let key_input_for_ok = key_input.clone();
        let error_msg_for_ok = error_message.clone();

        let key_input_for_render = key_input.clone();
        let error_msg_for_render = error_message.clone();

        let dialog_title = if is_first_setup {
            t!("Encryption.set_repo_password")
        } else if is_change_mode {
            t!("Encryption.change_repo_password")
        } else {
            t!("Encryption.unlock_repo_password")
        };

        window.open_dialog(cx, move |dialog, _window, cx| {
            let key_input_ok = key_input_for_ok.clone();
            let error_msg_ok = error_msg_for_ok.clone();

            dialog
                .title(dialog_title.to_string())
                .width(px(450.))
                .confirm()
                .on_ok(move |_, _window, cx| {
                    let input_key = key_input_ok.read(cx).text().to_string();

                    if input_key.is_empty() {
                        error_msg_ok.update(cx, |msg, cx| {
                            *msg = Some(t!("Encryption.key_empty").to_string());
                            cx.notify();
                        });
                        return false;
                    }

                    if is_first_setup {
                        crypto::set_master_key(&input_key);
                        return true;
                    }

                    if is_change_mode {
                        let old_key = match crypto::get_raw_master_key() {
                            Some(key) if !key.is_empty() => key,
                            _ => {
                                error_msg_ok.update(cx, |msg, cx| {
                                    *msg = Some(t!("Encryption.password_incorrect").to_string());
                                    cx.notify();
                                });
                                return false;
                            }
                        };

                        if input_key != old_key {
                            match crypto::change_master_key(&old_key, &input_key, &input_key) {
                                Ok(()) => {
                                    let storage = cx.global::<GlobalStorageState>().storage.clone();
                                    match re_encrypt_all_connections(&storage) {
                                        Ok(count) => {
                                            tracing::info!(
                                                "主密钥修改成功，已重新加密 {} 个本地连接",
                                                count
                                            );
                                        }
                                        Err(e) => {
                                            tracing::error!("重新加密本地连接失败: {}", e);
                                            error_msg_ok.update(cx, |msg, cx| {
                                                *msg = Some(e.to_string());
                                                cx.notify();
                                            });
                                            return false;
                                        }
                                    }
                                }
                                Err(e) => {
                                    error_msg_ok.update(cx, |msg, cx| {
                                        *msg = Some(e.to_string());
                                        cx.notify();
                                    });
                                    return false;
                                }
                            }
                        }

                        return true;
                    }

                    match crypto::verify_and_set_master_key(&input_key) {
                        Ok(()) => true,
                        Err(_) => {
                            error_msg_ok.update(cx, |msg, cx| {
                                *msg = Some(t!("Encryption.password_incorrect").to_string());
                                cx.notify();
                            });
                            false
                        }
                    }
                })
                .on_close({
                    let view_for_sync = view.clone();
                    move |_window, _result, cx| {
                        if crypto::has_master_key() {
                            view_for_sync.update(cx, |this, cx| {
                                // 密钥已就绪后刷新连接列表，修复启动时序导致的空密码回显
                                this.load_connections(cx);
                                if this.current_user.is_some() {
                                    tracing::info!("密钥设置/解锁成功，自动触发云同步");
                                    this.trigger_sync(cx);
                                }
                            });
                        }
                    }
                })
                .child(
                    v_flex()
                        .gap_4()
                        .p_4()
                        .bg(cx.theme().background)
                        .child(
                            h_flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .text_sm()
                                        .flex_shrink_0()
                                        .w(px(80.))
                                        .child(t!("Encryption.repo_password_label").to_string()),
                                )
                                .child(Input::new(&key_input_for_render).mask_toggle().w_full()),
                        )
                        .child(
                            v_flex()
                                .gap_2()
                                .child(
                                    div().text_base().font_weight(FontWeight::SEMIBOLD).child(
                                        t!("Encryption.remember_password_title").to_string(),
                                    ),
                                )
                                .child(div().text_sm().child(
                                    t!("Encryption.remember_password_detail_local").to_string(),
                                ))
                                .child(div().text_sm().text_color(cx.theme().warning).child(
                                    t!("Encryption.remember_password_detail_cloud").to_string(),
                                )),
                        )
                        .when_some(error_msg_for_render.read(cx).clone(), |this, msg| {
                            this.child(div().text_sm().text_color(cx.theme().danger).child(msg))
                        }),
                )
        });
    }

    fn render_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view_for_new_connection = cx.entity();
        let view_for_sort_field = view_for_new_connection.clone();
        let view_for_view_mode = view_for_new_connection.clone();
        let blur_enabled = cx.theme().window_blur_enabled;
        let window_opacity = cx.theme().backdrop_opacity;
        let toolbar_bg = layered_level_surface_color(
            cx.theme().background,
            blur_enabled,
            window_opacity,
            3,
            WindowsSurfaceLayer::ContentSection,
        );
        let workspace_filter_open = self.workspace_filter_open;
        let workspace_filter =
            self.render_workspace_filter_popover(workspace_filter_open, window, cx);
        let sort_field = Self::connection_list_sort_field(cx);
        let sort_order = Self::connection_list_sort_order(cx);
        let view_mode = Self::connection_list_view_mode(cx);

        let is_syncing = self.syncing;
        let is_logged_in = self.current_user.is_some();
        let sync_backend_type = AppSettings::global(cx).sync_backend_type.clone();
        let uses_github_gist = sync_backend_type == "github_gist";
        let can_sync = is_logged_in || uses_github_gist;
        let has_master_key = crypto::has_master_key();
        let has_conflicts = !self.pending_conflicts.is_empty();
        let conflict_count = self.pending_conflicts.len();
        h_flex()
            .gap_3()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(toolbar_bg)
            .items_center()
            // ===== 左侧功能区 =====
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    // 新建连接按钮（主要操作）
                    .child(
                        Button::new("new-connect-button")
                            .icon(IconName::Plus)
                            .primary()
                            .label(t!("Home.new_connection"))
                            .text_color(cx.theme().primary_foreground)
                            .bg(cx.theme().primary)
                            .cursor_pointer()
                            .with_variant(ButtonVariant::Custom(
                                ButtonCustomVariant::new(cx).hover(cx.theme().primary),
                            ))
                            .tooltip(t!("Home.new_connection"))
                            .dropdown_menu(move |menu, window, _cx| {
                                let mut menu = menu
                                    .item(
                                        PopupMenuItem::new(t!("Workspace.label"))
                                            .icon(
                                                IconName::AppsColor.color().with_size(Size::Medium),
                                            )
                                            .on_click(window.listener_for(
                                                &view_for_new_connection,
                                                move |_this, _, window, cx| {
                                                    let parent = cx.entity();
                                                    show_workspace_dialog(
                                                        parent,
                                                        None,
                                                        String::new(),
                                                        window,
                                                        cx,
                                                    );
                                                },
                                            )),
                                    )
                                    .separator()
                                    .item(
                                        PopupMenuItem::new("SSH")
                                            .icon(
                                                IconName::TerminalColor
                                                    .color()
                                                    .with_size(Size::Medium),
                                            )
                                            .on_click(window.listener_for(
                                                &view_for_new_connection,
                                                move |this, _, window, cx| {
                                                    this.editing_connection_id = None;
                                                    this.show_ssh_form(window, cx);
                                                },
                                            )),
                                    )
                                    .item(
                                        PopupMenuItem::new("Terminal")
                                            .icon(
                                                IconName::Terminal
                                                    .mono()
                                                    .text_color(gpui::rgb(0x8b5cf6))
                                                    .with_size(Size::Medium),
                                            )
                                            .on_click(window.listener_for(
                                                &view_for_new_connection,
                                                move |this, _, window, cx| {
                                                    this.add_terminal_tab(window, cx);
                                                },
                                            )),
                                    )
                                    .item(
                                        PopupMenuItem::new("Redis")
                                            .icon(IconName::Redis.color().with_size(Size::Medium))
                                            .on_click(window.listener_for(
                                                &view_for_new_connection,
                                                move |this, _, window, cx| {
                                                    this.editing_connection_id = None;
                                                    this.show_redis_form(window, cx);
                                                },
                                            )),
                                    )
                                    .item(
                                        PopupMenuItem::new("MongoDB")
                                            .icon(IconName::MongoDB.color().with_size(Size::Medium))
                                            .on_click(window.listener_for(
                                                &view_for_new_connection,
                                                move |this, _, window, cx| {
                                                    this.editing_connection_id = None;
                                                    this.show_mongodb_form(window, cx);
                                                },
                                            )),
                                    )
                                    .item(
                                        PopupMenuItem::new(t!("Serial.new"))
                                            .icon(
                                                IconName::SerialPort
                                                    .color()
                                                    .with_size(Size::Medium),
                                            )
                                            .on_click(window.listener_for(
                                                &view_for_new_connection,
                                                move |this, _, window, cx| {
                                                    this.editing_connection_id = None;
                                                    this.show_serial_form(window, cx);
                                                },
                                            )),
                                    )
                                    .separator();

                                for db_type in DatabaseType::all() {
                                    let db_type = *db_type;
                                    let label: SharedString = db_type.as_str().to_string().into();
                                    menu = menu.item(
                                        PopupMenuItem::new(label)
                                            .icon(db_type.as_node_icon().with_size(Size::Medium))
                                            .on_click(window.listener_for(
                                                &view_for_new_connection,
                                                move |this, _, window, cx| {
                                                    this.editing_connection_id = None;
                                                    this.show_connection_form(db_type, window, cx);
                                                },
                                            )),
                                    );
                                }

                                menu
                            }),
                    )
                    // 分隔线
                    .child(div().h(px(20.0)).w(px(1.0)).bg(cx.theme().border).mx_1())
                    // 同步按钮
                    .child(
                        Button::new("sync-button")
                            .icon(IconName::Refresh)
                            .label(if is_syncing {
                                t!("Home.syncing").to_string()
                            } else {
                                t!("Home.sync").to_string()
                            })
                            .cursor_pointer()
                            .ghost()
                            .disabled(!can_sync || is_syncing)
                            .tooltip(if !can_sync {
                                t!("Home.cloud_need_login")
                            } else {
                                t!("Home.sync_tooltip")
                            })
                            .on_click(cx.listener(move |this, _, _window, cx| {
                                this.trigger_sync(cx);
                            })),
                    )
                    // 冲突指示器
                    .when(has_conflicts, |this| {
                        this.child(
                            Button::new("conflict-button")
                                .icon(IconName::TriangleAlert)
                                .label(format!("{}", conflict_count))
                                .ghost()
                                .text_color(cx.theme().warning)
                                .tooltip(t!("Home.conflict_tooltip", count = conflict_count))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_conflict_dialog(window, cx);
                                })),
                        )
                    })
                    // 主密钥按钮
                    .child(
                        Button::new("encryption-key-button")
                            .icon(IconName::Key)
                            .label(if has_master_key {
                                t!("Encryption.key_unlocked").to_string()
                            } else {
                                t!("Encryption.edit_repo_password").to_string()
                            })
                            .cursor_pointer()
                            .ghost()
                            .when(has_master_key, |btn| btn.text_color(cx.theme().success))
                            .when(!has_master_key, |btn| {
                                btn.text_color(cx.theme().muted_foreground)
                            })
                            .tooltip(if has_master_key {
                                t!("Encryption.key_unlocked_tooltip")
                            } else {
                                t!("Encryption.key_locked_tooltip")
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_encryption_key_dialog(window, cx);
                            })),
                    ),
            )
            // ===== 中间弹性空间 =====
            .child(div().flex_1())
            // ===== 右侧操作区 =====
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    // 搜索框
                    .child(
                        Input::new(&self.search_input)
                            .cleanable(true)
                            .w(px(240.0))
                            .refine_style(&app_style::control_style()),
                    )
                    .child(
                        Button::new("connection-sort-field-button")
                            .icon(IconName::ChevronsUpDown)
                            .label(Self::connection_sort_field_label(sort_field))
                            .ghost()
                            .tooltip(t!("Home.sort_field"))
                            .dropdown_menu({
                                let view = view_for_sort_field.clone();
                                move |menu, window, _cx| {
                                    menu.item(
                                        PopupMenuItem::new(t!("Home.sort_by_updated_at"))
                                            .checked(
                                                sort_field == ConnectionListSortField::UpdatedAt,
                                            )
                                            .on_click(window.listener_for(
                                                &view,
                                                move |this, _, _, cx| {
                                                    this.set_connection_list_sort_field(
                                                        ConnectionListSortField::UpdatedAt,
                                                        cx,
                                                    );
                                                },
                                            )),
                                    )
                                    .item(
                                        PopupMenuItem::new(t!("Home.sort_by_created_at"))
                                            .checked(
                                                sort_field == ConnectionListSortField::CreatedAt,
                                            )
                                            .on_click(window.listener_for(
                                                &view,
                                                move |this, _, _, cx| {
                                                    this.set_connection_list_sort_field(
                                                        ConnectionListSortField::CreatedAt,
                                                        cx,
                                                    );
                                                },
                                            )),
                                    )
                                    .item(
                                        PopupMenuItem::new(t!("Home.sort_by_name"))
                                            .checked(sort_field == ConnectionListSortField::Name)
                                            .on_click(window.listener_for(
                                                &view,
                                                move |this, _, _, cx| {
                                                    this.set_connection_list_sort_field(
                                                        ConnectionListSortField::Name,
                                                        cx,
                                                    );
                                                },
                                            )),
                                    )
                                    .item(
                                        PopupMenuItem::new(t!("Home.sort_by_manual"))
                                            .checked(sort_field == ConnectionListSortField::Manual)
                                            .on_click(window.listener_for(
                                                &view,
                                                move |this, _, _, cx| {
                                                    this.set_connection_list_sort_field(
                                                        ConnectionListSortField::Manual,
                                                        cx,
                                                    );
                                                },
                                            )),
                                    )
                                }
                            }),
                    )
                    .child(
                        Button::new("connection-sort-order-button")
                            .icon(match sort_order {
                                ConnectionListSortOrder::Ascending => IconName::SortAscending,
                                ConnectionListSortOrder::Descending => IconName::SortDescending,
                            })
                            .ghost()
                            .tooltip(Self::connection_sort_order_label(sort_order))
                            .disabled(sort_field == ConnectionListSortField::Manual)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_connection_list_sort_order(cx);
                            })),
                    )
                    .child(
                        Button::new("connection-view-mode-button")
                            .icon(match view_mode {
                                ConnectionListViewMode::Card => IconName::Apps,
                                ConnectionListViewMode::List => IconName::Menu,
                            })
                            .cursor_pointer()
                            // .label(Self::connection_list_view_mode_label(view_mode))
                            .ghost()
                            .tooltip(t!("Home.view_mode"))
                            .dropdown_menu({
                                let view = view_for_view_mode.clone();
                                move |menu, window, _cx| {
                                    menu.item(
                                        PopupMenuItem::new(t!("Home.view_mode_card"))
                                            .checked(view_mode == ConnectionListViewMode::Card)
                                            .on_click(window.listener_for(
                                                &view,
                                                move |this, _, _, cx| {
                                                    this.set_connection_list_view_mode(
                                                        ConnectionListViewMode::Card,
                                                        cx,
                                                    );
                                                },
                                            )),
                                    )
                                    .item(
                                        PopupMenuItem::new(t!("Home.view_mode_list"))
                                            .checked(view_mode == ConnectionListViewMode::List)
                                            .on_click(window.listener_for(
                                                &view,
                                                move |this, _, _, cx| {
                                                    this.set_connection_list_view_mode(
                                                        ConnectionListViewMode::List,
                                                        cx,
                                                    );
                                                },
                                            )),
                                    )
                                }
                            }),
                    )
                    // 刷新按钮
                    .child(
                        Button::new("refresh-button")
                            .icon(IconName::Refresh)
                            .cursor_pointer()
                            .ghost()
                            .tooltip(t!("Home.refresh"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.refresh_local_home_data(cx);
                            })),
                    )
                    // 工作区筛选
                    .child(workspace_filter),
            )
    }

    fn render_workspace_filter_popover(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let view_for_select = view.clone();
        let view_for_clear = view.clone();
        let view_for_new = view.clone();

        let list = self.ensure_workspace_filter_list(window, cx);

        let workspaces = &self.workspaces;
        let connections = &self.connections;
        let filtered_ids = &self.filtered_workspace_ids;
        list.update(cx, |state, _cx| {
            state
                .delegate_mut()
                .update_items_with_data(workspaces, connections, filtered_ids);
        });

        let is_all_selected = self.filtered_workspace_ids.is_empty()
            || self.filtered_workspace_ids.len()
                == self.workspaces.iter().filter(|w| w.id.is_some()).count();

        Popover::new("workspace-filter-popover")
            .trigger(
                Button::new("workspace-filter")
                    .icon(IconName::Filter)
                    .tooltip(t!("Workspace.filter")),
            )
            .open(open)
            .on_open_change(cx.listener(|this, open, _, cx| {
                this.workspace_filter_open = *open;
                cx.notify();
            }))
            .content(move |_, _, cx| {
                v_flex()
                    .w(px(280.0))
                    .max_h(px(400.0))
                    .gap_2()
                    .p_2()
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .px_1()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child({
                                        let view_select = view_for_select.clone();
                                        Checkbox::new("select-all-ws")
                                            .checked(is_all_selected)
                                            .on_click(move |_, _, cx| {
                                                view_select.update(cx, |this, cx| {
                                                    if this.filtered_workspace_ids.is_empty()
                                                        || this.filtered_workspace_ids.len()
                                                            == this
                                                                .workspaces
                                                                .iter()
                                                                .filter(|w| w.id.is_some())
                                                                .count()
                                                    {
                                                        this.clear_workspace_filter(cx);
                                                    } else {
                                                        this.select_all_workspaces(cx);
                                                    }
                                                });
                                            })
                                    })
                                    .child(div().text_sm().child(
                                        t!("Workspace.select_all").to_string().into_any_element(),
                                    )),
                            )
                            .child(
                                h_flex()
                                    .gap_1()
                                    .child({
                                        let view_new = view_for_new.clone();
                                        Button::new("new-workspace-from-filter")
                                            .primary()
                                            .small()
                                            .label(t!("Common.new"))
                                            .on_click(move |_, window, cx| {
                                                show_workspace_dialog(
                                                    view_new.clone(),
                                                    None,
                                                    String::new(),
                                                    window,
                                                    cx,
                                                );
                                            })
                                    })
                                    .child({
                                        let view_clear = view_for_clear.clone();
                                        Button::new("clear-ws-filter")
                                            .ghost()
                                            .small()
                                            .label(t!("Workspace.clear_filter"))
                                            .on_click(move |_, _, cx| {
                                                view_clear.update(cx, |this, cx| {
                                                    this.clear_workspace_filter(cx);
                                                });
                                            })
                                    }),
                            ),
                    )
                    .child(div().border_t_1().border_color(cx.theme().border))
                    .child(
                        List::new(&list)
                            .w_full()
                            .max_h(px(320.0))
                            .p(px(8.))
                            .flex_1()
                            .border_1()
                            .border_color(cx.theme().border)
                            .rounded(cx.theme().radius),
                    )
            })
            .into_any_element()
    }

    fn ensure_workspace_filter_list(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ListState<WorkspaceFilterDelegate>> {
        if let Some(ref list) = self.workspace_filter_list {
            return list.clone();
        }

        let parent = cx.entity();
        let list = cx.new(|cx| {
            ListState::new(WorkspaceFilterDelegate::new(parent), window, cx).searchable(true)
        });
        self.workspace_filter_list = Some(list.clone());
        list
    }

    pub(crate) fn toggle_workspace_filter(&mut self, workspace_id: i64, cx: &mut Context<Self>) {
        if self.filtered_workspace_ids.is_empty() {
            for ws in &self.workspaces {
                if let Some(id) = ws.id {
                    self.filtered_workspace_ids.insert(id);
                }
            }
        }

        if self.filtered_workspace_ids.contains(&workspace_id) {
            self.filtered_workspace_ids.remove(&workspace_id);
        } else {
            self.filtered_workspace_ids.insert(workspace_id);
        }
        cx.notify();
    }

    fn select_all_workspaces(&mut self, cx: &mut Context<Self>) {
        self.filtered_workspace_ids.clear();
        for ws in &self.workspaces {
            if let Some(id) = ws.id {
                self.filtered_workspace_ids.insert(id);
            }
        }
        cx.notify();
    }

    fn clear_workspace_filter(&mut self, cx: &mut Context<Self>) {
        self.filtered_workspace_ids.clear();
        cx.notify();
    }

    fn render_sidebar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 同步全局用户状态：设置页面登录/登出后，同步本地状态
        let global_user = GlobalCurrentUser::get_user(cx);
        if global_user.is_none() && self.current_user.is_some() {
            self.current_user = None;
        } else if let Some(user) = global_user {
            if self.current_user.is_none() {
                self.current_user = Some(user);
            }
        }

        let blur_enabled = cx.theme().window_blur_enabled;
        let window_opacity = cx.theme().backdrop_opacity;
        let sidebar_bg = layered_level_surface_color(
            cx.theme().sidebar,
            blur_enabled,
            window_opacity,
            2,
            WindowsSurfaceLayer::ContentBase,
        );
        let sidebar_active_bg = layered_level_surface_color(
            cx.theme().list_active,
            blur_enabled,
            window_opacity,
            2,
            WindowsSurfaceLayer::ContentBase,
        );
        let sidebar_hover_bg = layered_level_surface_color(
            cx.theme().sidebar_accent,
            blur_enabled,
            window_opacity,
            2,
            WindowsSurfaceLayer::ContentBase,
        );
        let filter_types = ConnectionType::all();

        v_flex()
            .w(px(160.))
            .h_full()
            .flex_shrink_0()
            .bg(sidebar_bg)
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                // 侧边栏过滤选项
                v_flex()
                    .flex_1()
                    .w_full()
                    .p_1()
                    // .gap_2()
                    .children(filter_types.into_iter().map(|filter_type| {
                        let is_selected = self.selected_filter == filter_type;
                        let filter_type_clone = filter_type;

                        div()
                            .id(filter_type.label())
                            .flex()
                            .items_center()
                            .gap_3()
                            .w_full()
                            .px_3()
                            .py_1()
                            .cursor_pointer()
                            .rounded_lg()
                            .overflow_hidden()
                            .when(is_selected, |this| {
                                this.bg(sidebar_active_bg)
                                    .border_l_3()
                                    .border_color(cx.theme().list_active_border)
                            })
                            .when(!is_selected, |this| {
                                this.hover(|style| style.bg(sidebar_hover_bg))
                            })
                            .on_click(cx.listener(move |this: &mut HomePage, _, window, cx| {
                                if filter_type_clone == ConnectionType::ChatDB {
                                    this.add_ai_chat_tab(window, cx);
                                    return;
                                }
                                this.selected_filter = filter_type_clone;
                                cx.notify();
                            }))
                            .child(Icon::new(filter_type.icon()).color().with_size(Size::Large))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().foreground)
                                    .when(is_selected, |this| this.font_weight(FontWeight::MEDIUM))
                                    .child(filter_type.label()),
                            )
                    })),
            )
            .child(
                // 底部区域：设置
                v_flex()
                    .w_full()
                    // .p_4()
                    // .gap_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("open_settings")
                            .icon(IconName::Settings)
                            .label(t!("Common.settings"))
                            .cursor_pointer()
                            .w_full()
                            .justify_start()
                            .on_click(cx.listener(|this: &mut HomePage, _, window, cx| {
                                this.add_settings_tab(window, cx);
                            })),
                    ),
            )
    }

    fn match_connection_type(&self, conn: &StoredConnection) -> bool {
        match self.selected_filter {
            ConnectionType::All => true,
            filter_type => conn.connection_type == filter_type,
        }
    }

    fn match_connection(&self, conn: &StoredConnection, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }

        // 匹配连接名称
        if conn.name.to_lowercase().contains(query) {
            return true;
        }

        // 根据连接类型解析对应参数进行匹配
        match conn.connection_type {
            ConnectionType::Database => {
                if let Ok(params) = conn.to_db_connection() {
                    if params.host.to_lowercase().contains(query) {
                        return true;
                    }
                    if params.port.to_string().contains(query) {
                        return true;
                    }
                    if params.username.to_lowercase().contains(query) {
                        return true;
                    }
                    if params
                        .database
                        .as_ref()
                        .map_or(false, |db| db.to_lowercase().contains(query))
                    {
                        return true;
                    }
                    let conn_str = format!("{}@{}:{}", params.username, params.host, params.port);
                    if conn_str.to_lowercase().contains(query) {
                        return true;
                    }
                }
            }
            ConnectionType::SshSftp => {
                if let Ok(params) = conn.to_ssh_params() {
                    if params.host.to_lowercase().contains(query) {
                        return true;
                    }
                    if params.port.to_string().contains(query) {
                        return true;
                    }
                    if params.username.to_lowercase().contains(query) {
                        return true;
                    }
                    let conn_str = format!("{}@{}:{}", params.username, params.host, params.port);
                    if conn_str.to_lowercase().contains(query) {
                        return true;
                    }
                }
            }
            ConnectionType::Redis => {
                if let Ok(params) = conn.to_redis_params() {
                    if params.host.to_lowercase().contains(query) {
                        return true;
                    }
                    if params.port.to_string().contains(query) {
                        return true;
                    }
                    if params
                        .username
                        .as_ref()
                        .map_or(false, |u| u.to_lowercase().contains(query))
                    {
                        return true;
                    }
                }
            }
            ConnectionType::MongoDB => {
                if let Ok(params) = conn.to_mongodb_params() {
                    if params.host.to_lowercase().contains(query) {
                        return true;
                    }
                    if params.port.map_or(false, |p| p.to_string().contains(query)) {
                        return true;
                    }
                    if params
                        .username
                        .as_ref()
                        .map_or(false, |u| u.to_lowercase().contains(query))
                    {
                        return true;
                    }
                    if params
                        .database
                        .as_ref()
                        .map_or(false, |db| db.to_lowercase().contains(query))
                    {
                        return true;
                    }
                    if params.connection_string.to_lowercase().contains(query) {
                        return true;
                    }
                }
            }
            _ => {}
        }

        false
    }

    fn render_content_area(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let search_query = self.search_query.read(cx).to_lowercase();
        let selected_id = self.selected_connection_id;
        self.render_workspace_view(&search_query, selected_id, cx)
            .into_any_element()
    }

    fn sort_connections_for_display(
        &self,
        mut connections: Vec<StoredConnection>,
        cx: &App,
    ) -> Vec<StoredConnection> {
        let sort_field = Self::connection_list_sort_field(cx);
        let sort_order = Self::connection_list_sort_order(cx);
        connections.sort_by(|a, b| compare_connections(a, b, sort_field, sort_order));
        connections
    }

    fn connection_list_sort_field(cx: &App) -> ConnectionListSortField {
        if cx.has_global::<AppSettings>() {
            AppSettings::global(cx).connection_list_sort_field
        } else {
            ConnectionListSortField::default()
        }
    }

    fn connection_list_sort_order(cx: &App) -> ConnectionListSortOrder {
        if cx.has_global::<AppSettings>() {
            AppSettings::global(cx).connection_list_sort_order
        } else {
            ConnectionListSortOrder::default()
        }
    }

    fn connection_list_view_mode(cx: &App) -> ConnectionListViewMode {
        if cx.has_global::<AppSettings>() {
            AppSettings::global(cx).connection_list_view_mode
        } else {
            ConnectionListViewMode::default()
        }
    }

    fn connection_sort_field_label(sort_field: ConnectionListSortField) -> String {
        match sort_field {
            ConnectionListSortField::Name => t!("Home.sort_by_name").to_string(),
            ConnectionListSortField::CreatedAt => t!("Home.sort_by_created_at").to_string(),
            ConnectionListSortField::Manual => t!("Home.sort_by_manual").to_string(),
            ConnectionListSortField::UpdatedAt => t!("Home.sort_by_updated_at").to_string(),
        }
    }

    fn connection_sort_order_label(sort_order: ConnectionListSortOrder) -> String {
        match sort_order {
            ConnectionListSortOrder::Ascending => t!("Home.sort_order_ascending").to_string(),
            ConnectionListSortOrder::Descending => t!("Home.sort_order_descending").to_string(),
        }
    }

    fn update_connection_list_preferences(
        &mut self,
        update: impl FnOnce(&mut AppSettings),
        cx: &mut Context<Self>,
    ) {
        cx.update_global::<AppSettings, _>(|settings, _| {
            update(settings);
            settings.save();
        });
        cx.notify();
    }

    fn set_connection_list_sort_field(
        &mut self,
        sort_field: ConnectionListSortField,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_list_preferences(
            move |settings| {
                settings.connection_list_sort_field = sort_field;
            },
            cx,
        );
    }

    fn set_connection_list_view_mode(
        &mut self,
        view_mode: ConnectionListViewMode,
        cx: &mut Context<Self>,
    ) {
        self.update_connection_list_preferences(
            move |settings| {
                settings.connection_list_view_mode = view_mode;
            },
            cx,
        );
    }

    fn is_manual_sort_mode(cx: &App) -> bool {
        Self::connection_list_sort_field(cx) == ConnectionListSortField::Manual
    }

    fn clear_manual_drop_preview(&mut self, cx: &mut Context<Self>) {
        let had_preview = self.workspace_drop_preview.take().is_some()
            || self.connection_drop_preview.take().is_some()
            || self.connection_workspace_drop_target.take().is_some();
        if had_preview {
            cx.notify();
        }
    }

    fn update_workspace_drop_preview(
        &mut self,
        target_workspace_id: i64,
        position: ManualInsertPosition,
        cx: &mut Context<Self>,
    ) {
        let next_preview = Some(WorkspaceDropPreview {
            target_workspace_id,
            position,
        });
        if self.workspace_drop_preview == next_preview
            && self.connection_drop_preview.is_none()
            && self.connection_workspace_drop_target.is_none()
        {
            return;
        }

        self.workspace_drop_preview = next_preview;
        self.connection_drop_preview = None;
        self.connection_workspace_drop_target = None;
        cx.notify();
    }

    fn update_connection_drop_preview(
        &mut self,
        workspace_id: Option<i64>,
        target_connection_id: i64,
        position: ManualInsertPosition,
        edge: ManualDropIndicatorEdge,
        cx: &mut Context<Self>,
    ) {
        let next_preview = Some(ConnectionDropPreview {
            workspace_id,
            target_connection_id,
            position,
            edge,
        });
        if self.connection_drop_preview == next_preview
            && self.workspace_drop_preview.is_none()
            && self.connection_workspace_drop_target.is_none()
        {
            return;
        }

        self.connection_drop_preview = next_preview;
        self.workspace_drop_preview = None;
        self.connection_workspace_drop_target = None;
        cx.notify();
    }

    fn update_connection_workspace_drop_target(
        &mut self,
        target_workspace_id: i64,
        cx: &mut Context<Self>,
    ) {
        let next_target = Some(target_workspace_id);
        if self.connection_workspace_drop_target == next_target
            && self.workspace_drop_preview.is_none()
            && self.connection_drop_preview.is_none()
        {
            return;
        }

        self.connection_workspace_drop_target = next_target;
        self.workspace_drop_preview = None;
        self.connection_drop_preview = None;
        cx.notify();
    }

    fn set_dragging_connection_id(&mut self, connection_id: i64, cx: &mut Context<Self>) {
        if self.dragging_connection_id == Some(connection_id) {
            return;
        }

        self.dragging_connection_id = Some(connection_id);
        cx.notify();
    }

    fn render_manual_drop_indicator(position: ManualInsertPosition, cx: &App) -> AnyElement {
        let edge = match position {
            ManualInsertPosition::Before => ManualDropIndicatorEdge::Top,
            ManualInsertPosition::After => ManualDropIndicatorEdge::Bottom,
        };
        Self::render_manual_drop_indicator_for_edge(edge, cx)
    }

    fn render_manual_drop_indicator_for_edge(
        edge: ManualDropIndicatorEdge,
        cx: &App,
    ) -> AnyElement {
        let indicator = div().absolute().rounded_full().bg(cx.theme().drag_border);

        match edge {
            ManualDropIndicatorEdge::Top => indicator.left_3().right_3().top_0().h(px(3.0)),
            ManualDropIndicatorEdge::Bottom => indicator.left_3().right_3().bottom_0().h(px(3.0)),
            ManualDropIndicatorEdge::Left => indicator.left_0().top_3().bottom_3().w(px(3.0)),
            ManualDropIndicatorEdge::Right => indicator.right_0().top_3().bottom_3().w(px(3.0)),
        }
        .into_any_element()
    }

    fn render_connection_card_overlay_indicator(
        &self,
        bounds: Bounds<Pixels>,
        cx: &App,
    ) -> AnyElement {
        div()
            .absolute()
            .left(bounds.origin.x)
            .top(bounds.origin.y)
            .w(bounds.size.width)
            .h(bounds.size.height)
            .rounded_lg()
            .border_2()
            .border_dashed()
            .border_color(cx.theme().drag_border)
            .bg(cx.theme().drop_target.opacity(0.16))
            .into_any_element()
    }

    fn update_workspace_drag_preview_size(&mut self, workspace_id: i64, size: DragPreviewSize) {
        self.workspace_drag_preview_sizes.insert(workspace_id, size);
    }

    fn update_connection_list_drag_preview_size(
        &mut self,
        connection_id: i64,
        size: DragPreviewSize,
    ) {
        self.connection_list_drag_preview_sizes
            .insert(connection_id, size);
    }

    fn update_connection_card_drag_preview_size(
        &mut self,
        connection_id: i64,
        size: DragPreviewSize,
    ) {
        self.connection_card_drag_preview_sizes
            .insert(connection_id, size);
    }

    fn update_connection_card_bounds(&mut self, connection_id: i64, bounds: Bounds<Pixels>) {
        self.connection_card_bounds.insert(connection_id, bounds);
    }

    fn update_connection_grid_bounds(&mut self, workspace_id: Option<i64>, bounds: Bounds<Pixels>) {
        self.connection_grid_bounds.insert(workspace_id, bounds);
    }

    fn preview_for_connection_card_gap(
        &self,
        visible_connection_ids: &[i64],
        workspace_id: Option<i64>,
        dragged_connection_id: i64,
        position: Point<Pixels>,
    ) -> Option<ConnectionDropPreview> {
        let visible_card_bounds = visible_connection_ids
            .iter()
            .filter_map(|&connection_id| {
                if connection_id == dragged_connection_id {
                    return None;
                }

                self.connection_card_bounds
                    .get(&connection_id)
                    .copied()
                    .map(|bounds| (connection_id, bounds))
            })
            .collect::<Vec<_>>();

        preview_for_connection_card_gap_from_bounds(workspace_id, position, &visible_card_bounds)
    }

    fn connection_card_overlay_preview_bounds(
        &self,
        workspace_id: Option<i64>,
        visible_connection_ids: &[i64],
    ) -> Option<Bounds<Pixels>> {
        let preview = self
            .connection_drop_preview
            .filter(|preview| preview.workspace_id == workspace_id)?;
        let grid_bounds = self.connection_grid_bounds.get(&workspace_id).copied()?;
        let card_bounds = visible_connection_ids
            .iter()
            .filter_map(|connection_id| {
                self.connection_card_bounds
                    .get(connection_id)
                    .copied()
                    .map(|bounds| (*connection_id, bounds))
            })
            .collect::<Vec<_>>();

        connection_card_overlay_preview_bounds_from_bounds(
            preview,
            visible_connection_ids,
            grid_bounds,
            &card_bounds,
        )
    }

    fn reorder_workspaces_to_end_manually(
        &mut self,
        dragged_workspace_id: i64,
        last_workspace_id: i64,
        cx: &mut Context<Self>,
    ) {
        if dragged_workspace_id == last_workspace_id {
            self.clear_manual_drop_preview(cx);
            return;
        }

        self.reorder_workspaces_manually(
            dragged_workspace_id,
            last_workspace_id,
            ManualInsertPosition::After,
            cx,
        );
    }

    fn reorder_connections_to_end_manually(
        &mut self,
        workspace_id: Option<i64>,
        dragged_connection_id: i64,
        last_connection_id: i64,
        cx: &mut Context<Self>,
    ) {
        if dragged_connection_id == last_connection_id {
            self.clear_manual_drop_preview(cx);
            return;
        }

        self.reorder_connections_manually_at(
            workspace_id,
            dragged_connection_id,
            last_connection_id,
            ManualInsertPosition::After,
            cx,
        );
    }

    fn apply_workspace_manual_order(&mut self, ordered_workspace_ids: &[i64]) {
        for (sort_order, workspace_id) in ordered_workspace_ids.iter().enumerate() {
            if let Some(workspace) = self
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == Some(*workspace_id))
            {
                workspace.sort_order = Some(sort_order as i64);
            }
        }
    }

    fn reorder_workspaces_manually(
        &mut self,
        dragged_workspace_id: i64,
        target_workspace_id: i64,
        position: ManualInsertPosition,
        cx: &mut Context<Self>,
    ) {
        if dragged_workspace_id == target_workspace_id {
            return;
        }

        let mut ordered_workspace_ids: Vec<i64> = self
            .workspaces
            .iter()
            .filter_map(|workspace| workspace.id)
            .collect();
        ordered_workspace_ids.sort_by(|a, b| {
            let left = self
                .workspaces
                .iter()
                .find(|workspace| workspace.id == Some(*a))
                .expect("工作区 ID 已存在于当前列表");
            let right = self
                .workspaces
                .iter()
                .find(|workspace| workspace.id == Some(*b))
                .expect("工作区 ID 已存在于当前列表");
            compare_workspaces(
                left,
                right,
                ConnectionListSortField::Manual,
                ConnectionListSortOrder::Ascending,
            )
        });

        let Some(source_index) = ordered_workspace_ids
            .iter()
            .position(|workspace_id| *workspace_id == dragged_workspace_id)
        else {
            return;
        };
        let Some(target_index) = ordered_workspace_ids
            .iter()
            .position(|workspace_id| *workspace_id == target_workspace_id)
        else {
            return;
        };

        move_item_relative_to_target(
            &mut ordered_workspace_ids,
            source_index,
            target_index,
            position,
        );
        self.apply_workspace_manual_order(&ordered_workspace_ids);
        self.workspace_drop_preview = None;
        cx.notify();

        let storage = cx.global::<GlobalStorageState>().storage.clone();
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = (|| {
                let repo = storage
                    .get::<WorkspaceRepository>()
                    .ok_or_else(|| anyhow::anyhow!("WorkspaceRepository not found"))?;
                repo.reorder(&ordered_workspace_ids)
            })();

            match result {
                Ok(()) => {
                    _ = this.update(cx, |this, cx| {
                        if this.current_user.is_some() && crypto::has_master_key() {
                            this.trigger_sync(cx);
                        }
                    });
                }
                Err(error) => {
                    _ = this.update(cx, |this, cx| {
                        this.load_workspaces(cx);
                        let message = t!("Home.manual_sort_save_failed", error = error.to_string())
                            .to_string();
                        this.set_sync_feedback(SyncFeedbackLevel::Error, message);
                        if let Some(feedback) = &this.sync_feedback {
                            Self::push_sync_notification(feedback, cx);
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn apply_connection_manual_order(
        &mut self,
        workspace_id: Option<i64>,
        ordered_connection_ids: &[i64],
    ) {
        for (sort_order, connection_id) in ordered_connection_ids.iter().enumerate() {
            if let Some(connection) = self.connections.iter_mut().find(|connection| {
                connection.id == Some(*connection_id) && connection.workspace_id == workspace_id
            }) {
                connection.sort_order = Some(sort_order as i64);
            }
        }
    }

    fn reorder_connections_manually_at(
        &mut self,
        workspace_id: Option<i64>,
        dragged_connection_id: i64,
        target_connection_id: i64,
        position: ManualInsertPosition,
        cx: &mut Context<Self>,
    ) {
        if dragged_connection_id == target_connection_id {
            return;
        }

        let mut ordered_connection_ids =
            ordered_connection_ids_for_workspace(&self.connections, workspace_id);

        let Some(source_index) = ordered_connection_ids
            .iter()
            .position(|connection_id| *connection_id == dragged_connection_id)
        else {
            return;
        };
        let Some(target_index) = ordered_connection_ids
            .iter()
            .position(|connection_id| *connection_id == target_connection_id)
        else {
            return;
        };

        move_item_relative_to_target(
            &mut ordered_connection_ids,
            source_index,
            target_index,
            position,
        );
        self.apply_connection_manual_order(workspace_id, &ordered_connection_ids);
        self.connection_drop_preview = None;
        cx.notify();

        let storage = cx.global::<GlobalStorageState>().storage.clone();
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = (|| {
                let repo = storage
                    .get::<ConnectionRepository>()
                    .ok_or_else(|| anyhow::anyhow!("ConnectionRepository not found"))?;
                repo.reorder_within_workspace(workspace_id, &ordered_connection_ids)
            })();

            match result {
                Ok(()) => {
                    _ = this.update(cx, |this, cx| {
                        if this.current_user.is_some() && crypto::has_master_key() {
                            this.trigger_sync(cx);
                        }
                    });
                }
                Err(error) => {
                    _ = this.update(cx, |this, cx| {
                        this.load_connections(cx);
                        let message = t!("Home.manual_sort_save_failed", error = error.to_string())
                            .to_string();
                        this.set_sync_feedback(SyncFeedbackLevel::Error, message);
                        if let Some(feedback) = &this.sync_feedback {
                            Self::push_sync_notification(feedback, cx);
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn apply_connection_workspace_move_plan(
        &mut self,
        connection_id: i64,
        plan: &ConnectionWorkspaceMovePlan,
    ) {
        if let Some(connection) = self
            .connections
            .iter_mut()
            .find(|connection| connection.id == Some(connection_id))
        {
            connection.workspace_id = plan.target_workspace_id;
        }
        self.apply_connection_manual_order(plan.source_workspace_id, &plan.source_connection_ids);
        self.apply_connection_manual_order(plan.target_workspace_id, &plan.target_connection_ids);
    }

    fn move_connection_with_plan(
        &mut self,
        connection_id: i64,
        plan: ConnectionWorkspaceMovePlan,
        cx: &mut Context<Self>,
    ) {
        self.clear_manual_drop_preview(cx);
        self.apply_connection_workspace_move_plan(connection_id, &plan);
        cx.notify();

        let storage = cx.global::<GlobalStorageState>().storage.clone();
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let result = (|| -> anyhow::Result<StoredConnection> {
                let repo = storage
                    .get::<ConnectionRepository>()
                    .ok_or_else(|| anyhow::anyhow!("ConnectionRepository not found"))?;
                repo.move_across_workspaces(
                    connection_id,
                    plan.source_workspace_id,
                    plan.target_workspace_id,
                    &plan.source_connection_ids,
                    &plan.target_connection_ids,
                )?;
                repo.get(connection_id)?
                    .ok_or_else(|| anyhow::anyhow!("连接 {} 更新后丢失", connection_id))
            })();

            match result {
                Ok(updated_connection) => {
                    _ = this.update(cx, |this, cx| {
                        if let Some(position) = this
                            .connections
                            .iter()
                            .position(|connection| connection.id == updated_connection.id)
                        {
                            this.connections[position] = updated_connection.clone();
                        } else {
                            this.connections.push(updated_connection.clone());
                        }

                        emit_connection_event(
                            ConnectionDataEvent::ConnectionUpdated {
                                connection: updated_connection,
                            },
                            cx,
                        );
                        cx.notify();
                    });
                }
                Err(error) => {
                    _ = this.update(cx, |this, cx| {
                        this.load_connections(cx);
                        let message = t!("Home.manual_sort_save_failed", error = error.to_string())
                            .to_string();
                        this.set_sync_feedback(SyncFeedbackLevel::Error, message);
                        if let Some(feedback) = &this.sync_feedback {
                            Self::push_sync_notification(feedback, cx);
                        }
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    fn move_connection_to_workspace_at(
        &mut self,
        connection_id: i64,
        target_workspace_id: i64,
        target_connection_id: i64,
        position: ManualInsertPosition,
        cx: &mut Context<Self>,
    ) {
        let Some(plan) = plan_connection_move_to_workspace_position(
            &self.connections,
            connection_id,
            Some(target_workspace_id),
            target_connection_id,
            position,
        ) else {
            self.clear_manual_drop_preview(cx);
            return;
        };

        self.move_connection_with_plan(connection_id, plan, cx);
    }

    fn move_connection_to_workspace_end(
        &mut self,
        connection_id: i64,
        target_workspace_id: i64,
        cx: &mut Context<Self>,
    ) {
        let Some(plan) = plan_connection_move_to_workspace_end(
            &self.connections,
            connection_id,
            Some(target_workspace_id),
        ) else {
            self.clear_manual_drop_preview(cx);
            return;
        };

        self.move_connection_with_plan(connection_id, plan, cx);
    }

    fn toggle_connection_list_sort_order(&mut self, cx: &mut Context<Self>) {
        if Self::connection_list_sort_field(cx) == ConnectionListSortField::Manual {
            return;
        }
        self.update_connection_list_preferences(
            |settings| {
                settings.connection_list_sort_order = match settings.connection_list_sort_order {
                    ConnectionListSortOrder::Ascending => ConnectionListSortOrder::Descending,
                    ConnectionListSortOrder::Descending => ConnectionListSortOrder::Ascending,
                };
            },
            cx,
        );
    }

    fn connection_subtitle(&self, conn: &StoredConnection) -> Option<String> {
        match conn.connection_type {
            ConnectionType::Database => conn.to_db_connection().ok().map(|params| {
                if matches!(
                    params.database_type,
                    DatabaseType::SQLite | DatabaseType::DuckDB
                ) {
                    params.host
                } else {
                    let database = match params.database {
                        Some(database) => format!("/{}", database),
                        None => String::new(),
                    };
                    format!(
                        "{}@{}:{}{}",
                        params.username, params.host, params.port, database
                    )
                }
            }),
            ConnectionType::SshSftp => conn
                .to_ssh_params()
                .ok()
                .map(|params| format!("{}@{}:{}", params.username, params.host, params.port)),
            ConnectionType::Redis => conn.to_redis_params().ok().map(|params| match params.mode {
                RedisMode::Standalone => {
                    format!("{}:{}/{}", params.host, params.port, params.db_index)
                }
                RedisMode::Sentinel => {
                    let (master_name, sentinel_count) = params
                        .sentinel
                        .as_ref()
                        .map(|sentinel| (sentinel.master_name.as_str(), sentinel.sentinels.len()))
                        .unwrap_or(("sentinel", 0));
                    format!("{} (sentinel:{})", master_name, sentinel_count)
                }
                RedisMode::Cluster => {
                    let node_count = params
                        .cluster
                        .as_ref()
                        .map(|cluster| cluster.nodes.len())
                        .unwrap_or(0);
                    format!("cluster ({} nodes)", node_count)
                }
            }),
            ConnectionType::MongoDB => conn.to_mongodb_params().ok().map(|params| {
                if !params.host.is_empty() {
                    if let Some(port) = params.port {
                        format!("{}:{}", params.host, port)
                    } else {
                        params.host
                    }
                } else if !params.connection_string.is_empty() {
                    params.connection_string
                } else {
                    "MongoDB".to_string()
                }
            }),
            ConnectionType::Serial => conn.to_serial_params().ok().map(|params| {
                let parity_char = match params.parity {
                    one_core::storage::models::SerialParity::None => 'N',
                    one_core::storage::models::SerialParity::Odd => 'O',
                    one_core::storage::models::SerialParity::Even => 'E',
                };
                format!(
                    "{} ({}, {}{}{})",
                    params.port_name,
                    params.baud_rate,
                    params.data_bits,
                    parity_char,
                    params.stop_bits,
                )
            }),
            _ => None,
        }
    }

    fn render_connection_icon(&self, conn: &StoredConnection, size: f32) -> AnyElement {
        let icon = match conn.connection_type {
            ConnectionType::Database => conn
                .to_db_connection()
                .map(|c| c.database_type.as_icon())
                .unwrap_or_else(|_| IconName::Database.color())
                .with_size(px(size))
                .text_color(gpui::white()),
            ConnectionType::SshSftp => IconName::TerminalColor
                .color()
                .with_size(px(size))
                .text_color(gpui::rgb(0x8b5cf6)),
            ConnectionType::Redis => IconName::Redis
                .color()
                .with_size(px(size))
                .text_color(gpui::white()),
            ConnectionType::MongoDB => IconName::MongoDB
                .color()
                .with_size(px(size))
                .text_color(gpui::white()),
            ConnectionType::Serial => IconName::SerialPort
                .color()
                .with_size(px(size))
                .text_color(gpui::white()),
            _ => IconName::Server
                .color()
                .with_size(px(size))
                .text_color(gpui::white()),
        };

        icon.into_any_element()
    }

    fn render_workspace_view(
        &self,
        search_query: &str,
        selected_id: Option<i64>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        const WORKSPACE_GAP_HEIGHT: f32 = 12.0;
        let manual_sort_mode = Self::is_manual_sort_mode(cx);
        let workspaces_with_connections: Vec<_> = self
            .workspaces
            .iter()
            .filter(|ws| {
                if self.filtered_workspace_ids.is_empty() {
                    return true;
                }
                match ws.id {
                    Some(id) => self.filtered_workspace_ids.contains(&id),
                    None => true,
                }
            })
            .map(|ws| {
                let conn_list: Vec<_> = self
                    .connections
                    .iter()
                    .filter(|conn| conn.workspace_id == ws.id)
                    .filter(|conn| self.match_connection(conn, search_query))
                    .filter(|conn| self.match_connection_type(conn))
                    .cloned()
                    .collect();
                let conn_list = self.sort_connections_for_display(conn_list, cx);
                (ws.clone(), conn_list)
            })
            .collect();

        let sort_field = Self::connection_list_sort_field(cx);
        let sort_order = Self::connection_list_sort_order(cx);
        let mut workspaces_with_connections = workspaces_with_connections;
        workspaces_with_connections
            .sort_by(|(a, _), (b, _)| compare_workspaces(a, b, sort_field, sort_order));
        let visible_workspaces: Vec<_> = workspaces_with_connections
            .into_iter()
            .filter(|(_, connections)| !connections.is_empty())
            .collect();
        let last_visible_workspace_id = visible_workspaces
            .last()
            .and_then(|(workspace, _)| workspace.id);

        let unassigned_connections = self.sort_connections_for_display(
            self.connections
                .iter()
                .filter(|conn| conn.workspace_id.is_none())
                .filter(|conn| self.match_connection(conn, search_query))
                .filter(|conn| self.match_connection_type(conn))
                .cloned()
                .collect(),
            cx,
        );

        div()
            .id("home-content")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .p_3()
            .child({
                let mut container = v_flex().gap_0().w_full();

                for (index, (workspace, connections)) in
                    visible_workspaces.iter().cloned().enumerate()
                {
                    if index > 0 {
                        if manual_sort_mode {
                            let target_workspace_id =
                                workspace.id.expect("工作区排序占位目标必须存在工作区 ID");
                            let zone_active = self.workspace_drop_preview
                                == Some(WorkspaceDropPreview {
                                    target_workspace_id,
                                    position: ManualInsertPosition::Before,
                                })
                                && cx.has_active_drag();
                            container = container.child(
                                div()
                                    .w_full()
                                    .h(px(WORKSPACE_GAP_HEIGHT))
                                    .rounded_lg()
                                    .relative()
                                    .overflow_hidden()
                                    .on_drag_move(cx.listener(
                                        move |this, drag: &DragMoveEvent<DragWorkspace>, _, cx| {
                                            if !drag.bounds.contains(&drag.event.position) {
                                                return;
                                            }

                                            let drag_workspace = drag.drag(cx);
                                            if drag_workspace.workspace_id == target_workspace_id {
                                                this.clear_manual_drop_preview(cx);
                                                return;
                                            }

                                            this.update_workspace_drop_preview(
                                                target_workspace_id,
                                                ManualInsertPosition::Before,
                                                cx,
                                            );
                                        },
                                    ))
                                    .drag_over::<DragWorkspace>(move |this, drag, _, cx| {
                                        if drag.workspace_id == target_workspace_id {
                                            this
                                        } else {
                                            this.bg(cx.theme().drop_target.opacity(0.2))
                                        }
                                    })
                                    .on_drop(cx.listener(
                                        move |this, drag: &DragWorkspace, _, cx| {
                                            cx.stop_propagation();
                                            this.reorder_workspaces_manually(
                                                drag.workspace_id,
                                                target_workspace_id,
                                                ManualInsertPosition::Before,
                                                cx,
                                            );
                                        },
                                    ))
                                    .when(cx.has_active_drag() || zone_active, |this| {
                                        this.child(
                                            div()
                                                .absolute()
                                                .left_3()
                                                .right_3()
                                                .top(px((WORKSPACE_GAP_HEIGHT - 2.0) / 2.0))
                                                .h(px(2.0))
                                                .rounded_full()
                                                .bg(cx
                                                    .theme()
                                                    .drag_border
                                                    .opacity(if zone_active { 1.0 } else { 0.45 })),
                                        )
                                    }),
                            );
                        } else {
                            container = container.child(div().w_full().h(px(WORKSPACE_GAP_HEIGHT)));
                        }
                    }

                    container = container.child(self.render_workspace_section(
                        workspace,
                        connections,
                        selected_id,
                        cx,
                    ));
                }

                if manual_sort_mode {
                    if let Some(last_workspace_id) = last_visible_workspace_id {
                        let zone_active = self.workspace_drop_preview
                            == Some(WorkspaceDropPreview {
                                target_workspace_id: last_workspace_id,
                                position: ManualInsertPosition::After,
                            })
                            && cx.has_active_drag();
                        container = container.child(
                            div()
                                .w_full()
                                .min_h(px(36.0))
                                .rounded_lg()
                                .relative()
                                .overflow_hidden()
                                .on_drag_move(cx.listener(
                                    move |this, drag: &DragMoveEvent<DragWorkspace>, _, cx| {
                                        if !drag.bounds.contains(&drag.event.position) {
                                            return;
                                        }

                                        let drag_workspace = drag.drag(cx);
                                        if drag_workspace.workspace_id == last_workspace_id {
                                            this.clear_manual_drop_preview(cx);
                                            return;
                                        }

                                        this.update_workspace_drop_preview(
                                            last_workspace_id,
                                            ManualInsertPosition::After,
                                            cx,
                                        );
                                    },
                                ))
                                .drag_over::<DragWorkspace>(move |this, drag, _, cx| {
                                    if drag.workspace_id == last_workspace_id {
                                        this
                                    } else {
                                        this.bg(cx.theme().drop_target.opacity(0.25))
                                    }
                                })
                                .on_drop(cx.listener(move |this, drag: &DragWorkspace, _, cx| {
                                    cx.stop_propagation();
                                    this.reorder_workspaces_to_end_manually(
                                        drag.workspace_id,
                                        last_workspace_id,
                                        cx,
                                    );
                                }))
                                .when(cx.has_active_drag() || zone_active, |this| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .left_3()
                                            .right_3()
                                            .top(px(17.0))
                                            .h(px(2.0))
                                            .rounded_full()
                                            .bg(cx.theme().drag_border.opacity(if zone_active {
                                                1.0
                                            } else {
                                                0.45
                                            })),
                                    )
                                }),
                        );
                    }
                }

                // 如果用户没有设置工作区，直接显示连接列表；否则显示未分配工作区
                if !unassigned_connections.is_empty() {
                    let has_workspaces = self.workspaces.iter().any(|ws| ws.id.is_some());
                    if has_workspaces {
                        if !manual_sort_mode && !visible_workspaces.is_empty() {
                            container = container.child(div().w_full().h(px(WORKSPACE_GAP_HEIGHT)));
                        }
                        container = container.child(self.render_unassigned_section(
                            unassigned_connections,
                            selected_id,
                            cx,
                        ));
                    } else {
                        container = container.child(self.render_connections_collection(
                            unassigned_connections,
                            None,
                            selected_id,
                            cx,
                        ));
                    }
                }

                container
            })
    }

    fn render_workspace_section(
        &self,
        workspace: Workspace,
        connections: Vec<StoredConnection>,
        selected_id: Option<i64>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let blur_enabled = cx.theme().window_blur_enabled;
        let window_opacity = cx.theme().backdrop_opacity;
        let workspace_bg = layered_level_surface_color(
            cx.theme().list,
            blur_enabled,
            window_opacity,
            3,
            WindowsSurfaceLayer::ContentSection,
        );
        let workspace_hover_bg = layered_level_surface_color(
            cx.theme().table_hover,
            blur_enabled,
            window_opacity,
            2,
            WindowsSurfaceLayer::ContentSection,
        );
        let workspace_id = workspace.id;
        let is_collapsed = workspace_id
            .map(|id| self.collapsed_workspaces.contains(&id))
            .unwrap_or(false);
        let manual_sort_mode = Self::is_manual_sort_mode(cx);
        let view = cx.entity().clone();
        let draggable_workspace_id = workspace_id;
        let draggable_workspace_name: SharedString = workspace.name.clone().into();
        let workspace_preview_size = workspace_id.and_then(|workspace_id| {
            self.workspace_drag_preview_sizes
                .get(&workspace_id)
                .copied()
        });
        let workspace_drop_indicator = workspace_id
            .and_then(|workspace_id| {
                self.workspace_drop_preview
                    .filter(|preview| preview.target_workspace_id == workspace_id)
                    .map(|preview| preview.position)
            })
            .filter(|_| manual_sort_mode && cx.has_active_drag());
        let connection_workspace_drop_active = workspace_id
            .map(|workspace_id| {
                manual_sort_mode
                    && cx.has_active_drag()
                    && self.connection_workspace_drop_target == Some(workspace_id)
            })
            .unwrap_or(false);
        v_flex()
            .gap_0()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().table_row_border)
            .bg(workspace_bg)
            .overflow_hidden()
            .child(
                h_flex()
                    .id(ElementId::Name(SharedString::from(format!(
                        "workspace-header-{}",
                        workspace_id.unwrap_or(0)
                    ))))
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .relative()
                    .overflow_hidden()
                    .cursor_pointer()
                    .bg(cx.theme().table_head)
                    .when(manual_sort_mode && workspace_id.is_some(), |this| {
                        this.cursor_grab()
                    })
                    .rounded_t_lg()
                    .when(!is_collapsed, |this| {
                        this.border_b_1().border_color(cx.theme().table_row_border)
                    })
                    .when(is_collapsed, |this| this.rounded_b_lg())
                    .hover(|s| s.bg(workspace_hover_bg))
                    .when(connection_workspace_drop_active, |this| {
                        this.border_color(cx.theme().table_active_border)
                            .bg(cx.theme().table_active.opacity(0.28))
                    })
                    .when(
                        manual_sort_mode && draggable_workspace_id.is_some(),
                        |this| {
                            let workspace_id = draggable_workspace_id.expect("工作区 ID 应存在");
                            let workspace_name = draggable_workspace_name.clone();
                            this.on_drag(
                                DragWorkspace {
                                    workspace_id,
                                    name: workspace_name,
                                    preview_size: workspace_preview_size,
                                },
                                |drag, _, _, cx| {
                                    cx.stop_propagation();
                                    cx.new(|_| drag.clone())
                                },
                            )
                            .on_drag_move(cx.listener(
                                move |this, drag: &DragMoveEvent<DragWorkspace>, _, cx| {
                                    if !drag.bounds.contains(&drag.event.position) {
                                        return;
                                    }

                                    let drag_workspace = drag.drag(cx);
                                    if drag_workspace.workspace_id == workspace_id {
                                        this.clear_manual_drop_preview(cx);
                                        return;
                                    }

                                    let current_position = this
                                        .workspace_drop_preview
                                        .filter(|preview| {
                                            preview.target_workspace_id == workspace_id
                                        })
                                        .map(|preview| preview.position);
                                    let position =
                                        insert_position_from_drag(drag, current_position);
                                    this.update_workspace_drop_preview(workspace_id, position, cx);
                                },
                            ))
                            .drag_over::<DragWorkspace>(move |this, drag, _, cx| {
                                if drag.workspace_id == workspace_id {
                                    this
                                } else {
                                    this.border_color(cx.theme().table_active_border)
                                        .bg(cx.theme().table_active.opacity(0.35))
                                }
                            })
                            .on_drop(cx.listener(
                                move |this, drag: &DragWorkspace, _, cx| {
                                    cx.stop_propagation();
                                    let position = this
                                        .workspace_drop_preview
                                        .filter(|preview| {
                                            preview.target_workspace_id == workspace_id
                                        })
                                        .map(|preview| preview.position)
                                        .unwrap_or(ManualInsertPosition::After);
                                    this.reorder_workspaces_manually(
                                        drag.workspace_id,
                                        workspace_id,
                                        position,
                                        cx,
                                    );
                                },
                            ))
                        },
                    )
                    .when(manual_sort_mode && workspace_id.is_some(), |this| {
                        let workspace_id = workspace_id.expect("工作区 ID 应存在");
                        this.on_drag_move(cx.listener(
                            move |this, drag: &DragMoveEvent<DragConnection>, _, cx| {
                                if !drag.bounds.contains(&drag.event.position) {
                                    return;
                                }

                                let drag_connection = drag.drag(cx);
                                if !can_drop_connection_on_workspace(
                                    drag_connection.workspace_id,
                                    Some(workspace_id),
                                ) {
                                    if this.connection_workspace_drop_target == Some(workspace_id) {
                                        this.clear_manual_drop_preview(cx);
                                    }
                                    return;
                                }

                                this.update_connection_workspace_drop_target(workspace_id, cx);
                            },
                        ))
                        .drag_over::<DragConnection>(move |this, drag, _, cx| {
                            if !can_drop_connection_on_workspace(
                                drag.workspace_id,
                                Some(workspace_id),
                            ) {
                                this
                            } else {
                                this.border_color(cx.theme().table_active_border)
                                    .bg(cx.theme().table_active.opacity(0.35))
                            }
                        })
                        .on_drop(cx.listener(
                            move |this, drag: &DragConnection, _, cx| {
                                cx.stop_propagation();
                                if !can_drop_connection_on_workspace(
                                    drag.workspace_id,
                                    Some(workspace_id),
                                ) {
                                    this.clear_manual_drop_preview(cx);
                                    return;
                                }

                                this.move_connection_to_workspace_end(
                                    drag.connection_id,
                                    workspace_id,
                                    cx,
                                );
                            },
                        ))
                    })
                    .when_some(workspace_id, |this, workspace_id| {
                        let view = view.clone();
                        this.on_prepaint(move |bounds, _, cx| {
                            let size = DragPreviewSize {
                                width: f32::from(bounds.size.width),
                                height: f32::from(bounds.size.height),
                            };
                            view.update(cx, |this, _| {
                                this.update_workspace_drag_preview_size(workspace_id, size);
                            });
                        })
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(id) = workspace_id {
                            if this.collapsed_workspaces.contains(&id) {
                                this.collapsed_workspaces.remove(&id);
                            } else {
                                this.collapsed_workspaces.insert(id);
                            }
                            cx.notify();
                        }
                    }))
                    .child(
                        Icon::new(if is_collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .with_size(Size::Small)
                        .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        Icon::new(IconName::AppsColor)
                            .color()
                            .with_size(Size::Medium),
                    )
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().table_head_foreground)
                            .child(workspace.name.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                t!("Home.connection_count", count = connections.len()).to_string(),
                            ),
                    )
                    .child(div().flex_1())
                    .when_some(workspace_id, |this, workspace_id| {
                        this.child(
                            h_flex()
                                .gap_1()
                                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation()
                                })
                                .child(
                                    Button::new(format!("workspace-edit-{}", workspace_id))
                                        .icon(IconName::Edit)
                                        .xsmall()
                                        .ghost()
                                        .tooltip(t!("Workspace.edit"))
                                        .on_click(cx.listener(move |_this, _, window, cx| {
                                            let parent = cx.entity();
                                            show_workspace_dialog(
                                                parent,
                                                Some(workspace_id),
                                                String::new(),
                                                window,
                                                cx,
                                            );
                                        })),
                                )
                                .child(
                                    Button::new(format!("workspace-delete-{}", workspace_id))
                                        .icon(IconName::Remove)
                                        .xsmall()
                                        .ghost()
                                        .tooltip(t!("Workspace.delete"))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.delete_workspace(workspace_id, window, cx);
                                        })),
                                ),
                        )
                    })
                    .when_some(workspace_drop_indicator, |this, position| {
                        this.child(Self::render_manual_drop_indicator(position, cx))
                    }),
            )
            .when(!connections.is_empty() && !is_collapsed, |this| {
                this.child(
                    div()
                        .p_3()
                        .when(manual_sort_mode && workspace_id.is_some(), |this| {
                            let workspace_id = workspace_id.expect("工作区 ID 应存在");
                            this.on_drag_move(cx.listener(
                                move |this, drag: &DragMoveEvent<DragConnection>, _, cx| {
                                    if !drag.bounds.contains(&drag.event.position) {
                                        return;
                                    }

                                    let drag_connection = drag.drag(cx);
                                    if can_drop_connection_on_workspace(
                                        drag_connection.workspace_id,
                                        Some(workspace_id),
                                    ) && this
                                        .connection_drop_preview
                                        .map(|preview| preview.workspace_id)
                                        != Some(Some(workspace_id))
                                    {
                                        this.update_connection_workspace_drop_target(
                                            workspace_id,
                                            cx,
                                        );
                                    }
                                },
                            ))
                            .drag_over::<DragConnection>(move |this, drag, _, cx| {
                                if can_drop_connection_on_workspace(
                                    drag.workspace_id,
                                    Some(workspace_id),
                                ) {
                                    this.bg(cx.theme().drop_target.opacity(0.22))
                                } else {
                                    this
                                }
                            })
                            .on_drop(cx.listener(
                                move |this, drag: &DragConnection, _, cx| {
                                    if !can_drop_connection_on_workspace(
                                        drag.workspace_id,
                                        Some(workspace_id),
                                    ) {
                                        return;
                                    }

                                    cx.stop_propagation();
                                    if let Some(preview) =
                                        this.connection_drop_preview.filter(|preview| {
                                            preview.workspace_id == Some(workspace_id)
                                        })
                                    {
                                        this.move_connection_to_workspace_at(
                                            drag.connection_id,
                                            workspace_id,
                                            preview.target_connection_id,
                                            preview.position,
                                            cx,
                                        );
                                    } else {
                                        this.move_connection_to_workspace_end(
                                            drag.connection_id,
                                            workspace_id,
                                            cx,
                                        );
                                    }
                                },
                            ))
                        })
                        .child(self.render_connections_collection(
                            connections,
                            workspace_id,
                            selected_id,
                            cx,
                        )),
                )
            })
    }

    fn render_connections_collection(
        &self,
        connections: Vec<StoredConnection>,
        workspace_id: Option<i64>,
        selected_id: Option<i64>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match Self::connection_list_view_mode(cx) {
            ConnectionListViewMode::Card => self
                .render_connections_grid(connections, workspace_id, selected_id, cx)
                .into_any_element(),
            ConnectionListViewMode::List => self
                .render_connections_list(connections, workspace_id, selected_id, cx)
                .into_any_element(),
        }
    }

    fn render_connections_list(
        &self,
        connections: Vec<StoredConnection>,
        workspace_id: Option<i64>,
        selected_id: Option<i64>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        const CONNECTION_LIST_GAP_HEIGHT: f32 = 8.0;
        let manual_sort_mode = Self::is_manual_sort_mode(cx);
        let last_connection_id = connections.last().and_then(|connection| connection.id);
        let mut container = v_flex().w_full().gap_0();

        for (index, conn) in connections.into_iter().enumerate() {
            if index > 0 {
                if manual_sort_mode {
                    let target_connection_id = conn.id.expect("连接排序占位目标必须存在连接 ID");
                    let zone_active = self.connection_drop_preview
                        == Some(ConnectionDropPreview {
                            workspace_id,
                            target_connection_id,
                            position: ManualInsertPosition::Before,
                            edge: ManualDropIndicatorEdge::Top,
                        })
                        && cx.has_active_drag();
                    container = container.child(
                        div()
                            .w_full()
                            .h(px(CONNECTION_LIST_GAP_HEIGHT))
                            .rounded_lg()
                            .relative()
                            .overflow_hidden()
                            .on_drag_move(cx.listener(
                                move |this, drag: &DragMoveEvent<DragConnection>, _, cx| {
                                    if !drag.bounds.contains(&drag.event.position) {
                                        return;
                                    }

                                    let drag_connection = drag.drag(cx);
                                    if drag_connection.connection_id == target_connection_id {
                                        this.clear_manual_drop_preview(cx);
                                        return;
                                    }
                                    if !can_drop_connection_on_connection_target(
                                        drag_connection.connection_id,
                                        drag_connection.workspace_id,
                                        target_connection_id,
                                        workspace_id,
                                    ) {
                                        return;
                                    }

                                    this.update_connection_drop_preview(
                                        workspace_id,
                                        target_connection_id,
                                        ManualInsertPosition::Before,
                                        ManualDropIndicatorEdge::Top,
                                        cx,
                                    );
                                },
                            ))
                            .drag_over::<DragConnection>(move |this, drag, _, cx| {
                                if !can_drop_connection_on_connection_target(
                                    drag.connection_id,
                                    drag.workspace_id,
                                    target_connection_id,
                                    workspace_id,
                                ) {
                                    this
                                } else {
                                    this.bg(cx.theme().drop_target.opacity(0.2))
                                }
                            })
                            .on_drop(cx.listener(move |this, drag: &DragConnection, _, cx| {
                                if !can_drop_connection_on_connection_target(
                                    drag.connection_id,
                                    drag.workspace_id,
                                    target_connection_id,
                                    workspace_id,
                                ) {
                                    return;
                                }
                                cx.stop_propagation();
                                if drag.workspace_id == workspace_id {
                                    this.reorder_connections_manually_at(
                                        workspace_id,
                                        drag.connection_id,
                                        target_connection_id,
                                        ManualInsertPosition::Before,
                                        cx,
                                    );
                                } else if let Some(workspace_id) = workspace_id {
                                    this.move_connection_to_workspace_at(
                                        drag.connection_id,
                                        workspace_id,
                                        target_connection_id,
                                        ManualInsertPosition::Before,
                                        cx,
                                    );
                                }
                            }))
                            .when(cx.has_active_drag() || zone_active, |this| {
                                this.child(
                                    div()
                                        .absolute()
                                        .left_3()
                                        .right_3()
                                        .top(px((CONNECTION_LIST_GAP_HEIGHT - 2.0) / 2.0))
                                        .h(px(2.0))
                                        .rounded_full()
                                        .bg(cx.theme().drag_border.opacity(if zone_active {
                                            1.0
                                        } else {
                                            0.45
                                        })),
                                )
                            }),
                    );
                } else {
                    container = container.child(div().w_full().h(px(CONNECTION_LIST_GAP_HEIGHT)));
                }
            }

            container = container.child(self.render_connection_list_item(
                conn,
                workspace_id,
                selected_id,
                cx,
            ));
        }

        let show_tail_slot = should_render_connection_grid_tail_slot(
            manual_sort_mode,
            cx.has_active_drag(),
            self.connection_drop_preview
                .map(|preview| preview.workspace_id),
            workspace_id,
        );

        if show_tail_slot {
            if let Some(last_connection_id) = last_connection_id {
                let zone_active = self.connection_drop_preview
                    == Some(ConnectionDropPreview {
                        workspace_id,
                        target_connection_id: last_connection_id,
                        position: ManualInsertPosition::After,
                        edge: ManualDropIndicatorEdge::Bottom,
                    })
                    && cx.has_active_drag();
                container = container.child(
                    div()
                        .w_full()
                        .min_h(px(28.0))
                        .rounded_lg()
                        .relative()
                        .overflow_hidden()
                        .on_drag_move(cx.listener(
                            move |this, drag: &DragMoveEvent<DragConnection>, _, cx| {
                                if !drag.bounds.contains(&drag.event.position) {
                                    return;
                                }

                                let drag_connection = drag.drag(cx);
                                if drag_connection.connection_id == last_connection_id {
                                    this.clear_manual_drop_preview(cx);
                                    return;
                                }
                                if !can_drop_connection_on_connection_target(
                                    drag_connection.connection_id,
                                    drag_connection.workspace_id,
                                    last_connection_id,
                                    workspace_id,
                                ) {
                                    return;
                                }

                                this.update_connection_drop_preview(
                                    workspace_id,
                                    last_connection_id,
                                    ManualInsertPosition::After,
                                    ManualDropIndicatorEdge::Bottom,
                                    cx,
                                );
                            },
                        ))
                        .drag_over::<DragConnection>(move |this, drag, _, cx| {
                            if !can_drop_connection_on_connection_target(
                                drag.connection_id,
                                drag.workspace_id,
                                last_connection_id,
                                workspace_id,
                            ) {
                                this
                            } else {
                                this.bg(cx.theme().drop_target.opacity(0.25))
                            }
                        })
                        .on_drop(cx.listener(move |this, drag: &DragConnection, _, cx| {
                            if !can_drop_connection_on_connection_target(
                                drag.connection_id,
                                drag.workspace_id,
                                last_connection_id,
                                workspace_id,
                            ) {
                                return;
                            }
                            cx.stop_propagation();
                            if drag.workspace_id == workspace_id {
                                this.reorder_connections_to_end_manually(
                                    workspace_id,
                                    drag.connection_id,
                                    last_connection_id,
                                    cx,
                                );
                            } else if let Some(workspace_id) = workspace_id {
                                this.move_connection_to_workspace_at(
                                    drag.connection_id,
                                    workspace_id,
                                    last_connection_id,
                                    ManualInsertPosition::After,
                                    cx,
                                );
                            }
                        }))
                        .when(cx.has_active_drag() || zone_active, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .left_3()
                                    .right_3()
                                    .top(px(13.0))
                                    .h(px(2.0))
                                    .rounded_full()
                                    .bg(cx.theme().drag_border.opacity(if zone_active {
                                        1.0
                                    } else {
                                        0.45
                                    })),
                            )
                        }),
                );
            }
        }

        container
    }

    fn render_connection_list_item(
        &self,
        conn: StoredConnection,
        workspace_id: Option<i64>,
        selected_id: Option<i64>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let blur_enabled = cx.theme().window_blur_enabled;
        let window_opacity = cx.theme().backdrop_opacity;
        let item_bg = layered_level_surface_color(
            cx.theme().list,
            blur_enabled,
            window_opacity,
            3,
            WindowsSurfaceLayer::ContentCard,
        );
        let item_icon_bg = layered_level_surface_color(
            cx.theme().muted,
            blur_enabled,
            window_opacity,
            2,
            WindowsSurfaceLayer::ContentSection,
        );
        let conn_id = conn.id;
        let clone_conn = conn.clone();
        let sftp_hover_conn = conn.clone();
        let edit_conn = conn.clone();
        let edit_conn_type = conn.connection_type;
        let edit_conn_name = conn.name.clone();
        let duplicate_conn = conn.clone();
        let delete_conn_id = conn.id;
        let delete_conn_name = conn.name.clone();
        let is_selected = selected_id == conn.id;
        let subtitle = self.connection_subtitle(&conn);
        let workspace =
            workspace_id.and_then(|id| self.workspaces.iter().find(|w| w.id == Some(id)).cloned());

        let is_active = conn
            .id
            .map_or(false, |id| cx.global::<ActiveConnections>().is_active(id));
        let manual_sort_mode = Self::is_manual_sort_mode(cx);
        let view = cx.entity().clone();
        let drag_connection_id = conn.id;
        let drag_workspace_id = workspace_id;
        let drag_connection_name: SharedString = conn.name.clone().into();
        let list_preview_size = drag_connection_id.and_then(|connection_id| {
            self.connection_list_drag_preview_sizes
                .get(&connection_id)
                .copied()
        });
        let connection_drop_indicator = drag_connection_id
            .and_then(|connection_id| {
                self.connection_drop_preview
                    .filter(|preview| {
                        preview.target_connection_id == connection_id
                            && preview.workspace_id == drag_workspace_id
                    })
                    .map(|preview| preview.edge)
            })
            .filter(|_| manual_sort_mode && cx.has_active_drag());

        h_flex()
            .justify_between()
            .items_center()
            .gap_3()
            .id(SharedString::from(format!(
                "conn-list-item-{}",
                conn.id.unwrap_or(0)
            )))
            .w_full()
            .min_h(px(38.0))
            .px_3()
            .py_1()
            .rounded_lg()
            .bg(item_bg)
            .border_1()
            .relative()
            .overflow_hidden()
            .shadow_sm()
            .cursor_pointer()
            .when(manual_sort_mode && drag_connection_id.is_some(), |this| {
                this.cursor_grab()
            })
            .when(is_selected, |this| {
                this.border_color(cx.theme().list_active_border)
                    .shadow_lg()
                    .border_l_3()
            })
            .when(!is_selected, |this| this.border_color(cx.theme().border))
            .hover(|style| {
                style
                    .shadow_lg()
                    .border_color(cx.theme().list_active_border)
            })
            .when(manual_sort_mode && drag_connection_id.is_some(), |this| {
                let connection_id = drag_connection_id.expect("连接 ID 应存在");
                let connection_name = drag_connection_name.clone();
                let view = view.clone();
                this.on_drag(
                    DragConnection {
                        connection_id,
                        workspace_id: drag_workspace_id,
                        name: connection_name,
                        preview_size: list_preview_size,
                    },
                    move |drag, _, _, cx| {
                        _ = view.update(cx, |this, cx| {
                            this.set_dragging_connection_id(connection_id, cx);
                        });
                        cx.stop_propagation();
                        cx.new(|_| drag.clone())
                    },
                )
                .on_drag_move(cx.listener(
                    move |this, drag: &DragMoveEvent<DragConnection>, _, cx| {
                        if !drag.bounds.contains(&drag.event.position) {
                            return;
                        }

                        let drag_connection = drag.drag(cx);
                        if drag_connection.connection_id == connection_id {
                            this.clear_manual_drop_preview(cx);
                            return;
                        }
                        if !can_drop_connection_on_connection_target(
                            drag_connection.connection_id,
                            drag_connection.workspace_id,
                            connection_id,
                            drag_workspace_id,
                        ) {
                            return;
                        }

                        let current_position = this
                            .connection_drop_preview
                            .filter(|preview| {
                                preview.target_connection_id == connection_id
                                    && preview.workspace_id == drag_workspace_id
                            })
                            .map(|preview| preview.position);
                        let position = insert_position_from_drag(drag, current_position);
                        let edge = match position {
                            ManualInsertPosition::Before => ManualDropIndicatorEdge::Top,
                            ManualInsertPosition::After => ManualDropIndicatorEdge::Bottom,
                        };
                        this.update_connection_drop_preview(
                            drag_workspace_id,
                            connection_id,
                            position,
                            edge,
                            cx,
                        );
                    },
                ))
                .drag_over::<DragConnection>(move |this, drag, _, cx| {
                    if !can_drop_connection_on_connection_target(
                        drag.connection_id,
                        drag.workspace_id,
                        connection_id,
                        drag_workspace_id,
                    ) {
                        this
                    } else {
                        this.border_color(cx.theme().drag_border)
                            .bg(cx.theme().drop_target.opacity(0.35))
                    }
                })
                .on_drop(cx.listener(move |this, drag: &DragConnection, _, cx| {
                    if !can_drop_connection_on_connection_target(
                        drag.connection_id,
                        drag.workspace_id,
                        connection_id,
                        drag_workspace_id,
                    ) {
                        return;
                    }
                    cx.stop_propagation();
                    let position = this
                        .connection_drop_preview
                        .filter(|preview| {
                            preview.target_connection_id == connection_id
                                && preview.workspace_id == drag_workspace_id
                        })
                        .map(|preview| preview.position)
                        .unwrap_or(ManualInsertPosition::After);
                    if drag.workspace_id == drag_workspace_id {
                        this.reorder_connections_manually_at(
                            drag_workspace_id,
                            drag.connection_id,
                            connection_id,
                            position,
                            cx,
                        );
                    } else if let Some(workspace_id) = drag_workspace_id {
                        this.move_connection_to_workspace_at(
                            drag.connection_id,
                            workspace_id,
                            connection_id,
                            position,
                            cx,
                        );
                    }
                }))
            })
            .when_some(drag_connection_id, |this, connection_id| {
                let view = view.clone();
                this.on_prepaint(move |bounds, _, cx| {
                    let size = DragPreviewSize {
                        width: f32::from(bounds.size.width),
                        height: f32::from(bounds.size.height),
                    };
                    view.update(cx, |this, _| {
                        this.update_connection_list_drag_preview_size(connection_id, size);
                    });
                })
            })
            .when_some(connection_drop_indicator, |this, edge| {
                this.child(Self::render_manual_drop_indicator_for_edge(edge, cx))
            })
            .on_double_click(cx.listener(move |this, _, window, cx| {
                if !crypto::has_master_key() && crypto::has_repo_password_set() {
                    this.show_encryption_key_dialog(window, cx);
                    return;
                }

                let strategy =
                    build_connection_open_strategy(clone_conn.clone(), workspace.clone());
                strategy.open(this, window, cx);
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected_connection_id = conn_id;
                cx.notify();
            }))
            .child(
                h_flex()
                    .items_center()
                    .gap_3()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .size(px(32.0))
                            .rounded(Radius::Lg.px())
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(item_icon_bg)
                            .child(self.render_connection_icon(&conn, 20.0)),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .flex_1()
                            .min_w_0()
                            .child({
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().foreground)
                                    .flex_shrink()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .child(conn.name.clone())
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .flex_shrink_0()
                                    .child(format!("({})", conn.connection_type.label())),
                            )
                            .when_some(subtitle, |this, subtitle| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(subtitle),
                                )
                            }),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .flex_shrink_0()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .when(conn.connection_type == ConnectionType::SshSftp, |this| {
                        this.child(
                            Button::new(SharedString::from(format!(
                                "list-sftp-conn-{}",
                                conn.id.unwrap_or(0)
                            )))
                            .icon(IconName::Folder1.color())
                            .with_size(Size::Small)
                            .primary()
                            .tooltip(t!("Home.open_sftp"))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.open_sftp_view(sftp_hover_conn.clone(), window, cx);
                                },
                            )),
                        )
                    })
                    .child(
                        Button::new(SharedString::from(format!(
                            "list-edit-conn-{}",
                            conn.id.unwrap_or(0)
                        )))
                        .icon(IconName::Edit)
                        .with_size(Size::Small)
                        .primary()
                        .tooltip(t!("Home.edit_connection"))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                if let Some(conn_id) = edit_conn.id {
                                    let conn_name = edit_conn_name.clone();
                                    match edit_conn_type {
                                        ConnectionType::SshSftp => {
                                            this.editing_connection_id = Some(conn_id);
                                            this.show_ssh_form(window, cx);
                                        }
                                        ConnectionType::Database => {
                                            let db_type = edit_conn
                                                .to_db_connection()
                                                .ok()
                                                .map(|p| p.database_type);
                                            this.confirm_edit_connection(
                                                conn_id, conn_name, db_type, window, cx,
                                            );
                                        }
                                        ConnectionType::Redis => {
                                            this.editing_connection_id = Some(conn_id);
                                            this.show_redis_form(window, cx);
                                        }
                                        ConnectionType::MongoDB => {
                                            this.editing_connection_id = Some(conn_id);
                                            this.show_mongodb_form(window, cx);
                                        }
                                        ConnectionType::Serial => {
                                            this.editing_connection_id = Some(conn_id);
                                            this.show_serial_form(window, cx);
                                        }
                                        _ => {}
                                    }
                                }
                            },
                        )),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "list-duplicate-conn-{}",
                            conn.id.unwrap_or(0)
                        )))
                        .icon(IconName::Copy)
                        .with_size(Size::Small)
                        .primary()
                        .tooltip(t!("Home.duplicate_connection"))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.duplicate_connection_and_open_editor(
                                    &duplicate_conn,
                                    window,
                                    cx,
                                );
                            },
                        )),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "list-delete-conn-{}",
                            conn.id.unwrap_or(0)
                        )))
                        .icon(IconName::Remove)
                        .with_size(Size::Small)
                        .danger()
                        .tooltip(t!("Home.delete_connection"))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                if let Some(conn_id) = delete_conn_id {
                                    let conn_name = delete_conn_name.clone();
                                    this.confirm_delete_connection(conn_id, conn_name, window, cx);
                                }
                            },
                        )),
                    ),
            )
            .when(is_active, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(5.0))
                        .left(px(5.0))
                        .w(px(8.0))
                        .h(px(8.0))
                        .rounded_full()
                        .bg(cx.theme().success)
                        .shadow_lg(),
                )
            })
            .into_any_element()
    }

    fn render_connections_grid(
        &self,
        connections: Vec<StoredConnection>,
        workspace_id: Option<i64>,
        selected_id: Option<i64>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let manual_sort_mode = Self::is_manual_sort_mode(cx);
        let last_connection_id = connections.last().and_then(|connection| connection.id);
        let visible_connection_ids = connections
            .iter()
            .filter_map(|connection| connection.id)
            .collect::<Vec<_>>();
        let overlay_preview_bounds = self
            .connection_card_overlay_preview_bounds(workspace_id, &visible_connection_ids)
            .filter(|_| manual_sort_mode && cx.has_active_drag());
        let view = cx.entity().clone();
        let mut container = div()
            .flex()
            .flex_wrap()
            .w_full()
            .gap_3()
            .relative()
            .on_prepaint(move |bounds, _, cx| {
                view.update(cx, |this, _| {
                    this.update_connection_grid_bounds(workspace_id, bounds);
                });
            })
            .when(manual_sort_mode, |this| {
                let visible_connection_ids = visible_connection_ids.clone();
                this.on_drag_move(cx.listener(
                    move |this, drag: &DragMoveEvent<DragConnection>, _, cx| {
                        if !drag.bounds.contains(&drag.event.position) {
                            return;
                        }

                        let drag_connection = drag.drag(cx);
                        let is_same_workspace = drag_connection.workspace_id == workspace_id;
                        let is_cross_workspace = can_drop_connection_on_workspace(
                            drag_connection.workspace_id,
                            workspace_id,
                        );
                        if !is_same_workspace && !is_cross_workspace {
                            return;
                        }

                        if let Some(preview) = this.preview_for_connection_card_gap(
                            &visible_connection_ids,
                            workspace_id,
                            drag_connection.connection_id,
                            drag.event.position,
                        ) {
                            this.update_connection_drop_preview(
                                preview.workspace_id,
                                preview.target_connection_id,
                                preview.position,
                                preview.edge,
                                cx,
                            );
                        } else if is_cross_workspace {
                            this.update_connection_workspace_drop_target(
                                workspace_id.expect("工作区 ID 应存在"),
                                cx,
                            );
                        }
                    },
                ))
                .on_drop(cx.listener(
                    move |this, drag: &DragConnection, _, cx| {
                        if can_drop_connection_on_workspace(drag.workspace_id, workspace_id) {
                            let preview = this
                                .connection_drop_preview
                                .filter(|preview| preview.workspace_id == workspace_id);
                            cx.stop_propagation();
                            if let Some(preview) = preview {
                                this.move_connection_to_workspace_at(
                                    drag.connection_id,
                                    workspace_id.expect("工作区 ID 应存在"),
                                    preview.target_connection_id,
                                    preview.position,
                                    cx,
                                );
                            } else {
                                this.move_connection_to_workspace_end(
                                    drag.connection_id,
                                    workspace_id.expect("工作区 ID 应存在"),
                                    cx,
                                );
                            }
                            return;
                        }
                        if drag.workspace_id != workspace_id {
                            return;
                        }
                        cx.stop_propagation();

                        let Some(preview) = this
                            .connection_drop_preview
                            .filter(|preview| preview.workspace_id == workspace_id)
                        else {
                            return;
                        };

                        this.reorder_connections_manually_at(
                            workspace_id,
                            drag.connection_id,
                            preview.target_connection_id,
                            preview.position,
                            cx,
                        );
                    },
                ))
            });

        for conn in connections {
            let should_render_placeholder = should_render_dragging_connection_placeholder(
                manual_sort_mode,
                cx.has_active_drag(),
                self.dragging_connection_id,
                conn.id,
            );
            container = container.child(div().w(px(240.0)).flex_shrink_0().child(
                if should_render_placeholder {
                    div().w_full().h(px(60.0)).rounded_lg().into_any_element()
                } else {
                    self.render_connection_card(conn, workspace_id, selected_id, cx)
                },
            ));
        }

        if let Some(overlay_preview_bounds) = overlay_preview_bounds {
            container = container
                .child(self.render_connection_card_overlay_indicator(overlay_preview_bounds, cx));
        }

        if manual_sort_mode && cx.has_active_drag() {
            if let Some(last_connection_id) = last_connection_id {
                let zone_active = self.connection_drop_preview
                    == Some(ConnectionDropPreview {
                        workspace_id,
                        target_connection_id: last_connection_id,
                        position: ManualInsertPosition::After,
                        edge: ManualDropIndicatorEdge::Right,
                    })
                    && cx.has_active_drag();
                container = container.child(
                    div().w(px(240.0)).flex_shrink_0().child(
                        div()
                            .w_full()
                            .h(px(60.0))
                            .rounded_lg()
                            .relative()
                            .overflow_hidden()
                            .on_drag_move(cx.listener(
                                move |this, drag: &DragMoveEvent<DragConnection>, _, cx| {
                                    if !drag.bounds.contains(&drag.event.position) {
                                        return;
                                    }

                                    let drag_connection = drag.drag(cx);
                                    if drag_connection.connection_id == last_connection_id {
                                        this.clear_manual_drop_preview(cx);
                                        return;
                                    }
                                    if !can_drop_connection_on_connection_target(
                                        drag_connection.connection_id,
                                        drag_connection.workspace_id,
                                        last_connection_id,
                                        workspace_id,
                                    ) {
                                        return;
                                    }

                                    this.update_connection_drop_preview(
                                        workspace_id,
                                        last_connection_id,
                                        ManualInsertPosition::After,
                                        ManualDropIndicatorEdge::Right,
                                        cx,
                                    );
                                },
                            ))
                            .drag_over::<DragConnection>(move |this, drag, _, cx| {
                                if !can_drop_connection_on_connection_target(
                                    drag.connection_id,
                                    drag.workspace_id,
                                    last_connection_id,
                                    workspace_id,
                                ) {
                                    this
                                } else {
                                    this.border_1()
                                        .border_color(cx.theme().drag_border)
                                        .bg(cx.theme().drop_target.opacity(0.25))
                                }
                            })
                            .on_drop(cx.listener(move |this, drag: &DragConnection, _, cx| {
                                if !can_drop_connection_on_connection_target(
                                    drag.connection_id,
                                    drag.workspace_id,
                                    last_connection_id,
                                    workspace_id,
                                ) {
                                    return;
                                }
                                cx.stop_propagation();
                                if drag.workspace_id == workspace_id {
                                    this.reorder_connections_to_end_manually(
                                        workspace_id,
                                        drag.connection_id,
                                        last_connection_id,
                                        cx,
                                    );
                                } else if let Some(workspace_id) = workspace_id {
                                    this.move_connection_to_workspace_at(
                                        drag.connection_id,
                                        workspace_id,
                                        last_connection_id,
                                        ManualInsertPosition::After,
                                        cx,
                                    );
                                }
                            }))
                            .child(
                                div()
                                    .w_full()
                                    .h_full()
                                    .rounded_lg()
                                    .border_2()
                                    .border_dashed()
                                    .border_color(cx.theme().drag_border.opacity(if zone_active {
                                        1.0
                                    } else {
                                        0.45
                                    }))
                                    .bg(cx.theme().drop_target.opacity(if zone_active {
                                        0.24
                                    } else {
                                        0.12
                                    }))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(div().w(px(56.0)).h(px(6.0)).rounded_full().bg(
                                        cx.theme().drag_border.opacity(if zone_active {
                                            0.85
                                        } else {
                                            0.4
                                        }),
                                    )),
                            ),
                    ),
                );
            }
        }
        container
    }

    fn render_unassigned_section(
        &self,
        connections: Vec<StoredConnection>,
        selected_id: Option<i64>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().foreground)
                            .child(
                                t!("Home.unassigned_workspace")
                                    .to_string()
                                    .into_any_element(),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                t!("Home.connection_count", count = connections.len()).to_string(),
                            ),
                    ),
            )
            .child(self.render_connections_collection(connections, None, selected_id, cx))
    }

    fn render_connection_card(
        &self,
        conn: StoredConnection,
        workspace_id: Option<i64>,
        selected_id: Option<i64>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let blur_enabled = cx.theme().window_blur_enabled;
        let window_opacity = cx.theme().backdrop_opacity;
        let card_bg = layered_level_surface_color(
            cx.theme().list,
            blur_enabled,
            window_opacity,
            4,
            WindowsSurfaceLayer::ContentCard,
        );
        let card_overlay_bg = layered_level_surface_color(
            cx.theme().list,
            blur_enabled,
            window_opacity,
            5,
            WindowsSurfaceLayer::ContentSection,
        );
        let conn_id = conn.id;
        let clone_conn = conn.clone();
        let sftp_hover_conn = conn.clone();
        let edit_conn = conn.clone();
        let edit_conn_type = conn.connection_type;
        let edit_conn_name = conn.name.clone();
        let duplicate_conn = conn.clone();
        let delete_conn_id = conn.id;
        let delete_conn_name = conn.name.clone();
        let is_selected = selected_id == conn.id;
        let subtitle = self.connection_subtitle(&conn);
        let workspace =
            workspace_id.and_then(|id| self.workspaces.iter().find(|w| w.id == Some(id)).cloned());

        let is_active = conn
            .id
            .map_or(false, |id| cx.global::<ActiveConnections>().is_active(id));

        let manual_sort_mode = Self::is_manual_sort_mode(cx);
        let view = cx.entity().clone();
        let drag_connection_id = conn.id;
        let drag_workspace_id = workspace_id;
        let drag_connection_name: SharedString = conn.name.clone().into();
        let card_preview_size = drag_connection_id.and_then(|connection_id| {
            self.connection_card_drag_preview_sizes
                .get(&connection_id)
                .copied()
        });
        let connection_drop_indicator = drag_connection_id
            .and_then(|connection_id| {
                self.connection_drop_preview
                    .filter(|preview| {
                        preview.target_connection_id == connection_id
                            && preview.workspace_id == drag_workspace_id
                    })
                    .map(|preview| preview.edge)
            })
            .filter(|_| {
                manual_sort_mode
                    && cx.has_active_drag()
                    && Self::connection_list_view_mode(cx) != ConnectionListViewMode::Card
            });
        let group_name: SharedString = format!("conn-card-group-{}", conn.id.unwrap_or(0)).into();
        let card = v_flex()
            .justify_center()
            .id(SharedString::from(format!(
                "conn-card-{}",
                conn.id.unwrap_or(0)
            )))
            .w_full()
            .h(px(60.))
            .rounded(Radius::Lg.px())
            .bg(card_bg)
            .p_2()
            .border_1()
            .rounded_lg()
            .relative()
            .overflow_hidden()
            .shadow_sm()
            .group(group_name.clone())
            .when(is_selected, |this| {
                this.border_color(cx.theme().list_active_border)
                    .shadow_lg()
                    .border_l_3()
            })
            .when(!is_selected, |this| this.border_color(cx.theme().border))
            .cursor_pointer()
            .when(manual_sort_mode && drag_connection_id.is_some(), |this| {
                this.cursor_grab()
            })
            .hover(|style| {
                style
                    .shadow_lg()
                    .border_color(cx.theme().list_active_border)
            })
            .when(manual_sort_mode && drag_connection_id.is_some(), |this| {
                let connection_id = drag_connection_id.expect("连接 ID 应存在");
                let connection_name = drag_connection_name.clone();
                let view = view.clone();
                this.on_drag(
                    DragConnection {
                        connection_id,
                        workspace_id: drag_workspace_id,
                        name: connection_name,
                        preview_size: card_preview_size,
                    },
                    move |drag, _, _, cx| {
                        _ = view.update(cx, |this, cx| {
                            this.set_dragging_connection_id(connection_id, cx);
                        });
                        cx.stop_propagation();
                        cx.new(|_| drag.clone())
                    },
                )
                .on_drag_move(cx.listener(
                    move |this, drag: &DragMoveEvent<DragConnection>, _, cx| {
                        if !drag.bounds.contains(&drag.event.position) {
                            return;
                        }

                        let drag_connection = drag.drag(cx);
                        if drag_connection.connection_id == connection_id {
                            this.clear_manual_drop_preview(cx);
                            return;
                        }
                        if !can_drop_connection_on_connection_target(
                            drag_connection.connection_id,
                            drag_connection.workspace_id,
                            connection_id,
                            drag_workspace_id,
                        ) {
                            return;
                        }

                        let (position, edge) = card_insert_preview_from_drag(drag);
                        this.update_connection_drop_preview(
                            drag_workspace_id,
                            connection_id,
                            position,
                            edge,
                            cx,
                        );
                    },
                ))
                .drag_over::<DragConnection>(move |this, drag, _, _cx| {
                    if !can_drop_connection_on_connection_target(
                        drag.connection_id,
                        drag.workspace_id,
                        connection_id,
                        drag_workspace_id,
                    ) {
                        this
                    } else {
                        this
                    }
                })
                .on_drop(cx.listener(move |this, drag: &DragConnection, _, cx| {
                    if !can_drop_connection_on_connection_target(
                        drag.connection_id,
                        drag.workspace_id,
                        connection_id,
                        drag_workspace_id,
                    ) {
                        return;
                    }
                    cx.stop_propagation();
                    let position = this
                        .connection_drop_preview
                        .filter(|preview| {
                            preview.target_connection_id == connection_id
                                && preview.workspace_id == drag_workspace_id
                        })
                        .map(|preview| preview.position)
                        .unwrap_or(ManualInsertPosition::After);
                    if drag.workspace_id == drag_workspace_id {
                        this.reorder_connections_manually_at(
                            drag_workspace_id,
                            drag.connection_id,
                            connection_id,
                            position,
                            cx,
                        );
                    } else if let Some(workspace_id) = drag_workspace_id {
                        this.move_connection_to_workspace_at(
                            drag.connection_id,
                            workspace_id,
                            connection_id,
                            position,
                            cx,
                        );
                    }
                }))
            })
            .when_some(drag_connection_id, |this, connection_id| {
                let view = view.clone();
                this.on_prepaint(move |bounds, _, cx| {
                    let size = DragPreviewSize {
                        width: f32::from(bounds.size.width),
                        height: f32::from(bounds.size.height),
                    };
                    view.update(cx, |this, _| {
                        this.update_connection_card_drag_preview_size(connection_id, size);
                        this.update_connection_card_bounds(connection_id, bounds);
                    });
                })
            })
            .when_some(connection_drop_indicator, |this, edge| {
                this.child(Self::render_manual_drop_indicator_for_edge(edge, cx))
            })
            .on_double_click(cx.listener(move |this, _, w, cx| {
                let strategy =
                    build_connection_open_strategy(clone_conn.clone(), workspace.clone());
                strategy.open(this, w, cx);
                cx.notify()
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected_connection_id = conn_id;
                cx.notify();
            }))
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .w_full()
                    .child(
                        div()
                            .h(px(48.0))
                            .rounded(Radius::Lg.px())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(self.render_connection_icon(&conn, 40.0)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .overflow_hidden()
                            .child({
                                let name_tooltip: SharedString = conn.name.clone().into();
                                h_flex().gap_1().overflow_hidden().child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "conn-name-{}",
                                            conn.id.unwrap_or(0)
                                        )))
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(cx.theme().foreground)
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .flex_shrink()
                                        .min_w_0()
                                        .tooltip(move |window, cx| {
                                            Tooltip::new(name_tooltip.clone()).build(window, cx)
                                        })
                                        .child(conn.name.clone()),
                                )
                            })
                            .when_some(subtitle, |this, subtitle| {
                                let tooltip_text: SharedString = subtitle.clone().into();
                                this.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "conn-info-{}",
                                            conn.id.unwrap_or(0)
                                        )))
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .max_w_full()
                                        .tooltip(move |window, cx| {
                                            Tooltip::new(tooltip_text.clone()).build(window, cx)
                                        })
                                        .child(subtitle),
                                )
                            }),
                    ),
            )
            .child(
                // hover时显示的编辑和删除按钮（放在最后渲染，确保层级高于文字内容）
                h_flex()
                    .absolute()
                    .top_0()
                    .right_0()
                    // .gap_1()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .group_hover(group_name.clone(), |style| style.opacity(1.0))
                    .bg(card_overlay_bg)
                    .rounded(Radius::Lg.px())
                    .border_1()
                    .border_color(cx.theme().border.opacity(0.8))
                    .shadow_sm()
                    .cursor_pointer()
                    .opacity(0.0)
                    .when(conn.connection_type == ConnectionType::SshSftp, |this| {
                        this.child(
                            Button::new(SharedString::from(format!(
                                "sftp-conn-{}",
                                conn.id.unwrap_or(0)
                            )))
                            .icon(IconName::Folder1.color())
                            .with_size(Size::Small)
                            // .primary()
                            .cursor_pointer()
                            .tooltip(t!("Home.open_sftp"))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.open_sftp_view(sftp_hover_conn.clone(), window, cx);
                                },
                            )),
                        )
                    })
                    .child(
                        Button::new(SharedString::from(format!(
                            "edit-conn-{}",
                            conn.id.unwrap_or(0)
                        )))
                        .icon(IconName::Edit)
                        .with_size(Size::Small)
                        // .primary()
                        .cursor_pointer()
                        .tooltip(t!("Home.edit_connection"))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                if let Some(conn_id) = edit_conn.id {
                                    let conn_name = edit_conn_name.clone();
                                    match edit_conn_type {
                                        ConnectionType::SshSftp => {
                                            this.editing_connection_id = Some(conn_id);
                                            this.show_ssh_form(window, cx);
                                        }
                                        ConnectionType::Database => {
                                            let db_type = edit_conn
                                                .to_db_connection()
                                                .ok()
                                                .map(|p| p.database_type);
                                            this.confirm_edit_connection(
                                                conn_id, conn_name, db_type, window, cx,
                                            );
                                        }
                                        ConnectionType::Redis => {
                                            this.editing_connection_id = Some(conn_id);
                                            this.show_redis_form(window, cx);
                                        }
                                        ConnectionType::MongoDB => {
                                            this.editing_connection_id = Some(conn_id);
                                            this.show_mongodb_form(window, cx);
                                        }
                                        ConnectionType::Serial => {
                                            this.editing_connection_id = Some(conn_id);
                                            this.show_serial_form(window, cx);
                                        }
                                        _ => {}
                                    }
                                }
                            },
                        )),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "duplicate-conn-{}",
                            conn.id.unwrap_or(0)
                        )))
                        .icon(IconName::Copy)
                        .with_size(Size::Small)
                        // .primary()
                        .cursor_pointer()
                        .tooltip(t!("Home.duplicate_connection"))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.duplicate_connection_and_open_editor(
                                    &duplicate_conn,
                                    window,
                                    cx,
                                );
                            },
                        )),
                    )
                    .child(
                        Button::new(SharedString::from(format!(
                            "delete-conn-{}",
                            conn.id.unwrap_or(0)
                        )))
                        .icon(IconName::Remove)
                        .with_size(Size::Small)
                        .danger()
                        .cursor_pointer()
                        .tooltip(t!("Home.delete_connection"))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                cx.stop_propagation();
                                if let Some(conn_id) = delete_conn_id {
                                    let conn_name = delete_conn_name.clone();
                                    this.confirm_delete_connection(conn_id, conn_name, window, cx);
                                }
                            },
                        )),
                    ),
            )
            .when(is_active, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(px(2.0))
                        .left(px(2.0))
                        .w(px(16.0))
                        .h(px(16.0))
                        .rounded_full()
                        .bg(cx.theme().success)
                        .shadow_lg()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Icon::new(IconName::Check)
                                .with_size(px(14.0))
                                .text_color(gpui::white()),
                        ),
                )
            });

        card.into_any_element()
    }
}

fn compare_workspaces(
    a: &Workspace,
    b: &Workspace,
    sort_field: ConnectionListSortField,
    sort_order: ConnectionListSortOrder,
) -> Ordering {
    match (a.id, b.id) {
        (None, Some(_)) => return Ordering::Greater,
        (Some(_), None) => return Ordering::Less,
        _ => {}
    }
    let cmp = match sort_field {
        ConnectionListSortField::Manual => manual_sort_value(a.sort_order)
            .cmp(&manual_sort_value(b.sort_order))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| timestamp_value(a.updated_at).cmp(&timestamp_value(b.updated_at)))
            .then_with(|| timestamp_value(a.created_at).cmp(&timestamp_value(b.created_at)))
            .then_with(|| a.id.unwrap_or(0).cmp(&b.id.unwrap_or(0))),
        ConnectionListSortField::Name => a
            .name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| timestamp_value(a.updated_at).cmp(&timestamp_value(b.updated_at)))
            .then_with(|| timestamp_value(a.created_at).cmp(&timestamp_value(b.created_at)))
            .then_with(|| a.id.unwrap_or(0).cmp(&b.id.unwrap_or(0))),
        ConnectionListSortField::CreatedAt => timestamp_value(a.created_at)
            .cmp(&timestamp_value(b.created_at))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| timestamp_value(a.updated_at).cmp(&timestamp_value(b.updated_at)))
            .then_with(|| a.id.unwrap_or(0).cmp(&b.id.unwrap_or(0))),
        ConnectionListSortField::UpdatedAt => timestamp_value(a.updated_at)
            .cmp(&timestamp_value(b.updated_at))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| timestamp_value(a.created_at).cmp(&timestamp_value(b.created_at)))
            .then_with(|| a.id.unwrap_or(0).cmp(&b.id.unwrap_or(0))),
    };
    if sort_field == ConnectionListSortField::Manual {
        return cmp;
    }
    match sort_order {
        ConnectionListSortOrder::Ascending => cmp,
        ConnectionListSortOrder::Descending => cmp.reverse(),
    }
}

fn compare_connections(
    a: &StoredConnection,
    b: &StoredConnection,
    sort_field: ConnectionListSortField,
    sort_order: ConnectionListSortOrder,
) -> Ordering {
    let cmp = match sort_field {
        ConnectionListSortField::Manual => manual_sort_value(a.sort_order)
            .cmp(&manual_sort_value(b.sort_order))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| timestamp_value(a.updated_at).cmp(&timestamp_value(b.updated_at)))
            .then_with(|| timestamp_value(a.created_at).cmp(&timestamp_value(b.created_at)))
            .then_with(|| a.id.unwrap_or(0).cmp(&b.id.unwrap_or(0))),
        ConnectionListSortField::Name => a
            .name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| timestamp_value(a.updated_at).cmp(&timestamp_value(b.updated_at)))
            .then_with(|| timestamp_value(a.created_at).cmp(&timestamp_value(b.created_at)))
            .then_with(|| a.id.unwrap_or(0).cmp(&b.id.unwrap_or(0))),
        ConnectionListSortField::CreatedAt => timestamp_value(a.created_at)
            .cmp(&timestamp_value(b.created_at))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| timestamp_value(a.updated_at).cmp(&timestamp_value(b.updated_at)))
            .then_with(|| a.id.unwrap_or(0).cmp(&b.id.unwrap_or(0))),
        ConnectionListSortField::UpdatedAt => timestamp_value(a.updated_at)
            .cmp(&timestamp_value(b.updated_at))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| timestamp_value(a.created_at).cmp(&timestamp_value(b.created_at)))
            .then_with(|| a.id.unwrap_or(0).cmp(&b.id.unwrap_or(0))),
    };
    if sort_field == ConnectionListSortField::Manual {
        return cmp;
    }

    match sort_order {
        ConnectionListSortOrder::Ascending => cmp,
        ConnectionListSortOrder::Descending => cmp.reverse(),
    }
}

fn timestamp_value(value: Option<i64>) -> i64 {
    value.unwrap_or(0)
}

fn manual_sort_value(value: Option<i64>) -> i64 {
    value.unwrap_or(i64::MAX)
}

fn ordered_connection_ids_for_workspace(
    connections: &[StoredConnection],
    workspace_id: Option<i64>,
) -> Vec<i64> {
    let mut ordered_connection_ids: Vec<i64> = connections
        .iter()
        .filter(|connection| connection.workspace_id == workspace_id)
        .filter_map(|connection| connection.id)
        .collect();
    ordered_connection_ids.sort_by(|a, b| {
        let left = connections
            .iter()
            .find(|connection| connection.id == Some(*a))
            .expect("连接 ID 已存在于当前列表");
        let right = connections
            .iter()
            .find(|connection| connection.id == Some(*b))
            .expect("连接 ID 已存在于当前列表");
        compare_connections(
            left,
            right,
            ConnectionListSortField::Manual,
            ConnectionListSortOrder::Ascending,
        )
    });
    ordered_connection_ids
}

fn can_drop_connection_on_workspace(
    drag_workspace_id: Option<i64>,
    target_workspace_id: Option<i64>,
) -> bool {
    target_workspace_id.is_some() && drag_workspace_id != target_workspace_id
}

fn can_drop_connection_on_connection_target(
    drag_connection_id: i64,
    drag_workspace_id: Option<i64>,
    target_connection_id: i64,
    target_workspace_id: Option<i64>,
) -> bool {
    drag_connection_id != target_connection_id
        && (drag_workspace_id == target_workspace_id
            || can_drop_connection_on_workspace(drag_workspace_id, target_workspace_id))
}

fn plan_connection_move_to_workspace_position(
    connections: &[StoredConnection],
    connection_id: i64,
    target_workspace_id: Option<i64>,
    target_connection_id: i64,
    position: ManualInsertPosition,
) -> Option<ConnectionWorkspaceMovePlan> {
    let dragged_connection = connections
        .iter()
        .find(|connection| connection.id == Some(connection_id))?;
    let source_workspace_id = dragged_connection.workspace_id;
    if source_workspace_id == target_workspace_id {
        return None;
    }

    let mut source_connection_ids =
        ordered_connection_ids_for_workspace(connections, source_workspace_id);
    let source_index = source_connection_ids
        .iter()
        .position(|candidate_id| *candidate_id == connection_id)?;
    source_connection_ids.remove(source_index);

    let mut target_connection_ids =
        ordered_connection_ids_for_workspace(connections, target_workspace_id);
    let target_index = target_connection_ids
        .iter()
        .position(|candidate_id| *candidate_id == target_connection_id)?;
    let insert_index = match position {
        ManualInsertPosition::Before => target_index,
        ManualInsertPosition::After => target_index + 1,
    };
    target_connection_ids.insert(insert_index, connection_id);

    Some(ConnectionWorkspaceMovePlan {
        source_workspace_id,
        target_workspace_id,
        source_connection_ids,
        target_connection_ids,
    })
}

fn plan_connection_move_to_workspace_end(
    connections: &[StoredConnection],
    connection_id: i64,
    target_workspace_id: Option<i64>,
) -> Option<ConnectionWorkspaceMovePlan> {
    let dragged_connection = connections
        .iter()
        .find(|connection| connection.id == Some(connection_id))?;
    let source_workspace_id = dragged_connection.workspace_id;
    if source_workspace_id == target_workspace_id {
        return None;
    }

    let mut source_connection_ids =
        ordered_connection_ids_for_workspace(connections, source_workspace_id);
    let source_index = source_connection_ids
        .iter()
        .position(|candidate_id| *candidate_id == connection_id)?;
    source_connection_ids.remove(source_index);

    let mut target_connection_ids =
        ordered_connection_ids_for_workspace(connections, target_workspace_id);
    target_connection_ids.push(connection_id);

    Some(ConnectionWorkspaceMovePlan {
        source_workspace_id,
        target_workspace_id,
        source_connection_ids,
        target_connection_ids,
    })
}

fn should_render_connection_grid_tail_slot(
    manual_sort_mode: bool,
    has_active_drag: bool,
    preview_workspace_id: Option<Option<i64>>,
    workspace_id: Option<i64>,
) -> bool {
    manual_sort_mode && has_active_drag && preview_workspace_id == Some(workspace_id)
}

fn should_render_dragging_connection_placeholder(
    manual_sort_mode: bool,
    has_active_drag: bool,
    dragging_connection_id: Option<i64>,
    connection_id: Option<i64>,
) -> bool {
    manual_sort_mode
        && has_active_drag
        && dragging_connection_id.is_some()
        && dragging_connection_id == connection_id
}

fn move_item_relative_to_target<T>(
    items: &mut Vec<T>,
    source_index: usize,
    target_index: usize,
    position: ManualInsertPosition,
) {
    if source_index == target_index || source_index >= items.len() || target_index >= items.len() {
        return;
    }

    let item = items.remove(source_index);
    let mut insert_index = match position {
        ManualInsertPosition::Before => target_index,
        ManualInsertPosition::After => target_index + 1,
    };

    if source_index < target_index {
        insert_index = insert_index.saturating_sub(1);
    }

    items.insert(insert_index.min(items.len()), item);
}

fn insert_position_from_drag<T>(
    drag: &DragMoveEvent<T>,
    current_position: Option<ManualInsertPosition>,
) -> ManualInsertPosition {
    let bounds = drag.bounds;
    let position = drag.event.position;
    let upper_bound = bounds.top() + bounds.size.height * 0.45;
    let lower_bound = bounds.top() + bounds.size.height * 0.55;

    if position.y <= upper_bound {
        ManualInsertPosition::Before
    } else if position.y >= lower_bound {
        ManualInsertPosition::After
    } else {
        current_position.unwrap_or_else(|| {
            if position.y < bounds.top() + bounds.size.height * 0.5 {
                ManualInsertPosition::Before
            } else {
                ManualInsertPosition::After
            }
        })
    }
}

fn card_insert_preview_from_drag<T>(
    drag: &DragMoveEvent<T>,
) -> (ManualInsertPosition, ManualDropIndicatorEdge) {
    let bounds = drag.bounds;
    let position = drag.event.position;
    let width = f32::from(bounds.size.width).max(1.0);
    let height = f32::from(bounds.size.height).max(1.0);
    let relative_x = f32::from(position.x - bounds.left()) / width;
    let relative_y = f32::from(position.y - bounds.top()) / height;
    let horizontal_distance = (relative_x - 0.5).abs();
    let vertical_distance = (relative_y - 0.5).abs();

    if horizontal_distance >= vertical_distance {
        if relative_x < 0.5 {
            (ManualInsertPosition::Before, ManualDropIndicatorEdge::Left)
        } else {
            (ManualInsertPosition::After, ManualDropIndicatorEdge::Right)
        }
    } else if relative_y < 0.5 {
        (ManualInsertPosition::Before, ManualDropIndicatorEdge::Top)
    } else {
        (ManualInsertPosition::After, ManualDropIndicatorEdge::Bottom)
    }
}

fn card_gap_preview_from_bounds(
    bounds: Bounds<Pixels>,
    position: Point<Pixels>,
) -> (ManualInsertPosition, ManualDropIndicatorEdge, f32) {
    let left = f32::from(bounds.left());
    let right = f32::from(bounds.right());
    let top = f32::from(bounds.top());
    let bottom = f32::from(bounds.bottom());
    let x = f32::from(position.x);
    let y = f32::from(position.y);

    let horizontal_overlap = x >= left && x <= right;
    let vertical_overlap = y >= top && y <= bottom;

    if vertical_overlap {
        if x < left {
            return (
                ManualInsertPosition::Before,
                ManualDropIndicatorEdge::Left,
                left - x,
            );
        }
        if x > right {
            return (
                ManualInsertPosition::After,
                ManualDropIndicatorEdge::Right,
                x - right,
            );
        }
    }

    if horizontal_overlap {
        if y < top {
            return (
                ManualInsertPosition::Before,
                ManualDropIndicatorEdge::Top,
                top - y,
            );
        }
        if y > bottom {
            return (
                ManualInsertPosition::After,
                ManualDropIndicatorEdge::Bottom,
                y - bottom,
            );
        }
    }

    let left_distance = (x - left).abs();
    let right_distance = (x - right).abs();
    let top_distance = (y - top).abs();
    let bottom_distance = (y - bottom).abs();

    let mut best = (
        ManualInsertPosition::Before,
        ManualDropIndicatorEdge::Left,
        left_distance,
    );
    for candidate in [
        (
            ManualInsertPosition::After,
            ManualDropIndicatorEdge::Right,
            right_distance,
        ),
        (
            ManualInsertPosition::Before,
            ManualDropIndicatorEdge::Top,
            top_distance,
        ),
        (
            ManualInsertPosition::After,
            ManualDropIndicatorEdge::Bottom,
            bottom_distance,
        ),
    ] {
        if candidate.2 < best.2 {
            best = candidate;
        }
    }

    best
}

fn preview_for_connection_card_gap_from_bounds(
    workspace_id: Option<i64>,
    position: Point<Pixels>,
    card_bounds: &[(i64, Bounds<Pixels>)],
) -> Option<ConnectionDropPreview> {
    let mut best_preview: Option<(f32, ConnectionDropPreview)> = None;

    for &(connection_id, bounds) in card_bounds {
        if bounds.contains(&position) {
            return None;
        }

        let (position_kind, edge, distance) = card_gap_preview_from_bounds(bounds, position);
        let preview = ConnectionDropPreview {
            workspace_id,
            target_connection_id: connection_id,
            position: position_kind,
            edge,
        };

        match best_preview {
            Some((best_distance, _)) if distance >= best_distance => {}
            _ => best_preview = Some((distance, preview)),
        }
    }

    best_preview.map(|(_, preview)| preview)
}

fn relative_bounds_in_grid(bounds: Bounds<Pixels>, grid_bounds: Bounds<Pixels>) -> Bounds<Pixels> {
    Bounds::new(
        Point::new(
            bounds.origin.x - grid_bounds.origin.x,
            bounds.origin.y - grid_bounds.origin.y,
        ),
        bounds.size,
    )
}

fn connection_card_overlay_preview_bounds_from_bounds(
    preview: ConnectionDropPreview,
    visible_connection_ids: &[i64],
    grid_bounds: Bounds<Pixels>,
    card_bounds: &[(i64, Bounds<Pixels>)],
) -> Option<Bounds<Pixels>> {
    let find_bounds = |connection_id| {
        card_bounds
            .iter()
            .find(|(candidate_id, _)| *candidate_id == connection_id)
            .map(|(_, bounds)| *bounds)
    };

    if let Some(anchor_index) = connection_card_slot_anchor_index(preview, visible_connection_ids) {
        let anchor_bounds = find_bounds(visible_connection_ids[anchor_index])?;
        let relative_anchor_bounds = relative_bounds_in_grid(anchor_bounds, grid_bounds);
        let relative_partner_bounds = anchor_index
            .checked_sub(1)
            .and_then(|index| find_bounds(visible_connection_ids[index]))
            .map(|bounds| relative_bounds_in_grid(bounds, grid_bounds));

        Some(connection_card_slot_indicator_bounds(
            relative_anchor_bounds,
            relative_partner_bounds,
        ))
    } else {
        None
    }
}

fn connection_card_slot_anchor_index(
    preview: ConnectionDropPreview,
    visible_connection_ids: &[i64],
) -> Option<usize> {
    let target_index = visible_connection_ids
        .iter()
        .position(|connection_id| *connection_id == preview.target_connection_id)?;

    match preview.position {
        ManualInsertPosition::Before => Some(target_index),
        ManualInsertPosition::After => {
            let next_index = target_index + 1;
            (next_index < visible_connection_ids.len()).then_some(next_index)
        }
    }
}

fn connection_card_slot_indicator_bounds(
    anchor_bounds: Bounds<Pixels>,
    partner_bounds: Option<Bounds<Pixels>>,
) -> Bounds<Pixels> {
    let thickness = px(12.0);
    let inset = px(8.0);

    if let Some(partner_bounds) = partner_bounds {
        let vertical_overlap_top = anchor_bounds.top().max(partner_bounds.top()) + inset;
        let vertical_overlap_bottom = anchor_bounds.bottom().min(partner_bounds.bottom()) - inset;
        let horizontal_gap = anchor_bounds.left() - partner_bounds.right();

        if horizontal_gap > px(0.0) && vertical_overlap_bottom > vertical_overlap_top {
            let center_x = partner_bounds.right() + horizontal_gap * 0.5;
            return Bounds::new(
                Point::new(center_x - thickness * 0.5, vertical_overlap_top),
                gpui::size(thickness, vertical_overlap_bottom - vertical_overlap_top),
            );
        }

        let vertical_gap = anchor_bounds.top() - partner_bounds.bottom();
        if vertical_gap > px(0.0) {
            let horizontal_overlap_left = anchor_bounds.left().max(partner_bounds.left()) + inset;
            let horizontal_overlap_right =
                anchor_bounds.right().min(partner_bounds.right()) - inset;
            let (origin_x, width) = if horizontal_overlap_right > horizontal_overlap_left {
                (
                    horizontal_overlap_left,
                    horizontal_overlap_right - horizontal_overlap_left,
                )
            } else {
                (
                    anchor_bounds.origin.x + inset,
                    (anchor_bounds.size.width - inset * 2.0).max(px(36.0)),
                )
            };
            let center_y = partner_bounds.bottom() + vertical_gap * 0.5;
            return Bounds::new(
                Point::new(origin_x, center_y - thickness * 0.5),
                gpui::size(width, thickness),
            );
        }
    }

    connection_card_overlay_indicator_bounds(anchor_bounds, ManualDropIndicatorEdge::Left)
}

fn connection_card_overlay_indicator_bounds(
    bounds: Bounds<Pixels>,
    edge: ManualDropIndicatorEdge,
) -> Bounds<Pixels> {
    let thickness = px(12.0);
    let inset = px(8.0);
    let horizontal_width = (bounds.size.width - inset * 2.0).max(px(36.0));
    let vertical_height = (bounds.size.height - inset * 2.0).max(px(36.0));

    match edge {
        ManualDropIndicatorEdge::Left => Bounds::new(
            Point::new(bounds.origin.x - thickness * 0.5, bounds.origin.y + inset),
            gpui::size(thickness, vertical_height),
        ),
        ManualDropIndicatorEdge::Right => Bounds::new(
            Point::new(
                bounds.origin.x + bounds.size.width - thickness * 0.5,
                bounds.origin.y + inset,
            ),
            gpui::size(thickness, vertical_height),
        ),
        ManualDropIndicatorEdge::Top => Bounds::new(
            Point::new(bounds.origin.x + inset, bounds.origin.y - thickness * 0.5),
            gpui::size(horizontal_width, thickness),
        ),
        ManualDropIndicatorEdge::Bottom => Bounds::new(
            Point::new(
                bounds.origin.x + inset,
                bounds.origin.y + bounds.size.height - thickness * 0.5,
            ),
            gpui::size(horizontal_width, thickness),
        ),
    }
}

#[cfg(test)]
mod connection_list_sort_tests {
    use super::*;

    fn make_connection(
        id: i64,
        name: &str,
        created_at: i64,
        updated_at: i64,
        sort_order: i64,
    ) -> StoredConnection {
        StoredConnection {
            id: Some(id),
            name: name.to_string(),
            connection_type: ConnectionType::Database,
            params: String::new(),
            sort_order: Some(sort_order),
            workspace_id: None,
            selected_databases: None,
            remark: None,
            sync_enabled: true,
            cloud_id: None,
            last_synced_at: None,
            created_at: Some(created_at),
            updated_at: Some(updated_at),
            owner_id: None,
        }
    }

    #[test]
    fn compare_connections_sorts_by_name_ascending() {
        let mut items = vec![
            make_connection(1, "zeta", 10, 30, 2),
            make_connection(2, "alpha", 20, 10, 1),
            make_connection(3, "Beta", 30, 20, 0),
        ];

        items.sort_by(|a, b| {
            compare_connections(
                a,
                b,
                ConnectionListSortField::Name,
                ConnectionListSortOrder::Ascending,
            )
        });

        let names: Vec<_> = items.into_iter().map(|item| item.name).collect();
        assert_eq!(names, vec!["alpha", "Beta", "zeta"]);
    }

    #[test]
    fn compare_connections_sorts_by_updated_at_descending() {
        let mut items = vec![
            make_connection(1, "alpha", 10, 100, 2),
            make_connection(2, "beta", 20, 300, 1),
            make_connection(3, "gamma", 30, 200, 0),
        ];

        items.sort_by(|a, b| {
            compare_connections(
                a,
                b,
                ConnectionListSortField::UpdatedAt,
                ConnectionListSortOrder::Descending,
            )
        });

        let ids: Vec<_> = items.into_iter().map(|item| item.id.unwrap()).collect();
        assert_eq!(ids, vec![2, 3, 1]);
    }

    #[test]
    fn compare_connections_sorts_by_created_at_ascending() {
        let mut items = vec![
            make_connection(1, "alpha", 30, 100, 2),
            make_connection(2, "beta", 10, 300, 1),
            make_connection(3, "gamma", 20, 200, 0),
        ];

        items.sort_by(|a, b| {
            compare_connections(
                a,
                b,
                ConnectionListSortField::CreatedAt,
                ConnectionListSortOrder::Ascending,
            )
        });

        let ids: Vec<_> = items.into_iter().map(|item| item.id.unwrap()).collect();
        assert_eq!(ids, vec![2, 3, 1]);
    }

    #[test]
    fn compare_connections_sorts_by_manual_order() {
        let mut items = vec![
            make_connection(1, "alpha", 30, 100, 2),
            make_connection(2, "beta", 10, 300, 0),
            make_connection(3, "gamma", 20, 200, 1),
        ];

        items.sort_by(|a, b| {
            compare_connections(
                a,
                b,
                ConnectionListSortField::Manual,
                ConnectionListSortOrder::Descending,
            )
        });

        let ids: Vec<_> = items.into_iter().map(|item| item.id.unwrap()).collect();
        assert_eq!(ids, vec![2, 3, 1]);
    }

    #[test]
    fn can_drop_connection_on_workspace_rejects_same_workspace_and_empty_target() {
        assert!(!can_drop_connection_on_workspace(Some(7), Some(7)));
        assert!(can_drop_connection_on_workspace(Some(7), Some(9)));
        assert!(can_drop_connection_on_workspace(None, Some(9)));
        assert!(!can_drop_connection_on_workspace(Some(7), None));
    }

    #[test]
    fn plan_connection_move_to_workspace_position_inserts_before_target_and_compacts_source() {
        let mut source_first = make_connection(1, "source-a", 10, 10, 0);
        source_first.workspace_id = Some(7);
        let mut moving = make_connection(2, "moving", 20, 20, 1);
        moving.workspace_id = Some(7);
        let mut source_last = make_connection(3, "source-b", 30, 30, 2);
        source_last.workspace_id = Some(7);
        let mut target_first = make_connection(4, "target-a", 40, 40, 0);
        target_first.workspace_id = Some(9);
        let mut target_last = make_connection(5, "target-b", 50, 50, 1);
        target_last.workspace_id = Some(9);

        let plan = plan_connection_move_to_workspace_position(
            &[source_first, moving, source_last, target_first, target_last],
            2,
            Some(9),
            5,
            ManualInsertPosition::Before,
        )
        .expect("应生成跨工作区插入计划");

        assert_eq!(plan.source_workspace_id, Some(7));
        assert_eq!(plan.target_workspace_id, Some(9));
        assert_eq!(plan.source_connection_ids, vec![1, 3]);
        assert_eq!(plan.target_connection_ids, vec![4, 2, 5]);
    }

    #[test]
    fn plan_connection_move_to_workspace_end_appends_to_target_end() {
        let mut moving = make_connection(2, "moving", 20, 20, 0);
        moving.workspace_id = Some(7);
        let mut target_first = make_connection(4, "target-a", 40, 40, 0);
        target_first.workspace_id = Some(9);
        let mut target_last = make_connection(5, "target-b", 50, 50, 1);
        target_last.workspace_id = Some(9);

        let plan =
            plan_connection_move_to_workspace_end(&[moving, target_first, target_last], 2, Some(9))
                .expect("应生成跨工作区末尾移动计划");

        assert_eq!(plan.source_workspace_id, Some(7));
        assert_eq!(plan.target_workspace_id, Some(9));
        assert!(plan.source_connection_ids.is_empty());
        assert_eq!(plan.target_connection_ids, vec![4, 5, 2]);
    }

    #[test]
    fn should_render_connection_grid_tail_slot_only_when_workspace_preview_matches() {
        assert!(should_render_connection_grid_tail_slot(
            true,
            true,
            Some(Some(7)),
            Some(7)
        ));
        assert!(!should_render_connection_grid_tail_slot(
            true,
            true,
            Some(Some(9)),
            Some(7)
        ));
        assert!(!should_render_connection_grid_tail_slot(
            true,
            false,
            Some(Some(7)),
            Some(7)
        ));
    }

    #[test]
    fn should_render_dragging_connection_placeholder_only_for_active_dragged_item() {
        assert!(should_render_dragging_connection_placeholder(
            true,
            true,
            Some(11),
            Some(11)
        ));
        assert!(!should_render_dragging_connection_placeholder(
            true,
            true,
            Some(11),
            Some(12)
        ));
        assert!(!should_render_dragging_connection_placeholder(
            true,
            false,
            Some(11),
            Some(11)
        ));
    }

    #[test]
    fn move_item_relative_to_target_moves_before_target() {
        let mut items = vec![1, 2, 3, 4];

        move_item_relative_to_target(&mut items, 3, 1, ManualInsertPosition::Before);

        assert_eq!(items, vec![1, 4, 2, 3]);
    }

    #[test]
    fn move_item_relative_to_target_moves_after_target() {
        let mut items = vec![1, 2, 3, 4];

        move_item_relative_to_target(&mut items, 0, 2, ManualInsertPosition::After);

        assert_eq!(items, vec![2, 3, 1, 4]);
    }

    fn make_card_bounds(x: f32, y: f32) -> Bounds<Pixels> {
        Bounds::new(gpui::point(px(x), px(y)), gpui::size(px(100.0), px(80.0)))
    }

    #[test]
    fn preview_for_connection_card_gap_prefers_horizontal_gap() {
        let preview = preview_for_connection_card_gap_from_bounds(
            Some(7),
            gpui::point(px(120.0), px(40.0)),
            &[
                (11, make_card_bounds(0.0, 0.0)),
                (12, make_card_bounds(140.0, 0.0)),
            ],
        );

        assert_eq!(
            preview,
            Some(ConnectionDropPreview {
                workspace_id: Some(7),
                target_connection_id: 11,
                position: ManualInsertPosition::After,
                edge: ManualDropIndicatorEdge::Right,
            })
        );
    }

    #[test]
    fn preview_for_connection_card_gap_prefers_vertical_gap() {
        let preview = preview_for_connection_card_gap_from_bounds(
            Some(7),
            gpui::point(px(50.0), px(90.0)),
            &[
                (11, make_card_bounds(0.0, 0.0)),
                (12, make_card_bounds(0.0, 100.0)),
            ],
        );

        assert_eq!(
            preview,
            Some(ConnectionDropPreview {
                workspace_id: Some(7),
                target_connection_id: 11,
                position: ManualInsertPosition::After,
                edge: ManualDropIndicatorEdge::Bottom,
            })
        );
    }

    #[test]
    fn preview_for_connection_card_gap_returns_none_inside_card() {
        let preview = preview_for_connection_card_gap_from_bounds(
            Some(7),
            gpui::point(px(40.0), px(30.0)),
            &[
                (11, make_card_bounds(0.0, 0.0)),
                (12, make_card_bounds(140.0, 0.0)),
            ],
        );

        assert_eq!(preview, None);
    }

    #[test]
    fn connection_card_slot_anchor_index_maps_after_to_next_slot() {
        let anchor_index = connection_card_slot_anchor_index(
            ConnectionDropPreview {
                workspace_id: Some(7),
                target_connection_id: 22,
                position: ManualInsertPosition::After,
                edge: ManualDropIndicatorEdge::Right,
            },
            &[11, 22, 33],
        );

        assert_eq!(anchor_index, Some(2));
    }

    #[test]
    fn connection_card_slot_indicator_bounds_for_same_row_center_between_cards() {
        let bounds = connection_card_slot_indicator_bounds(
            make_card_bounds(140.0, 30.0),
            Some(make_card_bounds(0.0, 30.0)),
        );

        assert_eq!(bounds.origin.x, px(114.0));
        assert_eq!(bounds.origin.y, px(38.0));
        assert_eq!(bounds.size.width, px(12.0));
        assert_eq!(bounds.size.height, px(64.0));
    }

    #[test]
    fn connection_card_slot_indicator_bounds_for_next_row_centers_between_rows() {
        let bounds = connection_card_slot_indicator_bounds(
            make_card_bounds(0.0, 100.0),
            Some(make_card_bounds(240.0, 0.0)),
        );

        assert_eq!(bounds.origin.x, px(8.0));
        assert_eq!(bounds.origin.y, px(84.0));
        assert_eq!(bounds.size.width, px(84.0));
        assert_eq!(bounds.size.height, px(12.0));
    }
}

impl Focusable for HomePage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<TabContentEvent> for HomePage {}

impl TabContent for HomePage {
    fn content_key(&self) -> &'static str {
        "Home"
    }

    fn title(&self, _cx: &App) -> SharedString {
        SharedString::from(t!("Home.title"))
    }

    fn icon(&self, _cx: &App) -> Option<Icon> {
        Some(IconName::Home.color())
    }

    fn closeable(&self, _cx: &App) -> bool {
        false
    }

    fn width_size(&self, _cx: &App) -> Option<Size> {
        Some(Size::Small)
    }

    fn status_summary(&self, _cx: &App) -> Option<SharedString> {
        let workspace_count = self.workspaces.len();
        let connection_count = self.connections.len();

        let db_count = self
            .connections
            .iter()
            .filter(|c| c.connection_type == ConnectionType::Database)
            .count();
        let ssh_sftp_count = self
            .connections
            .iter()
            .filter(|c| c.connection_type == ConnectionType::SshSftp)
            .count();
        let redis_count = self
            .connections
            .iter()
            .filter(|c| c.connection_type == ConnectionType::Redis)
            .count();
        let mongo_count = self
            .connections
            .iter()
            .filter(|c| c.connection_type == ConnectionType::MongoDB)
            .count();
        let serial_count = self
            .connections
            .iter()
            .filter(|c| c.connection_type == ConnectionType::Serial)
            .count();

        let mut breakdown = Vec::new();
        if db_count > 0 {
            breakdown.push(format!("DB:{}", db_count));
        }
        if ssh_sftp_count > 0 {
            breakdown.push(format!("SSH:{}", ssh_sftp_count));
        }
        if redis_count > 0 {
            breakdown.push(format!("Redis:{}", redis_count));
        }
        if mongo_count > 0 {
            breakdown.push(format!("Mongo:{}", mongo_count));
        }
        if serial_count > 0 {
            breakdown.push(format!("Ser:{}", serial_count));
        }

        let base = format!(
            "{} {}，{} {}{}",
            t!("Workspace.label"),
            workspace_count,
            t!("Home.connection"),
            connection_count,
            if breakdown.is_empty() {
                String::new()
            } else {
                format!("({})", breakdown.join("/"))
            }
        );

        Some(SharedString::from(base))
    }

    fn status_summary_element(&self, cx: &App) -> Option<gpui::AnyElement> {
        let workspace_count = self.workspaces.len();
        let connection_count = self.connections.len();

        let db_count = self
            .connections
            .iter()
            .filter(|c| c.connection_type == ConnectionType::Database)
            .count();
        let ssh_sftp_count = self
            .connections
            .iter()
            .filter(|c| c.connection_type == ConnectionType::SshSftp)
            .count();
        let redis_count = self
            .connections
            .iter()
            .filter(|c| c.connection_type == ConnectionType::Redis)
            .count();
        let mongo_count = self
            .connections
            .iter()
            .filter(|c| c.connection_type == ConnectionType::MongoDB)
            .count();
        let serial_count = self
            .connections
            .iter()
            .filter(|c| c.connection_type == ConnectionType::Serial)
            .count();

        let fg = cx.theme().muted_foreground;
        let fgc = cx.theme().foreground;

        let mut parts: Vec<gpui::AnyElement> = vec![];
        parts.push(
            div()
                .text_sm()
                .text_color(fg)
                .child(format!(
                    "{} {}，{} {}(",
                    t!("Workspace.label"),
                    workspace_count,
                    t!("Home.connection"),
                    connection_count
                ))
                .into_any_element(),
        );

        if db_count > 0 {
            parts.push(
                Icon::new(IconName::Database)
                    .small()
                    .text_color(fg)
                    .into_any_element(),
            );
            parts.push(
                div()
                    .text_sm()
                    .text_color(fgc)
                    .child(db_count.to_string())
                    .into_any_element(),
            );
            if ssh_sftp_count > 0 || redis_count > 0 || mongo_count > 0 || serial_count > 0 {
                parts.push(div().text_sm().text_color(fg).child("/").into_any_element());
            }
        }
        if ssh_sftp_count > 0 {
            parts.push(
                Icon::new(IconName::Terminal)
                    .small()
                    .text_color(fg)
                    .into_any_element(),
            );
            parts.push(
                div()
                    .text_sm()
                    .text_color(fgc)
                    .child(ssh_sftp_count.to_string())
                    .into_any_element(),
            );
            if redis_count > 0 || mongo_count > 0 || serial_count > 0 {
                parts.push(div().text_sm().text_color(fg).child("/").into_any_element());
            }
        }
        if redis_count > 0 {
            parts.push(
                Icon::new(IconName::Redis)
                    .small()
                    .text_color(fg)
                    .into_any_element(),
            );
            parts.push(
                div()
                    .text_sm()
                    .text_color(fgc)
                    .child(redis_count.to_string())
                    .into_any_element(),
            );
            if mongo_count > 0 || serial_count > 0 {
                parts.push(div().text_sm().text_color(fg).child("/").into_any_element());
            }
        }
        if mongo_count > 0 {
            parts.push(
                Icon::new(IconName::MongoDB)
                    .small()
                    .text_color(fg)
                    .into_any_element(),
            );
            parts.push(
                div()
                    .text_sm()
                    .text_color(fgc)
                    .child(mongo_count.to_string())
                    .into_any_element(),
            );
            if serial_count > 0 {
                parts.push(div().text_sm().text_color(fg).child("/").into_any_element());
            }
        }
        if serial_count > 0 {
            parts.push(
                Icon::new(IconName::SerialPort)
                    .small()
                    .text_color(fg)
                    .into_any_element(),
            );
            parts.push(
                div()
                    .text_sm()
                    .text_color(fgc)
                    .child(serial_count.to_string())
                    .into_any_element(),
            );
        }

        parts.push(div().text_sm().text_color(fg).child(")").into_any_element());

        Some(
            h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_1()
                .children(parts)
                .into_any_element(),
        )
    }
}

impl Render for HomePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.dragging_connection_id.is_some() && !cx.has_active_drag() {
            self.dragging_connection_id = None;
        }

        // 检测会话过期：token 刷新失败时由回调设置静态标志，在此处响应
        if crate::auth::check_and_reset_session_expired() {
            self.handle_auth_state_cleared(cx);
            // 延迟弹出登录对话框，避免在 render 中直接修改窗口
            let view = cx.entity();
            window.defer(cx, move |window, cx| {
                view.update(cx, |this, cx| {
                    this.show_login_dialog(window, cx);
                });
            });
        }

        // 检测认证错误：登录/注册失败时显示错误提示
        if let Some(error) = self.auth_error.take() {
            let view = cx.entity();
            window.defer(cx, move |window, cx| {
                let error_msg = error.clone();
                let view_for_ok = view.clone();
                window.open_dialog(cx, move |dialog, _window, _cx| {
                    let view_clone = view_for_ok.clone();
                    dialog
                        .title(t!("Auth.auth_error_title").to_string())
                        .child(error_msg.clone().into_any_element())
                        .alert()
                        .on_ok(move |_, window, cx| {
                            // 延迟到当前错误弹窗关闭后再重新打开登录弹窗，避免关闭掉新弹窗。
                            let view_for_login = view_clone.clone();
                            window.defer(cx, move |window, cx| {
                                _ = view_for_login.update(cx, |this, cx| {
                                    this.show_login_dialog(window, cx);
                                });
                            });
                            true
                        })
                });
            });
        }

        self.maybe_prompt_connection_restore(window, cx);
        let blur_enabled = cx.theme().window_blur_enabled;
        let window_opacity = cx.theme().backdrop_opacity;
        let home_shell_bg = if cfg!(target_os = "windows")
            || cx.theme().window_blur_enabled
            || cx.theme().backdrop_opacity < 1.0
        {
            cx.theme().transparent
        } else {
            layered_level_surface_color(
                cx.theme().background,
                false,
                window_opacity,
                1,
                WindowsSurfaceLayer::ContentBase,
            )
        };
        let home_content_bg = layered_level_surface_color(
            cx.theme().muted,
            blur_enabled,
            window_opacity,
            2,
            WindowsSurfaceLayer::ContentBase,
        );

        div().size_full().track_focus(&self.focus_handle).child(
            h_flex()
                .size_full()
                .child(self.render_sidebar(window, cx))
                .child(
                    v_flex()
                        .flex_1()
                        .h_full()
                        .bg(home_shell_bg)
                        .child(self.render_toolbar(window, cx))
                        .child(
                            div()
                                .flex_1()
                                .w_full()
                                .overflow_hidden()
                                .bg(home_content_bg)
                                .child(self.render_content_area(cx)),
                        ),
                ),
        )
    }
}

/// 使用当前主密钥重新加密并保存所有连接。
fn re_encrypt_all_connections(
    storage: &one_core::storage::StorageManager,
) -> anyhow::Result<usize> {
    let conn_repo = storage
        .get::<ConnectionRepository>()
        .ok_or_else(|| anyhow::anyhow!("ConnectionRepository not found"))?;

    let connections = conn_repo.list()?;
    let mut count = 0;

    for conn in connections {
        conn_repo.update(&conn)?;
        count += 1;
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::{HomePage, SyncFeedbackLevel};
    use one_core::cloud_sync::SyncResult;

    #[test]
    fn summarize_sync_result_returns_info_when_nothing_changed() {
        let result = SyncResult::default();

        let feedback = HomePage::summarize_sync_result(&result);

        assert_eq!(feedback.level, SyncFeedbackLevel::Info);
        assert!(!feedback.message.is_empty());
    }

    #[test]
    fn summarize_sync_result_returns_success_when_changes_applied() {
        let result = SyncResult {
            uploaded: 2,
            downloaded: 1,
            deleted: 0,
            conflicts: Vec::new(),
            errors: Vec::new(),
        };

        let feedback = HomePage::summarize_sync_result(&result);

        assert_eq!(feedback.level, SyncFeedbackLevel::Success);
        assert!(feedback.message.contains("上传 2 项"));
        assert!(feedback.message.contains("下载 1 项"));
    }

    #[test]
    fn summarize_sync_result_returns_warning_when_errors_exist() {
        let result = SyncResult {
            uploaded: 1,
            downloaded: 0,
            deleted: 0,
            conflicts: Vec::new(),
            errors: vec!["请先输入主密钥解锁".to_string()],
        };

        let feedback = HomePage::summarize_sync_result(&result);

        assert_eq!(feedback.level, SyncFeedbackLevel::Warning);
        assert!(!feedback.message.is_empty());
        assert!(feedback.message.contains("请先输入主密钥解锁"));
    }
}

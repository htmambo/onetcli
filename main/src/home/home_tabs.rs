use crate::home_tab::HomePage;
use crate::setting_tab::{AppSettings, DatabaseOpenMode, SettingsPanel};
use db_view::chatdb::chat_panel::ChatPanel;
use db_view::database_tab::DatabaseTabView;
use gpui::AppContext;
use gpui::{App, BorrowAppContext, Context, Entity, Window};
use mongodb_view::MongoTabView;
use one_core::connection_restore::{LocalTerminalRestoreState, SshTerminalRestoreState};
use one_core::storage::{ConnectionType, StoredConnection, Workspace};
use one_core::tab_container::TabItem;
use redis_view::RedisTabView;
use sftp_view::{SftpView, SftpViewEvent};
use terminal::LocalConfig;
use terminal_view::{TerminalConnectionKind, TerminalTheme, TerminalView, TerminalViewEvent};

impl HomePage {
    fn terminal_sync_path_enabled(cx: &App) -> bool {
        if cx.has_global::<AppSettings>() {
            AppSettings::global(cx).terminal_sync_path_with_terminal
        } else {
            false
        }
    }

    fn database_open_mode(cx: &App) -> DatabaseOpenMode {
        if cx.has_global::<AppSettings>() {
            AppSettings::global(cx).database_open_mode
        } else {
            DatabaseOpenMode::default()
        }
    }

    fn find_workspace_for_connection(&self, connection: &StoredConnection) -> Option<Workspace> {
        connection
            .workspace_id
            .and_then(|id| {
                self.workspaces
                    .iter()
                    .find(|workspace| workspace.id == Some(id))
            })
            .cloned()
    }

    fn find_existing_tab_index_for_connection(
        &self,
        connection: &StoredConnection,
        cx: &App,
    ) -> Option<usize> {
        let connection_id = connection.id?;
        let tabs = self.tab_container.read(cx);

        match connection.connection_type {
            ConnectionType::Database => tabs.tabs().iter().enumerate().find_map(|(index, tab)| {
                let view = tab.content().view();
                let database_tab = view.downcast::<DatabaseTabView>().ok()?;
                database_tab
                    .read(cx)
                    .contains_connection_id(connection_id)
                    .then_some(index)
            }),
            ConnectionType::Redis => tabs.tabs().iter().enumerate().find_map(|(index, tab)| {
                let view = tab.content().view();
                let redis_tab = view.downcast::<RedisTabView>().ok()?;
                redis_tab
                    .read(cx)
                    .contains_connection_id(connection_id)
                    .then_some(index)
            }),
            ConnectionType::MongoDB => tabs.tabs().iter().enumerate().find_map(|(index, tab)| {
                let view = tab.content().view();
                let mongo_tab = view.downcast::<MongoTabView>().ok()?;
                mongo_tab
                    .read(cx)
                    .contains_connection_id(connection_id)
                    .then_some(index)
            }),
            ConnectionType::Serial => tabs.tabs().iter().enumerate().find_map(|(index, tab)| {
                let view = tab.content().view();
                let terminal = view.downcast::<TerminalView>().ok()?;
                let terminal = terminal.read(cx);

                (terminal.connection_kind(cx) == TerminalConnectionKind::Serial
                    && terminal.connection_id(cx) == Some(connection_id))
                .then_some(index)
            }),
            ConnectionType::SshSftp => {
                let mut sftp_fallback_index = None;

                for (index, tab) in tabs.tabs().iter().enumerate() {
                    if let Ok(terminal) = tab.content().view().downcast::<TerminalView>() {
                        let terminal = terminal.read(cx);
                        if terminal.connection_kind(cx) == TerminalConnectionKind::Ssh
                            && terminal.connection_id(cx) == Some(connection_id)
                        {
                            return Some(index);
                        }
                    }

                    if let Ok(sftp) = tab.content().view().downcast::<SftpView>() {
                        if sftp.read(cx).connection_id() == Some(connection_id)
                            && sftp_fallback_index.is_none()
                        {
                            sftp_fallback_index = Some(index);
                        }
                    }
                }

                sftp_fallback_index
            }
            _ => None,
        }
    }

    fn activate_existing_tab_for_connection(
        &mut self,
        connection: &StoredConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.find_existing_tab_index_for_connection(connection, cx) else {
            return false;
        };

        self.tab_container.update(cx, |tab_container, cx| {
            tab_container.set_active_index(index, window, cx);
        });
        true
    }

    pub(crate) fn open_connection_from_saved_picker(
        &mut self,
        connection: &StoredConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.activate_existing_tab_for_connection(connection, window, cx) {
            return;
        }

        let workspace = self.find_workspace_for_connection(connection);
        match connection.connection_type {
            ConnectionType::Database => {
                self.add_item_to_tab(connection, workspace, window, cx);
            }
            ConnectionType::Redis => {
                self.open_redis_tab(connection.clone(), workspace, window, cx);
            }
            ConnectionType::MongoDB => {
                self.open_mongodb_tab(connection.clone(), workspace, window, cx);
            }
            ConnectionType::SshSftp => {
                self.open_ssh_terminal(connection.clone(), window, cx);
            }
            ConnectionType::Serial => {
                self.open_serial_terminal(connection.clone(), window, cx);
            }
            _ => {}
        }
    }

    fn register_terminal_view(&mut self, terminal_view: &Entity<TerminalView>) {
        self.terminal_views.retain(|view| view.upgrade().is_some());
        self.terminal_views.push(terminal_view.downgrade());
    }

    /// 注册终端视图：应用当前全局设置 + 绑定事件同步
    ///
    /// 所有创建 TerminalView 的地方都应调用此方法，
    /// 替代之前散落的 register + apply + bind × 2。
    fn setup_terminal_view(
        &mut self,
        terminal_view: &Entity<TerminalView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::info!(
            "setup_terminal_view called, tab_container={:?}",
            self.tab_container.entity_id()
        );
        self.register_terminal_view(terminal_view);
        terminal_view.update(cx, |view, cx| {
            view.set_tab_container(self.tab_container.clone());
            if let Some(wh) = cx.active_window() {
                view.set_window_handle(wh);
            }
        });

        // 从 AppSettings 读取所有终端设置并应用
        if cx.has_global::<AppSettings>() {
            let settings = AppSettings::global(cx);
            let font_size = settings.terminal_font_size as f32;
            let font_family = settings.terminal_font_family.clone();
            let font_ligatures = settings.terminal_font_ligatures;
            let line_height_scale = settings.terminal_line_height_scale as f32;
            let auto_copy = settings.terminal_auto_copy;
            let autocomplete_enabled = settings.terminal_enable_autocomplete;
            let middle_click_paste = settings.terminal_middle_click_paste;
            let sync_path = settings.terminal_sync_path_with_terminal;
            let cursor_blink = settings.terminal_cursor_blink;
            let confirm_multiline = settings.terminal_confirm_multiline_paste;
            let confirm_high_risk = settings.terminal_confirm_high_risk_command;
            let exit_behavior = settings.terminal_exit_behavior.clone();
            let theme = TerminalTheme::find_by_name(&settings.terminal_theme);

            terminal_view.update(cx, |view, cx| {
                view.apply_terminal_settings(
                    font_size,
                    font_family.clone(),
                    font_ligatures,
                    line_height_scale,
                    auto_copy,
                    autocomplete_enabled,
                    middle_click_paste,
                    sync_path,
                    &exit_behavior,
                    window,
                    cx,
                );
                view.apply_cursor_blink(cursor_blink, window, cx);
                view.apply_confirm_multiline_paste(confirm_multiline, cx);
                view.apply_confirm_high_risk_command(confirm_high_risk, cx);
            });
            if let Some(theme) = theme {
                terminal_view.update(cx, |view, cx| {
                    view.apply_theme(&theme, window, cx);
                });
            }
        }

        // 单一订阅处理所有 TerminalViewEvent
        let subscription = cx.subscribe_in(
            terminal_view,
            window,
            |this, _view, event: &TerminalViewEvent, window, cx| {
                match event {
                    // ---- 持久化到 AppSettings 并同步 ----
                    TerminalViewEvent::FontSizeChanged { size } => {
                        cx.update_global::<AppSettings, _>(|s, _| {
                            s.terminal_font_size = *size as f64;
                            s.save();
                        });
                        let settings = AppSettings::global(cx).clone();
                        this.apply_terminal_settings_to_all(&settings, window, cx);
                    }
                    TerminalViewEvent::FontFamilyChanged { family } => {
                        cx.update_global::<AppSettings, _>(|s, _| {
                            s.terminal_font_family = family.clone();
                            s.save();
                        });
                        let settings = AppSettings::global(cx).clone();
                        this.apply_terminal_settings_to_all(&settings, window, cx);
                    }
                    TerminalViewEvent::LineHeightScaleChanged { scale } => {
                        cx.update_global::<AppSettings, _>(|s, _| {
                            s.terminal_line_height_scale = *scale as f64;
                            s.save();
                        });
                        let settings = AppSettings::global(cx).clone();
                        this.apply_terminal_settings_to_all(&settings, window, cx);
                    }
                    TerminalViewEvent::AutoCopyChanged { enabled } => {
                        cx.update_global::<AppSettings, _>(|s, _| {
                            s.terminal_auto_copy = *enabled;
                            s.save();
                        });
                        let settings = AppSettings::global(cx).clone();
                        this.apply_terminal_settings_to_all(&settings, window, cx);
                    }
                    TerminalViewEvent::MiddleClickPasteChanged { enabled } => {
                        cx.update_global::<AppSettings, _>(|s, _| {
                            s.terminal_middle_click_paste = *enabled;
                            s.save();
                        });
                        let settings = AppSettings::global(cx).clone();
                        this.apply_terminal_settings_to_all(&settings, window, cx);
                    }
                    TerminalViewEvent::SyncPathChanged { enabled } => {
                        cx.update_global::<AppSettings, _>(|s, _| {
                            s.terminal_sync_path_with_terminal = *enabled;
                            s.save();
                        });
                        let settings = AppSettings::global(cx).clone();
                        this.apply_terminal_settings_to_all(&settings, window, cx);
                    }

                    // ---- 持久化到 AppSettings 并同步 ----
                    TerminalViewEvent::ThemeChanged { theme } => {
                        cx.update_global::<AppSettings, _>(|s, _| {
                            s.terminal_theme = theme.name.to_string();
                            s.save();
                        });
                        let theme = theme.clone();
                        this.for_each_terminal_view(window, cx, |view, window, cx| {
                            view.apply_theme(&theme, window, cx);
                        });
                    }
                    TerminalViewEvent::CursorBlinkChanged { enabled } => {
                        cx.update_global::<AppSettings, _>(|s, _| {
                            s.terminal_cursor_blink = *enabled;
                            s.save();
                        });
                        let enabled = *enabled;
                        this.for_each_terminal_view(window, cx, |view, window, cx| {
                            view.apply_cursor_blink(enabled, window, cx);
                        });
                    }
                    TerminalViewEvent::ConfirmMultilinePasteChanged { enabled } => {
                        cx.update_global::<AppSettings, _>(|s, _| {
                            s.terminal_confirm_multiline_paste = *enabled;
                            s.save();
                        });
                        let enabled = *enabled;
                        this.for_each_terminal_view(window, cx, |view, _window, cx| {
                            view.apply_confirm_multiline_paste(enabled, cx);
                        });
                    }
                    TerminalViewEvent::ConfirmHighRiskCommandChanged { enabled } => {
                        cx.update_global::<AppSettings, _>(|s, _| {
                            s.terminal_confirm_high_risk_command = *enabled;
                            s.save();
                        });
                        let enabled = *enabled;
                        this.for_each_terminal_view(window, cx, |view, _window, cx| {
                            view.apply_confirm_high_risk_command(enabled, cx);
                        });
                    }
                    TerminalViewEvent::Close => {
                        let view_id = _view.entity_id();
                        let tab_container = this.tab_container.clone();
                        tab_container.update(cx, |container, cx| {
                            if let Some(index) = container
                                .tabs()
                                .iter()
                                .position(|t| t.content().content_id(cx) == view_id)
                            {
                                container.close_tab(index, window, cx).detach();
                            }
                        });
                    }
                }
                cx.notify();
            },
        );
        self._subscriptions.push(subscription);
    }

    fn next_local_terminal_tab_id_and_index(&self, cx: &App) -> (String, Option<usize>) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let tab_id = format!("local-terminal-{}", timestamp);
        let existing_count = self
            .tab_container
            .read(cx)
            .tabs()
            .iter()
            .filter(|tab| {
                tab.id().starts_with("local-terminal-") || tab.id().starts_with("terminal-")
            })
            .count();
        let tab_index = (existing_count > 0).then_some(existing_count + 1);

        (tab_id, tab_index)
    }

    fn open_local_terminal_with_state(
        &mut self,
        from: &'static str,
        config: LocalConfig,
        restore_state: Option<&LocalTerminalRestoreState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (tab_id, tab_index) = self.next_local_terminal_tab_id_and_index(cx);
        let terminal_view = if let Some(restore_state) = restore_state.cloned() {
            cx.new(|cx| {
                TerminalView::new_restored_local_with_index(
                    config,
                    restore_state,
                    tab_index,
                    window,
                    cx,
                )
            })
        } else {
            cx.new(|cx| TerminalView::new_with_index(config, tab_index, window, cx))
        };

        self.setup_terminal_view(&terminal_view, window, cx);
        if let Some(restore_state) = restore_state {
            terminal_view.update(cx, |view, cx| {
                view.apply_local_restore_state(restore_state, window, cx);
            });
        }

        self.tab_container.update(cx, |tc, cx| {
            let tab = TabItem::new(tab_id, from, terminal_view);
            tc.add_and_activate_tab_with_focus(tab, window, cx);
        });
    }

    pub(crate) fn open_local_terminal(
        &mut self,
        working_dir: Option<String>,
        from: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = LocalConfig {
            working_dir,
            ..Default::default()
        };
        self.open_local_terminal_with_state(from, config, None, window, cx);
    }

    pub(crate) fn restore_local_terminal(
        &mut self,
        restore_state: LocalTerminalRestoreState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let working_dir = restore_state
            .working_dir
            .as_deref()
            .filter(|dir| {
                std::path::Path::new(dir).is_absolute()
                    || dir.contains(":\\")
                    || dir.starts_with("\\\\")
            })
            .map(str::to_string);
        let config = LocalConfig {
            working_dir,
            ..Default::default()
        };

        self.open_local_terminal_with_state("terminal", config, Some(&restore_state), window, cx);
    }

    pub(crate) fn apply_terminal_settings_to_all(
        &mut self,
        settings: &AppSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let font_size = settings.terminal_font_size as f32;
        let font_family = settings.terminal_font_family.clone();
        let font_ligatures = settings.terminal_font_ligatures;
        let line_height_scale = settings.terminal_line_height_scale as f32;
        let auto_copy = settings.terminal_auto_copy;
        let autocomplete_enabled = settings.terminal_enable_autocomplete;
        let middle_click_paste = settings.terminal_middle_click_paste;
        let sync_path = settings.terminal_sync_path_with_terminal;
        let exit_behavior = settings.terminal_exit_behavior.clone();
        self.terminal_views.retain(|weak| {
            if let Some(view) = weak.upgrade() {
                view.update(cx, |view, cx| {
                    view.apply_terminal_settings(
                        font_size,
                        font_family.clone(),
                        font_ligatures,
                        line_height_scale,
                        auto_copy,
                        autocomplete_enabled,
                        middle_click_paste,
                        sync_path,
                        &exit_behavior,
                        window,
                        cx,
                    );
                });
                true
            } else {
                false
            }
        });
    }

    pub(crate) fn apply_app_settings(
        &mut self,
        settings: &AppSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_terminal_settings_to_all(settings, window, cx);

        let theme = TerminalTheme::find_by_name(&settings.terminal_theme);
        let cursor_blink = settings.terminal_cursor_blink;
        let confirm_multiline = settings.terminal_confirm_multiline_paste;
        let confirm_high_risk = settings.terminal_confirm_high_risk_command;

        self.for_each_terminal_view(window, cx, |view, window, cx| {
            if let Some(theme) = theme.as_ref() {
                view.apply_theme(theme, window, cx);
            }
            view.apply_cursor_blink(cursor_blink, window, cx);
            view.apply_confirm_multiline_paste(confirm_multiline, cx);
            view.apply_confirm_high_risk_command(confirm_high_risk, cx);
        });
    }

    /// 遍历所有存活的终端视图并执行回调，同时清理已释放的弱引用
    fn for_each_terminal_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        mut f: impl FnMut(&mut TerminalView, &mut Window, &mut Context<TerminalView>),
    ) {
        self.terminal_views.retain(|weak| {
            if let Some(entity) = weak.upgrade() {
                entity.update(cx, |view, cx| {
                    f(view, window, cx);
                });
                true
            } else {
                false
            }
        });
    }

    pub(crate) fn open_ssh_terminal(
        &mut self,
        conn: StoredConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_ssh_terminal_with_state(conn, None, window, cx);
    }

    pub(crate) fn open_ssh_terminal_with_state(
        &mut self,
        conn: StoredConnection,
        restore_state: Option<&SshTerminalRestoreState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        tracing::info!("open_ssh_terminal_with_state called, conn_id={:?}", conn.id);
        let conn_id = conn.id.unwrap_or(0);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let tab_id = format!("ssh-terminal-{}-{}", conn_id, timestamp);

        let prefix = format!("ssh-terminal-{}-", conn_id);
        let existing_count = self
            .tab_container
            .read(cx)
            .tabs()
            .iter()
            .filter(|t| t.id().starts_with(&prefix))
            .count();
        let tab_index = if existing_count > 0 {
            Some(existing_count + 1)
        } else {
            None
        };
        let sync_path = Self::terminal_sync_path_enabled(cx);
        let working_dir = restore_state
            .and_then(|state| state.working_dir.as_deref())
            .filter(|dir| {
                std::path::Path::new(dir).is_absolute()
                    || dir.contains(":\\")
                    || dir.starts_with("\\\\")
            })
            .map(str::to_string);
        let recovery_content = restore_state.and_then(|state| state.buffer_content.clone());

        let terminal_view = cx.new(|cx| {
            if let Some(recovery_content) = recovery_content.clone() {
                TerminalView::new_restored_ssh_with_index(
                    conn.clone(),
                    working_dir.as_deref(),
                    Some(recovery_content),
                    tab_index,
                    window,
                    cx,
                    sync_path,
                )
            } else {
                TerminalView::new_ssh_with_index(
                    conn.clone(),
                    tab_index,
                    window,
                    cx,
                    working_dir.as_deref(),
                    sync_path,
                )
            }
        });
        self.setup_terminal_view(&terminal_view, window, cx);
        self.tab_container.update(cx, |tc, cx| {
            let tab = TabItem::new(tab_id, "ssh", terminal_view);
            tc.add_and_activate_tab_with_focus(tab, window, cx);
        });
    }

    pub(crate) fn open_serial_terminal(
        &mut self,
        conn: StoredConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let conn_id = conn.id.unwrap_or(0);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let tab_id = format!("serial-terminal-{}-{}", conn_id, timestamp);

        let prefix = format!("serial-terminal-{}-", conn_id);
        let existing_count = self
            .tab_container
            .read(cx)
            .tabs()
            .iter()
            .filter(|t| t.id().starts_with(&prefix))
            .count();
        let tab_index = if existing_count > 0 {
            Some(existing_count + 1)
        } else {
            None
        };

        let terminal_view =
            cx.new(|cx| TerminalView::new_serial_with_index(conn, tab_index, window, cx));
        self.setup_terminal_view(&terminal_view, window, cx);
        self.tab_container.update(cx, |tc, cx| {
            let tab = TabItem::new(tab_id, "serial", terminal_view);
            tc.add_and_activate_tab_with_focus(tab, window, cx);
        });
    }

    pub(crate) fn open_sftp_view(
        &mut self,
        conn: StoredConnection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let conn_id = conn.id.unwrap_or(0);
        // 使用时间戳生成唯一 tab_id，支持同一连接打开多个 SFTP 视图
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let tab_id = format!("sftp-{}-{}", conn_id, timestamp);

        // 统计同一连接的 SFTP 视图数量，计算序号
        let prefix = format!("sftp-{}-", conn_id);
        let existing_count = self
            .tab_container
            .read(cx)
            .tabs()
            .iter()
            .filter(|t| t.id().starts_with(&prefix))
            .count();
        let tab_index = if existing_count > 0 {
            Some(existing_count + 1)
        } else {
            None
        };

        // 创建 SftpView 并订阅终端打开事件
        let sftp_view = cx.new(|cx| SftpView::new_with_index(conn, tab_index, window, cx));
        let tab_container = self.tab_container.clone();

        let subscription = cx.subscribe_in(
            &sftp_view,
            window,
            move |this, _sftp, event: &SftpViewEvent, window, cx| {
                match event {
                    SftpViewEvent::OpenLocalTerminal { working_dir } => {
                        this.open_local_terminal(Some(working_dir.clone()), "terminal", window, cx);
                    }
                    SftpViewEvent::OpenSshTerminal {
                        connection,
                        working_dir,
                    } => {
                        // 使用时间戳生成唯一 tab_id，支持打开多个 SSH 终端
                        let ts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis())
                            .unwrap_or(0);
                        let conn_id = connection.id.unwrap_or(0);
                        let tab_id = format!("ssh-terminal-{}-{}", conn_id, ts);
                        let conn = connection.clone();
                        // 统计同一连接的 SSH 终端数量
                        let prefix = format!("ssh-terminal-{}-", conn_id);
                        let existing = tab_container
                            .read(cx)
                            .tabs()
                            .iter()
                            .filter(|t| t.id().starts_with(&prefix))
                            .count();
                        let idx = if existing > 0 {
                            Some(existing + 1)
                        } else {
                            None
                        };
                        let sync_path = HomePage::terminal_sync_path_enabled(cx);
                        let terminal_view = cx.new(|cx| {
                            TerminalView::new_ssh_with_index(
                                conn,
                                idx,
                                window,
                                cx,
                                Some(working_dir),
                                sync_path,
                            )
                        });
                        this.setup_terminal_view(&terminal_view, window, cx);
                        tab_container.update(cx, |tc, cx| {
                            let tab = TabItem::new(tab_id, "ssh", terminal_view);
                            tc.add_and_activate_tab_with_focus(tab, window, cx);
                        });
                    }
                }
            },
        );
        self._subscriptions.push(subscription);

        // 添加标签页
        let tab = TabItem::new(tab_id, "sftp", sftp_view);
        self.tab_container.update(cx, |tc, cx| {
            tc.add_and_activate_tab_with_focus(tab, window, cx);
        });
    }

    fn open_redis_tab_in_mode(
        &mut self,
        conn: StoredConnection,
        workspace: Option<Workspace>,
        open_mode: DatabaseOpenMode,
        active_conn_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace_id = workspace.as_ref().and_then(|ws| ws.id);

        let (tab_id, connections, workspace_for_tab) = match open_mode {
            DatabaseOpenMode::Workspace if workspace_id.is_some() => {
                let connections = self
                    .connections
                    .iter()
                    .filter(|connection| connection.workspace_id == workspace_id)
                    .filter(|connection| connection.connection_type == ConnectionType::Redis)
                    .cloned()
                    .collect();
                let tab_id = format!("workspace-redis-tab-{}", workspace_id.unwrap_or(0));
                (tab_id, connections, workspace)
            }
            _ => {
                let conn_id = conn.id.unwrap_or(0);
                let tab_id = format!("redis-{}", conn_id);
                (tab_id, vec![conn.clone()], None)
            }
        };

        let tab_container = self.tab_container.clone();
        window.defer(cx, move |window, cx| {
            let tab_id_for_tab = tab_id.clone();
            tab_container.update(cx, |tc, cx| {
                tc.activate_or_add_tab_lazy(
                    tab_id,
                    move |window, cx| {
                        let redis_view = cx.new(|cx| {
                            RedisTabView::new_with_active_conn(
                                workspace_for_tab,
                                connections,
                                active_conn_id,
                                window,
                                cx,
                            )
                        });
                        TabItem::new(tab_id_for_tab, "redis", redis_view)
                    },
                    window,
                    cx,
                );
            });
        });
    }

    pub(crate) fn open_redis_tab(
        &mut self,
        conn: StoredConnection,
        workspace: Option<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_conn_id = conn.id;
        self.open_redis_tab_in_mode(
            conn,
            workspace,
            Self::database_open_mode(cx),
            active_conn_id,
            window,
            cx,
        );
    }

    pub(crate) fn restore_redis_tab(
        &mut self,
        conn: StoredConnection,
        workspace: Option<Workspace>,
        use_workspace_tab: bool,
        active_conn_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open_mode =
            if use_workspace_tab && workspace.as_ref().and_then(|item| item.id).is_some() {
                DatabaseOpenMode::Workspace
            } else {
                DatabaseOpenMode::Single
            };
        self.open_redis_tab_in_mode(conn, workspace, open_mode, active_conn_id, window, cx);
    }

    fn open_mongodb_tab_in_mode(
        &mut self,
        conn: StoredConnection,
        workspace: Option<Workspace>,
        open_mode: DatabaseOpenMode,
        active_conn_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace_id = workspace.as_ref().and_then(|ws| ws.id);

        let (tab_id, connections, workspace_for_tab) = match open_mode {
            DatabaseOpenMode::Workspace if workspace_id.is_some() => {
                let connections = self
                    .connections
                    .iter()
                    .filter(|connection| connection.workspace_id == workspace_id)
                    .filter(|connection| connection.connection_type == ConnectionType::MongoDB)
                    .cloned()
                    .collect();
                let tab_id = format!("workspace-mongodb-tab-{}", workspace_id.unwrap_or(0));
                (tab_id, connections, workspace)
            }
            _ => {
                let conn_id = conn.id.unwrap_or(0);
                let tab_id = format!("mongodb-{}", conn_id);
                (tab_id, vec![conn.clone()], None)
            }
        };

        let tab_container = self.tab_container.clone();
        window.defer(cx, move |window, cx| {
            let tab_id_for_tab = tab_id.clone();
            tab_container.update(cx, |tc, cx| {
                tc.activate_or_add_tab_lazy(
                    tab_id,
                    move |window, cx| {
                        let mongo_view = cx.new(|cx| {
                            MongoTabView::new_with_active_conn(
                                workspace_for_tab,
                                connections,
                                active_conn_id,
                                window,
                                cx,
                            )
                        });
                        TabItem::new(tab_id_for_tab, "mongodb", mongo_view)
                    },
                    window,
                    cx,
                );
            });
        });
    }

    pub(crate) fn open_mongodb_tab(
        &mut self,
        conn: StoredConnection,
        workspace: Option<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let active_conn_id = conn.id;
        self.open_mongodb_tab_in_mode(
            conn,
            workspace,
            Self::database_open_mode(cx),
            active_conn_id,
            window,
            cx,
        );
    }

    pub(crate) fn restore_mongodb_tab(
        &mut self,
        conn: StoredConnection,
        workspace: Option<Workspace>,
        use_workspace_tab: bool,
        active_conn_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open_mode =
            if use_workspace_tab && workspace.as_ref().and_then(|item| item.id).is_some() {
                DatabaseOpenMode::Workspace
            } else {
                DatabaseOpenMode::Single
            };
        self.open_mongodb_tab_in_mode(conn, workspace, open_mode, active_conn_id, window, cx);
    }

    pub(crate) fn add_settings_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_container = self.tab_container.clone();
        window.defer(cx, move |window, cx| {
            tab_container.update(cx, |tc, cx| {
                tc.activate_or_add_tab_lazy(
                    "settings",
                    |win, cx| {
                        let settings = cx.new(|cx| SettingsPanel::new(win, cx));
                        TabItem::new("settings", "home", settings)
                    },
                    window,
                    cx,
                );
            });
        });
    }

    pub(crate) fn add_terminal_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let home = cx.entity();
        window.defer(cx, move |window, cx| {
            home.update(cx, |this, cx| {
                this.open_local_terminal(None, "home", window, cx);
            });
        });
    }

    pub(crate) fn add_ai_chat_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_container = self.tab_container.clone();
        window.defer(cx, move |window, cx| {
            tab_container.update(cx, |tc, cx| {
                tc.activate_or_add_tab_lazy(
                    "ai-chat",
                    |win, cx| {
                        let ai_chat = cx.new(|x| ChatPanel::new(win, x));
                        TabItem::new("ai-chat", "home", ai_chat)
                    },
                    window,
                    cx,
                );
            });
        });
    }

    fn open_database_tab_in_mode(
        &mut self,
        conn: &StoredConnection,
        workspace: Option<Workspace>,
        open_mode: DatabaseOpenMode,
        active_conn_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // 在 defer 之前准备所有需要的数据，避免在 HomePage 更新期间
        // 触发 on_deactivate 导致双重借用 panic
        let workspace_id = workspace.as_ref().and_then(|w| w.id);
        let conn_clone = conn.clone();
        let connections: Vec<StoredConnection> = match open_mode {
            DatabaseOpenMode::Workspace if workspace_id.is_some() => self
                .connections
                .iter()
                .filter(|c| c.workspace_id == workspace_id)
                .filter(|c| c.connection_type == ConnectionType::Database)
                .cloned()
                .collect(),
            _ => vec![conn.clone()],
        };

        let tab_container = self.tab_container.clone();
        window.defer(cx, move |window, cx| {
            tab_container.update(cx, |tc, cx| match open_mode {
                DatabaseOpenMode::Single => {
                    let tab_id = format!("database-tab-{}", conn_clone.id.unwrap_or(0));
                    tc.activate_or_add_tab_lazy(
                        tab_id.clone(),
                        move |window, cx| {
                            let db_view = cx.new(|cx| {
                                DatabaseTabView::new_with_active_conn(
                                    None,
                                    vec![conn_clone.clone()],
                                    active_conn_id.or(conn_clone.id),
                                    window,
                                    cx,
                                )
                            });
                            TabItem::new(tab_id.clone(), "home", db_view)
                        },
                        window,
                        cx,
                    );
                }
                DatabaseOpenMode::Workspace => {
                    let tab_id = if workspace_id.is_some() {
                        format!("workspace-database-tab-{}", workspace_id.unwrap_or(0))
                    } else {
                        format!("database-tab-{}", conn_clone.id.unwrap_or(0))
                    };

                    tc.activate_or_add_tab_lazy(
                        tab_id.clone(),
                        move |window, cx| {
                            let db_view = cx.new(|cx| {
                                DatabaseTabView::new_with_active_conn(
                                    workspace,
                                    connections,
                                    active_conn_id,
                                    window,
                                    cx,
                                )
                            });
                            TabItem::new(tab_id.clone(), "home", db_view)
                        },
                        window,
                        cx,
                    );
                }
            });
        });
    }

    pub(crate) fn add_item_to_tab(
        &mut self,
        conn: &StoredConnection,
        workspace: Option<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_database_tab_in_mode(
            conn,
            workspace,
            Self::database_open_mode(cx),
            conn.id,
            window,
            cx,
        );
    }

    pub(crate) fn restore_database_tab(
        &mut self,
        conn: &StoredConnection,
        workspace: Option<Workspace>,
        use_workspace_tab: bool,
        active_conn_id: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open_mode =
            if use_workspace_tab && workspace.as_ref().and_then(|item| item.id).is_some() {
                DatabaseOpenMode::Workspace
            } else {
                DatabaseOpenMode::Single
            };
        self.open_database_tab_in_mode(conn, workspace, open_mode, active_conn_id, window, cx);
    }

    fn ssh_connection_for_tab_id(&self, tab_id: &str, cx: &App) -> Option<StoredConnection> {
        let connection_id = {
            let tab_container = self.tab_container.read(cx);
            let tab = tab_container
                .tabs()
                .iter()
                .find(|tab| tab.id().to_string() == tab_id)?;
            let terminal = tab.content().view().downcast::<TerminalView>().ok()?;
            let terminal = terminal.read(cx);

            (terminal.connection_kind(cx) == TerminalConnectionKind::Ssh)
                .then(|| terminal.connection_id(cx))
                .flatten()?
        };

        self.connections
            .iter()
            .find(|connection| connection.id == Some(connection_id))
            .cloned()
    }

    pub(crate) fn open_sftp_for_tab_id(
        &mut self,
        tab_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(connection) = self.ssh_connection_for_tab_id(tab_id, cx) {
            self.open_sftp_view(connection, window, cx);
        }
    }

    /// 复制当前活动标签并打开
    pub(crate) fn duplicate_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tc = self.tab_container.read(cx);

        // pinned tab 不支持复制
        if tc.is_pinned_tab_active() {
            return;
        }

        let Some(active_tab) = tc.active_tab() else {
            return;
        };

        let content_key = active_tab.content().content_key(cx);

        match content_key {
            "Terminal" => {
                // 获取终端视图的连接信息
                let view = active_tab.content().view();
                let Ok(terminal_view) = view.downcast::<TerminalView>() else {
                    return;
                };

                let kind = terminal_view.read(cx).connection_kind(cx);
                match kind {
                    TerminalConnectionKind::Ssh => {
                        // SSH 终端：通过 connection_id 找到 StoredConnection 并打开新连接
                        let conn_id = terminal_view.read(cx).connection_id(cx);
                        if let Some(conn_id) = conn_id {
                            if let Some(conn) = self
                                .connections
                                .iter()
                                .find(|c| c.id == Some(conn_id))
                                .cloned()
                            {
                                self.open_ssh_terminal(conn, window, cx);
                            }
                        }
                    }
                    TerminalConnectionKind::Serial => {
                        let conn_id = terminal_view.read(cx).connection_id(cx);
                        if let Some(conn_id) = conn_id {
                            if let Some(conn) = self
                                .connections
                                .iter()
                                .find(|c| c.id == Some(conn_id))
                                .cloned()
                            {
                                self.open_serial_terminal(conn, window, cx);
                            }
                        }
                    }
                    TerminalConnectionKind::Local => {
                        // 本地终端：直接新建
                        self.add_terminal_tab(window, cx);
                    }
                }
            }
            _ => {
                // 其他类型暂不支持复制
            }
        }
    }
}

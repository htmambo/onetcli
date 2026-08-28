use std::sync::Arc;
use std::time::Duration;

use crate::home_tab::{HomePage, NewConnectionShortcut, OpenConnectionQuickOpen};
use crate::saved_connection_picker::TabBarSavedConnectionPicker;
use crate::setting_tab::{AppSettings, SavedWindowBounds, build_app_http_client};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyWindowHandle, App, AppContext, Context, Entity, InteractiveElement, IntoElement, KeyBinding,
    ParentElement, Render, Styled, Task, Window, actions, div, px,
};
#[cfg(target_os = "macos")]
use gpui::{Menu, MenuItem};
use gpui_component::{WindowExt, WindowsSurfaceLayer, layered_level_surface_color};

actions!(
    omnihub_app,
    [
        ActivateTab1,
        ActivateTab2,
        ActivateTab3,
        ActivateTab4,
        ActivateTab5,
        ActivateTab6,
        ActivateTab7,
        ActivateTab8,
        ActivateTab9,
        ToggleFullscreen,
        ToggleAlwaysOnTop,
        MinimizeWindow,
        DuplicateTab,
        QuitApp,
    ]
);

#[derive(Clone)]
pub struct GlobalTabContainer {
    pub tab_container: Entity<TabContainer>,
}

impl gpui::Global for GlobalTabContainer {}

#[derive(Clone)]
pub struct GlobalHomePage {
    pub home_page: Entity<HomePage>,
}

impl gpui::Global for GlobalHomePage {}

#[derive(Clone, Copy)]
pub struct GlobalMainWindowHandle {
    pub window_handle: AnyWindowHandle,
}

impl gpui::Global for GlobalMainWindowHandle {}

use gpui_component::dock::{ClosePanel, ToggleZoom};
use gpui_component::{ActiveTheme, Icon, IconName, Root, Sizable, h_flex, v_flex};
use one_core::RunningState;
use one_core::llm::manager::GlobalProviderState;
use one_core::storage::ActiveConnections;
use one_core::tab_container::{
    TabContainer, TabContainerEvent, TabContainerState, TabContentRegistry, TabItem,
};
use one_core::tab_persistence::{save_tab_state, schedule_save};
use one_core::utils::debouncer::Debouncer;
use rust_i18n::t;
use terminal_view::with_recovery_snapshot_overrides;

mod close_guard;
mod logging;
mod system_monitor;
mod tab_restore;
mod window_actions;

use close_guard::{create_close_guard_state, request_main_window_close};
use logging::init_tracing;
use system_monitor::{GlobalSystemMonitor, format_bytes, init_system_monitor};
use tab_restore::load_startup_tabs_without_connection_restore;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use window_actions::toggle_always_on_top;
use window_actions::{
    activate_tab_by_number, duplicate_tab, open_sftp_from_tab, quit_app, toggle_fullscreen,
};

const APP_WINDOW_TITLE: &str = "OmniHub";
const GLOBAL_STATUS_BAR_HEIGHT: f32 = 28.0;
const BACKGROUND_RECOVERY_SAVE_INTERVAL_SECS: u64 = 30;
const BACKGROUND_TERMINAL_RECOVERY_SCROLLBACK_LINES: usize = 200;
const BACKGROUND_TERMINAL_RECOVERY_MAX_CHARS: usize = 128 * 1024;
pub(super) const WINDOW_CLOSE_TERMINAL_RECOVERY_MAX_CHARS: usize = 256 * 1024;

/// 连接类型统计
#[derive(Default, Clone)]
pub struct ConnectionStats {
    /// 数据库连接数
    pub db: usize,
    /// Redis 连接数
    pub redis: usize,
    /// MongoDB 连接数
    pub mongo: usize,
    /// SSH 终端连接数
    pub ssh: usize,
    /// SFTP 连接数
    pub sftp: usize,
    /// SQL Chat 连接数
    pub sql_chat: usize,
}

impl ConnectionStats {
    fn total(&self) -> usize {
        self.db + self.redis + self.mongo + self.ssh + self.sftp + self.sql_chat
    }
}

/// 从 TabContainer 统计各类型连接数
fn count_connection_stats(tab_container: &TabContainer, cx: &App) -> ConnectionStats {
    let mut stats = ConnectionStats::default();

    for tab in tab_container.tabs() {
        let key = tab.content().content_key(cx);
        match key {
            "Database" | "SqlEditor" | "TableData" | "TableDesigner" | "DatabaseObjects" => {
                stats.db += 1;
            }
            "Redis" | "RedisCli" | "KeyValue" => {
                stats.redis += 1;
            }
            "MongoDB" | "MongoCollection" => {
                stats.mongo += 1;
            }
            "Terminal" => {
                stats.ssh += 1;
            }
            "SFTP" => {
                stats.sftp += 1;
            }
            "SQL-Chat" => {
                stats.sql_chat += 1;
            }
            _ => {}
        }
    }

    stats
}

fn build_window_title(active_tab_title: Option<&str>) -> String {
    let title = active_tab_title
        .map(str::trim)
        .filter(|title| !title.is_empty() && *title != APP_WINDOW_TITLE);

    match title {
        Some(title) => format!("{APP_WINDOW_TITLE} - {title}"),
        None => APP_WINDOW_TITLE.to_string(),
    }
}

#[cfg(test)]
fn build_status_bar_title(active_tab_title: Option<&str>) -> String {
    active_tab_title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or(APP_WINDOW_TITLE)
        .to_string()
}

fn collect_running_states(tab_container: &Entity<TabContainer>, cx: &App) -> Vec<RunningState> {
    tab_container
        .read(cx)
        .tabs()
        .iter()
        .filter_map(|tab| tab.content().running_state(cx))
        .collect()
}

pub fn init(cx: &mut App) -> AppSettings {
    let settings = AppSettings::load();
    init_tracing(&settings);
    let http_client = build_app_http_client(&settings.global_proxy).expect("HTTP 客户端初始化失败");
    cx.set_http_client(http_client);
    gpui_component::init(cx);
    one_core::init(cx);
    one_ui::init(cx);
    db_view::search_shortcut::init(cx);
    db_view::sql_editor_view::init(cx);
    db_view::chatdb::agents::init(cx);
    terminal_view::agents::init(cx);
    crate::auth::init(cx, &settings);
    {
        let auth_service = crate::auth::get_auth_service(cx);
        let global_provider_state = cx.global::<GlobalProviderState>().clone();
        global_provider_state.set_cloud_client(auth_service.cloud_client());
    }
    db::init_cache(cx);
    // 启动后台磁盘缓存清理任务
    if let Some(cache) = cx.try_global::<db::GlobalNodeCache>() {
        let _ = cache.start_cleanup_task(cx);
    }
    terminal_view::init(cx);
    redis_view::init(cx);
    mongodb_view::init(cx);
    remote_desktop_view::init(cx);
    crate::home_tab::init(cx);

    // 初始化系统监控全局状态
    init_system_monitor(cx);
    let keybindings = vec![
        KeyBinding::new("shift-escape", ToggleZoom, None),
        KeyBinding::new("ctrl-w", ClosePanel, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-1", ActivateTab1, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-2", ActivateTab2, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-3", ActivateTab3, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-4", ActivateTab4, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-5", ActivateTab5, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-6", ActivateTab6, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-7", ActivateTab7, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-8", ActivateTab8, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-9", ActivateTab9, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-1", ActivateTab1, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-2", ActivateTab2, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-3", ActivateTab3, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-4", ActivateTab4, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-5", ActivateTab5, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-6", ActivateTab6, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-7", ActivateTab7, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-8", ActivateTab8, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-9", ActivateTab9, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("ctrl-cmd-f", ToggleFullscreen, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-enter", ToggleFullscreen, None),
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        KeyBinding::new("ctrl-alt-t", ToggleAlwaysOnTop, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-t", DuplicateTab, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-shift-t", DuplicateTab, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-q", QuitApp, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-f4", QuitApp, None),
    ];

    cx.bind_keys(keybindings);

    cx.on_action(|_: &ActivateTab1, cx| activate_tab_by_number(1, cx));
    cx.on_action(|_: &ActivateTab2, cx| activate_tab_by_number(2, cx));
    cx.on_action(|_: &ActivateTab3, cx| activate_tab_by_number(3, cx));
    cx.on_action(|_: &ActivateTab4, cx| activate_tab_by_number(4, cx));
    cx.on_action(|_: &ActivateTab5, cx| activate_tab_by_number(5, cx));
    cx.on_action(|_: &ActivateTab6, cx| activate_tab_by_number(6, cx));
    cx.on_action(|_: &ActivateTab7, cx| activate_tab_by_number(7, cx));
    cx.on_action(|_: &ActivateTab8, cx| activate_tab_by_number(8, cx));
    cx.on_action(|_: &ActivateTab9, cx| activate_tab_by_number(9, cx));
    cx.on_action(|_: &ToggleFullscreen, cx| toggle_fullscreen(cx));
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    cx.on_action(|_: &ToggleAlwaysOnTop, cx| toggle_always_on_top(cx));
    cx.on_action(|_: &DuplicateTab, cx| duplicate_tab(cx));
    cx.on_action(|_: &QuitApp, cx| quit_app(cx));
    cx.on_action(|_: &OpenConnectionQuickOpen, cx| {
        let Some(active_window) = cx.active_window() else {
            return;
        };
        let Some(home) = cx.try_global::<GlobalHomePage>() else {
            return;
        };
        let home_page = home.home_page.clone();
        cx.defer(move |cx| {
            _ = active_window.update(cx, |_, window, cx| {
                if window.has_active_dialog(cx) {
                    window.close_all_dialogs(cx);
                }
                home_page.update(cx, |hp, cx| {
                    hp.show_connection_quick_open(window, cx);
                });
            });
        });
    });
    cx.on_action(|_: &NewConnectionShortcut, cx| {
        let Some(active_window) = cx.active_window() else {
            return;
        };
        let Some(home) = cx.try_global::<GlobalHomePage>() else {
            return;
        };
        let home_page = home.home_page.clone();
        cx.defer(move |cx| {
            _ = active_window.update(cx, |_, window, cx| {
                if window.has_active_dialog(cx) {
                    window.close_all_dialogs(cx);
                }
                home_page.update(cx, |hp, cx| {
                    hp.show_new_connection_dialog(window, cx);
                });
            });
        });
    });

    cx.set_global(TabContentRegistry::new());

    // 设置应用菜单，将 Quit 菜单项映射到 QuitApp action。
    // 这样 Cmd+Q (macOS) 会走 GPUI 的 action 分发系统，触发关闭守卫弹窗。
    #[cfg(target_os = "macos")]
    {
        cx.set_menus(vec![Menu {
            name: "OmniHub".into(),
            items: vec![MenuItem::action("Quit OmniHub", QuitApp)],
        }]);
    }

    cx.activate(true);
    settings
}

pub struct OmniHubApp {
    tab_container: Entity<TabContainer>,
    last_layout_state: Option<TabContainerState>,
    last_background_recovery_state: Option<TabContainerState>,
    _save_layout_task: Option<Task<()>>,
    _background_recovery_task: Option<Task<()>>,
    pending_window_bounds: Option<SavedWindowBounds>,
    window_bounds_save_debouncer: Arc<Debouncer>,
    _save_window_bounds_task: Option<Task<()>>,
    window_title: String,
}

const WINDOW_BOUNDS_SAVE_DEBOUNCE_MS: u64 = 300;

impl OmniHubApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.set_global(GlobalMainWindowHandle {
            window_handle: window.window_handle(),
        });
        cx.set_global(create_close_guard_state());

        if AppSettings::global(cx).auto_switch_theme {
            let settings = AppSettings::global(cx).clone();
            settings.apply_theme_preferences(Some(window), cx);
        }

        let tab_container = cx.new(|cx| {
            let mut container = TabContainer::new(window, cx)
                .with_tab_bar_colors(None, None)
                .with_tab_item_colors(None, None)
                .with_inactive_tab_bg_color(None)
                .with_tab_content_colors(None, None);

            #[cfg(target_os = "macos")]
            {
                container = container
                    .with_left_padding(px(80.0))
                    .with_top_padding(px(4.0))
            }

            #[cfg(not(target_os = "macos"))]
            {
                container = container
                    .with_window_controls(true)
                    .with_window_close_handler(|window, cx| {
                        if request_main_window_close(window, cx) {
                            cx.quit();
                        }
                    })
            }

            #[cfg(target_os = "windows")]
            {
                container = container.with_always_on_top_control(
                    Arc::new(|_window, cx| toggle_always_on_top(cx)),
                    Arc::new(|| {
                        use std::sync::atomic::Ordering;
                        window_actions::ALWAYS_ON_TOP.load(Ordering::Relaxed)
                    }),
                )
            }

            container
        });

        window.on_window_should_close(cx, move |window, cx| {
            let should_close = request_main_window_close(window, cx);
            if should_close {
                cx.quit();
            }
            should_close
        });

        cx.set_global(GlobalTabContainer {
            tab_container: tab_container.clone(),
        });

        let registry = cx.global::<TabContentRegistry>().clone();

        let saved_active_index = match load_startup_tabs_without_connection_restore(
            &tab_container,
            &registry,
            window,
            cx,
        ) {
            Ok(active_index) => {
                tracing::info!("Tab layout loaded successfully");
                Some(active_index)
            }
            Err(err) => {
                tracing::error!("Failed to load tab layout: {:?}", err);
                None
            }
        };
        let restored_tab_count = tab_container.read(cx).tabs().len();
        let has_restored_tabs = restored_tab_count > 0;
        // Set HomePage as the pinned tab (always visible, not scrollable)
        // 先创建 HomePage 以获取待恢复连接快照
        let home_page = cx.new(|cx| HomePage::new(tab_container.clone(), window, cx));
        let has_pending_restore = home_page.read(cx).has_pending_connection_restore_snapshot();
        // 将恢复标签时保存的原始活动标签索引存入 HomePage，供跳过恢复时使用
        let _ = home_page.update(cx, |home, _| {
            home.set_saved_active_tab_index(saved_active_index);
        });
        let saved_connection_picker = {
            tab_container.update(cx, |tc, cx| {
                let saved_connection_picker =
                    cx.new(|cx| TabBarSavedConnectionPicker::new(window, cx));
                let home_tab = TabItem::new("home", "app", home_page.clone());
                tc.set_pinned_tab(home_tab, cx);
                tc.set_tab_bar_trailing_view(saved_connection_picker.clone());
                tc.set_tab_list_header_action_label(t!("Home.new_connection").to_string());
                // 如果有待恢复连接弹窗或没有恢复标签，激活 pinned tab。
                // 有待恢复弹窗时激活是为了让 HomePage 渲染并触发弹窗流程。
                if !has_restored_tabs || has_pending_restore {
                    tc.activate_pinned_tab(window, cx);
                }
                saved_connection_picker
            })
        };
        cx.set_global(GlobalHomePage {
            home_page: home_page.clone(),
        });

        let tab_container_for_events = tab_container.clone();
        let saved_connection_picker_for_events = saved_connection_picker.clone();
        cx.subscribe_in(
            &tab_container,
            window,
            move |this, _tc, ev: &TabContainerEvent, _window, cx| match ev {
                TabContainerEvent::LayoutChanged => {
                    this.save_layout(cx);
                    cx.notify();
                }
                TabContainerEvent::ActiveContentChanged
                | TabContainerEvent::TabActivated { .. }
                | TabContainerEvent::TabClosed { .. } => {
                    cx.notify();
                }
                TabContainerEvent::OpenSftpRequested { tab_id } => {
                    open_sftp_from_tab(tab_id.clone(), cx);
                }
                TabContainerEvent::TabBarTrailingActionRequested => {
                    tab_container_for_events.update(cx, |tc, cx| {
                        tc.scroll_to_tab_bar_trailing_view(cx);
                    });

                    let saved_connection_picker = saved_connection_picker_for_events.clone();
                    cx.defer(move |cx| {
                        let Some(window_id) = cx.active_window() else {
                            return;
                        };

                        let _ = cx.update_window(window_id, |_entity, window, cx| {
                            saved_connection_picker.update(cx, |picker, cx| {
                                picker.open(window, cx);
                            });
                        });
                    });
                }
            },
        )
        .detach();

        cx.observe_global::<ActiveConnections>(|_, cx| {
            cx.notify();
        })
        .detach();

        cx.observe_window_appearance(window, |_this, window, cx| {
            let settings = AppSettings::global(cx).clone();
            if settings.auto_switch_theme {
                settings.apply_theme_preferences(Some(window), cx);
            }
        })
        .detach();

        cx.observe_window_bounds(window, |this, window, cx| {
            if let Some(next_bounds) = AppSettings::snapshot_main_window_bounds(window) {
                if this.stage_window_bounds(next_bounds) {
                    this.schedule_window_bounds_save(cx);
                }
            }
        })
        .detach();

        cx.observe_window_activation(window, |_this, window, cx| {
            if !window.is_window_active() {
                return;
            }

            let settings = AppSettings::global(cx).clone();
            if settings.auto_switch_theme {
                settings.apply_theme_preferences(Some(window), cx);
            }
        })
        .detach();

        cx.on_release(|this, cx| {
            this.flush_pending_window_bounds(cx);
        })
        .detach();

        cx.on_app_quit({
            let tab_container = tab_container.clone();
            move |_, cx| {
                // 检查是否有标签页，如果没有说明已经在 force_close 中保存过了
                let has_tabs = tab_container.read(cx).tabs().len() > 0;
                if has_tabs {
                    tracing::info!("应用退出：保存标签状态");
                    let state = with_recovery_snapshot_overrides(
                        cx,
                        None,
                        Some(WINDOW_CLOSE_TERMINAL_RECOVERY_MAX_CHARS),
                        |cx| tab_container.read(cx).dump(cx),
                    );
                    if let Err(err) = save_tab_state(&state) {
                        tracing::error!("退出时保存标签状态失败：{:?}", err);
                    }
                } else {
                    tracing::info!("应用退出：标签页已清空，跳过保存（状态已在强制退出时保存）");
                }
                AppSettings::save_global(cx);
                let redis_state = cx
                    .try_global::<redis_view::manager::GlobalRedisState>()
                    .cloned();
                let mongo_state = cx
                    .try_global::<mongodb_view::manager::GlobalMongoState>()
                    .cloned();
                cx.background_executor().spawn(async move {
                    if let Some(state) = redis_state {
                        state.close_all().await;
                    }
                    if let Some(state) = mongo_state {
                        state.close_all().await;
                    }
                })
            }
        })
        .detach();

        let mut this = Self {
            tab_container,
            last_layout_state: None,
            last_background_recovery_state: None,
            _save_layout_task: None,
            _background_recovery_task: None,
            pending_window_bounds: AppSettings::snapshot_main_window_bounds(window),
            window_bounds_save_debouncer: Arc::new(Debouncer::new(Duration::from_millis(
                WINDOW_BOUNDS_SAVE_DEBOUNCE_MS,
            ))),
            _save_window_bounds_task: None,
            window_title: String::new(),
        };
        this.ensure_background_recovery_loop(cx);
        this
    }

    fn save_layout(&mut self, cx: &mut App) {
        self._save_layout_task = Some(schedule_save(
            self.tab_container.clone(),
            &mut self.last_layout_state,
            cx,
        ));
    }

    fn ensure_background_recovery_loop(&mut self, cx: &mut Context<Self>) {
        if self._background_recovery_task.is_some() {
            return;
        }

        self._background_recovery_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(BACKGROUND_RECOVERY_SAVE_INTERVAL_SECS))
                    .await;

                let keep_running = this
                    .update(cx, |this, cx| {
                        this.save_background_recovery_state(cx);
                        true
                    })
                    .unwrap_or(false);

                if !keep_running {
                    break;
                }
            }
        }));
    }

    fn save_background_recovery_state(&mut self, cx: &mut Context<Self>) {
        let state = with_recovery_snapshot_overrides(
            cx,
            Some(BACKGROUND_TERMINAL_RECOVERY_SCROLLBACK_LINES),
            Some(BACKGROUND_TERMINAL_RECOVERY_MAX_CHARS),
            |cx| self.tab_container.read(cx).dump(cx),
        );

        if Some(&state) == self.last_background_recovery_state.as_ref() {
            return;
        }

        match save_tab_state(&state) {
            Ok(()) => {
                self.last_background_recovery_state = Some(state);
            }
            Err(err) => {
                tracing::warn!("后台保存恢复状态失败：{:?}", err);
            }
        }
    }

    fn stage_window_bounds(&mut self, next_bounds: SavedWindowBounds) -> bool {
        if self.pending_window_bounds == Some(next_bounds) {
            return false;
        }

        self.pending_window_bounds = Some(next_bounds);
        true
    }

    fn flush_pending_window_bounds(&mut self, cx: &mut App) {
        if let Some(main_window_bounds) = self.pending_window_bounds {
            AppSettings::set_global_main_window_bounds(main_window_bounds, cx);
        }
    }

    fn schedule_window_bounds_save(&mut self, cx: &mut Context<Self>) {
        let debouncer = Arc::clone(&self.window_bounds_save_debouncer);
        self._save_window_bounds_task = Some(cx.spawn(async move |this, cx| {
            if debouncer.debounce(cx).await {
                let _ = this.update(cx, |this, cx| {
                    this.flush_pending_window_bounds(cx);
                    AppSettings::save_global(cx);
                });
            }
        }));
    }

    /// 渲染状态栏连接统计：总数(终端:数量/Redis:数量/Mongo:数量/HardDrive:数量)
    fn render_connection_stats(
        total: usize,
        ssh: usize,
        db: usize,
        redis: usize,
        mongo: usize,
        sftp: usize,
        sql_chat: usize,
        cx: &App,
    ) -> impl IntoElement {
        use gpui_component::h_flex;
        h_flex()
            .items_center()
            .gap_1()
            .flex_shrink_0()
            // 总数
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child(total.to_string()),
            )
            // 左括号
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("("),
            )
            // 分组内容：只显示数量>0的，用/分隔
            .when(ssh > 0, |this| {
                this.child(
                    Icon::new(IconName::Terminal)
                        .small()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground)
                        .child(ssh.to_string()),
                )
            })
            .when(
                ssh > 0 && (db > 0 || redis > 0 || mongo > 0 || sftp > 0 || sql_chat > 0),
                |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("/"),
                    )
                },
            )
            .when(db > 0, |this| {
                this.child(
                    Icon::new(IconName::Database)
                        .small()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground)
                        .child(db.to_string()),
                )
            })
            .when(
                db > 0 && (redis > 0 || mongo > 0 || sftp > 0 || sql_chat > 0),
                |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("/"),
                    )
                },
            )
            .when(redis > 0, |this| {
                this.child(
                    Icon::new(IconName::Redis)
                        .small()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground)
                        .child(redis.to_string()),
                )
            })
            .when(
                redis > 0 && (mongo > 0 || sftp > 0 || sql_chat > 0),
                |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("/"),
                    )
                },
            )
            .when(mongo > 0, |this| {
                this.child(
                    Icon::new(IconName::MongoDB)
                        .small()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground)
                        .child(mongo.to_string()),
                )
            })
            .when(mongo > 0 && (sftp > 0 || sql_chat > 0), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("/"),
                )
            })
            .when(sftp > 0 && sql_chat > 0, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("/"),
                )
            })
            .when(sftp > 0, |this| {
                this.child(
                    Icon::new(IconName::FolderOpen)
                        .small()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground)
                        .child(sftp.to_string()),
                )
            })
            .when(sql_chat > 0, |this| {
                this.child(
                    Icon::new(IconName::Bot)
                        .small()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground)
                        .child(sql_chat.to_string()),
                )
            })
            // 右括号
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(")"),
            )
    }

    fn render_global_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // 获取富文本状态摘要（优先），否则降级到纯文本
        let (status_summary_element, status_summary) = {
            let tab_container = self.tab_container.read(cx);
            (
                tab_container.current_status_summary_element(cx),
                tab_container
                    .current_status_summary(cx)
                    .map(|s| s.to_string()),
            )
        };

        // 获取系统监控数据（复制字段以避免借用冲突）
        let sys_used_mem = cx.global::<GlobalSystemMonitor>().used_memory;
        let sys_total_mem = cx.global::<GlobalSystemMonitor>().total_memory;
        let app_mem = cx.global::<GlobalSystemMonitor>().app_memory;
        let cpu_usage = cx.global::<GlobalSystemMonitor>().cpu_usage;

        // 获取各类型连接统计
        let tab_container = self.tab_container.read(cx);
        let conn_stats = count_connection_stats(&tab_container, cx);
        let total_connections = conn_stats.total();

        h_flex()
            .id("global-status-bar")
            .w_full()
            .h(px(GLOBAL_STATUS_BAR_HEIGHT))
            .px_3()
            .gap_4()
            .items_center()
            .justify_between()
            .rounded_bl(cx.theme().radius_lg)
            .rounded_br(cx.theme().radius_lg)
            .overflow_hidden()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(layered_level_surface_color(
                cx.theme().muted,
                cx.theme().window_blur_enabled,
                cx.theme().backdrop_opacity,
                2,
                WindowsSurfaceLayer::ContentBase,
            ))
            .child({
                let left = if let Some(element) = status_summary_element {
                    div().flex_1().min_w_0().truncate().child(element)
                } else if let Some(s) = status_summary {
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(s)
                } else {
                    div()
                };
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_3()
                    .child(left)
            })
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .when(total_connections > 0, |this| {
                        this.child(Self::render_connection_stats(
                            total_connections,
                            conn_stats.ssh,
                            conn_stats.db,
                            conn_stats.redis,
                            conn_stats.mongo,
                            conn_stats.sftp,
                            conn_stats.sql_chat,
                            cx,
                        ))
                    })
                    // 内存: 图标 + 已用/总量(应用)
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .flex_shrink_0()
                            .child(
                                Icon::new(IconName::MemoryStick)
                                    .small()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(div().text_xs().text_color(cx.theme().foreground).child(
                                format!(
                                    "{}/{}",
                                    format_bytes(sys_used_mem),
                                    format_bytes(sys_total_mem),
                                ),
                            ))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("({})", format_bytes(app_mem))),
                            ),
                    )
                    // CPU: 图标 + 百分比
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .flex_shrink_0()
                            .child(
                                Icon::new(IconName::Cpu)
                                    .small()
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().foreground)
                                    .child(format!("{:.0}%", cpu_usage)),
                            ),
                    ),
            )
    }
}

impl Render for OmniHubApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let next_window_title = {
            let active_title = self
                .tab_container
                .read(cx)
                .current_title(cx)
                .map(|title| title.to_string());

            build_window_title(active_title.as_deref())
        };

        if self.window_title != next_window_title {
            self.window_title = next_window_title.clone();
            window.set_window_title(&next_window_title);
        }

        let sheet_layer = Root::render_sheet_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let notification_layer = Root::render_notification_layer_with_offset(
            window,
            cx,
            Some(px(GLOBAL_STATUS_BAR_HEIGHT)),
        );

        div()
            .size_full()
            .relative()
            .bg(cx.theme().transparent)
            .child(
                v_flex()
                    .size_full()
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .w_full()
                            .child(self.tab_container.clone()),
                    )
                    .child(self.render_global_status_bar(cx)),
            )
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}

#[cfg(test)]
mod tests {
    use super::close_guard::{AppCloseDecision, AppCloseGuard};
    use super::{ConnectionStats, build_status_bar_title, build_window_title};

    #[test]
    fn 活动标签存在时拼接应用名和标签名() {
        assert_eq!(build_window_title(Some("终端")), "OmniHub - 终端");
    }

    #[test]
    fn 空标题时回退到应用名() {
        assert_eq!(build_window_title(Some("   ")), "OmniHub");
        assert_eq!(build_window_title(None), "OmniHub");
    }

    #[test]
    fn 状态栏标题在空值时回退到应用名() {
        assert_eq!(build_status_bar_title(Some("  ")), "OmniHub");
        assert_eq!(build_status_bar_title(None), "OmniHub");
    }

    #[test]
    fn 连接统计总数会汇总所有标签类型() {
        let stats = ConnectionStats {
            db: 2,
            redis: 1,
            mongo: 3,
            ssh: 4,
            sftp: 5,
            sql_chat: 6,
        };

        assert_eq!(stats.total(), 21);
        assert_eq!(ConnectionStats::default().total(), 0);
    }

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

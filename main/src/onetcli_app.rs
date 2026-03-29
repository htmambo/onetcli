use std::sync::Arc;
use std::time::Duration;

use crate::home_tab::{HomePage, NewConnectionShortcut, OpenConnectionQuickOpen};
use crate::saved_connection_picker::TabBarSavedConnectionPicker;
use crate::setting_tab::{AppSettings, SavedWindowBounds};
use gpui::{
    AnyWindowHandle, App, AppContext, Context, Entity, IntoElement, KeyBinding, ParentElement,
    Render, Styled, Task, Window, actions, div,
};

actions!(
    onetcli_app,
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

#[cfg(target_os = "macos")]
use gpui::px;

use gpui_component::dock::{ClosePanel, ToggleZoom};
use gpui_component::{ActiveTheme, Root};
use one_core::llm::manager::GlobalProviderState;
use one_core::tab_container::{
    TabContainer, TabContainerEvent, TabContainerState, TabContentRegistry, TabItem,
};
use one_core::tab_persistence::{load_tabs, save_tab_state, schedule_save};
use one_core::utils::debouncer::Debouncer;
use reqwest_client::ReqwestClient;
use rust_i18n::t;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const APP_WINDOW_TITLE: &str = "OnetCli";

fn build_window_title(active_tab_title: Option<&str>) -> String {
    let title = active_tab_title
        .map(str::trim)
        .filter(|title| !title.is_empty() && *title != APP_WINDOW_TITLE);

    match title {
        Some(title) => format!("{APP_WINDOW_TITLE} - {title}"),
        None => APP_WINDOW_TITLE.to_string(),
    }
}

fn activate_tab_by_number(number: usize, cx: &mut App) {
    let Some(active_window) = cx.active_window() else {
        return;
    };
    let Some(container) = cx.try_global::<GlobalTabContainer>() else {
        return;
    };
    let container = container.tab_container.clone();

    cx.defer(move |cx| {
        _ = active_window.update(cx, |_, window, cx| {
            container.update(cx, |tc, cx| {
                if number == 1 && tc.has_pinned_tab() {
                    tc.activate_pinned_tab(window, cx);
                    return;
                }

                let index = if tc.has_pinned_tab() {
                    number.saturating_sub(2)
                } else {
                    number.saturating_sub(1)
                };

                if index < tc.tabs().len() {
                    tc.set_active_index(index, window, cx);
                }
            });
        });
    });
}

fn toggle_fullscreen(cx: &mut App) {
    let Some(active_window) = cx.active_window() else {
        return;
    };
    cx.defer(move |cx| {
        _ = active_window.update(cx, |_, window, _| {
            window.toggle_fullscreen();
        });
    });
}

fn minimize_window(cx: &mut App) {
    let Some(active_window) = cx.active_window() else {
        return;
    };
    cx.defer(move |cx| {
        _ = active_window.update(cx, |_, window, _| {
            if window.is_window_active() {
                window.minimize_window();
            } else {
                window.activate_window();
            }
        });
    });
}

fn duplicate_tab(cx: &mut App) {
    let Some(active_window) = cx.active_window() else {
        return;
    };
    let Some(home) = cx.try_global::<GlobalHomePage>() else {
        return;
    };
    let home_page = home.home_page.clone();

    cx.defer(move |cx| {
        _ = active_window.update(cx, |_, window, cx| {
            home_page.update(cx, |hp, cx| {
                hp.duplicate_active_tab(window, cx);
            });
        });
    });
}

fn quit_app(cx: &mut App) {
    cx.quit();
}

pub fn init(cx: &mut App) {
    // 从 RUST_LOG 环境变量读取日志级别，默认 info
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(env_filter)
        .init();
    let http_client =
        std::sync::Arc::new(ReqwestClient::user_agent("one-hub").expect("HTTP 客户端初始化失败"));
    cx.set_http_client(http_client);
    gpui_component::init(cx);
    one_core::init(cx);
    one_ui::init(cx);
    db_view::chatdb::agents::init(cx);
    crate::auth::init(cx);
    {
        let auth_service = crate::auth::get_auth_service(cx);
        let global_provider_state = cx.global::<GlobalProviderState>().clone();
        global_provider_state.set_cloud_client(auth_service.cloud_client());
    }
    db::init_cache(cx);
    // 启动后台磁盘缓存清理任务
    if let Some(cache) = cx.try_global::<db::GlobalNodeCache>() {
        cache.start_cleanup_task(cx);
    }
    terminal_view::init(cx);
    redis_view::init(cx);
    mongodb_view::init(cx);
    crate::home_tab::init(cx);
    cx.bind_keys(vec![
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
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-m", MinimizeWindow, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-space", MinimizeWindow, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-t", DuplicateTab, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-shift-t", DuplicateTab, None),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-q", QuitApp, None),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("alt-f4", QuitApp, None),
    ]);

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
    cx.on_action(|_: &MinimizeWindow, cx| minimize_window(cx));
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
                home_page.update(cx, |hp, cx| {
                    hp.show_new_connection_dialog(window, cx);
                });
            });
        });
    });

    let registry = TabContentRegistry::new();
    cx.set_global(registry);
    cx.activate(true);
}

pub struct OnetCliApp {
    tab_container: Entity<TabContainer>,
    last_layout_state: Option<TabContainerState>,
    _save_layout_task: Option<Task<()>>,
    pending_window_bounds: Option<SavedWindowBounds>,
    window_bounds_save_debouncer: Arc<Debouncer>,
    _save_window_bounds_task: Option<Task<()>>,
    window_title: String,
}

const WINDOW_BOUNDS_SAVE_DEBOUNCE_MS: u64 = 300;

impl OnetCliApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.set_global(GlobalMainWindowHandle {
            window_handle: window.window_handle(),
        });

        if AppSettings::global(cx).auto_switch_theme {
            let settings = AppSettings::global(cx).clone();
            settings.apply_theme_preferences(Some(window), cx);
        }

        let tab_container = cx.new(|cx| {
            let mut container = TabContainer::new(window, cx)
                .with_tab_bar_colors(
                    Some(gpui::rgb(0x2b2b2b).into()),
                    Some(gpui::rgb(0x1e1e1e).into()),
                )
                .with_tab_item_colors(
                    Some(gpui::rgb(0x555555).into()),
                    Some(gpui::rgb(0x3a3a3a).into()),
                )
                .with_inactive_tab_bg_color(Some(gpui::rgb(0x3a3a3a).into()))
                .with_tab_content_colors(Some(gpui::white()), Some(gpui::rgb(0xaaaaaa).into()));

            #[cfg(target_os = "macos")]
            {
                container = container
                    .with_left_padding(px(80.0))
                    .with_top_padding(px(4.0))
            }

            #[cfg(not(target_os = "macos"))]
            {
                container = container.with_window_controls(true)
            }

            container
        });

        cx.set_global(GlobalTabContainer {
            tab_container: tab_container.clone(),
        });

        let registry = cx.global::<TabContentRegistry>().clone();

        match load_tabs(&tab_container, &registry, window, cx) {
            Ok(_) => {
                tracing::info!("Tab layout loaded successfully");
            }
            Err(err) => {
                tracing::error!("Failed to load tab layout: {:?}", err);
            }
        }

        // Set HomePage as the pinned tab (always visible, not scrollable)
        let saved_connection_picker = {
            let tab_container_clone = tab_container.clone();
            tab_container.update(cx, |tc, cx| {
                let home_page = cx.new(|cx| HomePage::new(tab_container_clone, window, cx));
                cx.set_global(GlobalHomePage {
                    home_page: home_page.clone(),
                });
                let saved_connection_picker =
                    cx.new(|cx| TabBarSavedConnectionPicker::new(window, cx));
                let home_tab = TabItem::new("home", "app", home_page);
                tc.set_pinned_tab(home_tab, cx);
                tc.set_tab_bar_trailing_view(saved_connection_picker.clone());
                tc.set_tab_list_header_action_label(t!("Home.new_connection").to_string());
                tc.activate_pinned_tab(window, cx);
                saved_connection_picker
            })
        };

        let tab_container_for_events = tab_container.clone();
        let saved_connection_picker_for_events = saved_connection_picker.clone();
        cx.subscribe_in(
            &tab_container,
            window,
            move |this, _tc, ev: &TabContainerEvent, _window, cx| match ev {
                TabContainerEvent::LayoutChanged => {
                    this.save_layout(cx);
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
                _ => {}
            },
        )
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
                let state = tab_container.read(cx).dump(cx);
                if let Err(err) = save_tab_state(&state) {
                    tracing::error!("退出时保存标签状态失败：{:?}", err);
                }
                AppSettings::save_global(cx);
                Task::ready(())
            }
        })
        .detach();

        Self {
            tab_container,
            last_layout_state: None,
            _save_layout_task: None,
            pending_window_bounds: AppSettings::snapshot_main_window_bounds(window),
            window_bounds_save_debouncer: Arc::new(Debouncer::new(Duration::from_millis(
                WINDOW_BOUNDS_SAVE_DEBOUNCE_MS,
            ))),
            _save_window_bounds_task: None,
            window_title: String::new(),
        }
    }

    fn save_layout(&mut self, cx: &mut App) {
        self._save_layout_task = Some(schedule_save(
            self.tab_container.clone(),
            &mut self.last_layout_state,
            cx,
        ));
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
}

impl Render for OnetCliApp {
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
        let notification_layer = Root::render_notification_layer(window, cx);

        div()
            .size_full()
            .relative()
            .bg(cx.theme().background)
            .child(div().size_full().child(self.tab_container.clone()))
            .children(sheet_layer)
            .children(dialog_layer)
            .children(notification_layer)
    }
}

#[cfg(test)]
mod tests {
    use super::build_window_title;

    #[test]
    fn 活动标签存在时拼接应用名和标签名() {
        assert_eq!(build_window_title(Some("终端")), "OnetCli - 终端");
    }

    #[test]
    fn 空标题时回退到应用名() {
        assert_eq!(build_window_title(Some("   ")), "OnetCli");
        assert_eq!(build_window_title(None), "OnetCli");
    }
}

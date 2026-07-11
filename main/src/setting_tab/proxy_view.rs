//! 设置页 - 全局代理设置弹窗（轮 9d 重构抽取）。
//!
//! 包含 `GlobalProxySettingsView`（独立 popup 窗口）、`ProxyTypeOption`、
//! `render_global_proxy_settings_item`（嵌在 SettingsPanel 内的入口项）、
//! `show_global_proxy_settings_window`（弹出 popup）、
//! `apply_global_http_client`（保存代理后刷新全局 HTTP 客户端）。
//!
//! 父模块 `SettingsPanel::render` 仅调用 `render_global_proxy_settings_item`。

use std::sync::Arc;

use gpui::http_client::{AsyncBody, HttpClient, Method, Request};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, AsyncApp, Context, Entity, FocusHandle, Focusable, FontWeight, IntoElement,
    ParentElement, Render, SharedString, Styled, WeakEntity, Window, div,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputState};
use gpui_component::scroll::ScrollableElement;
use gpui_component::select::{Select, SelectItem, SelectState};
use gpui_component::switch::Switch;
use gpui_component::{
    ActiveTheme, Disableable, IconName, IndexPath, Sizable, TitleBar, WindowExt, h_flex, v_flex,
};
use one_core::gpui_tokio::Tokio;
use one_core::llm::manager::GlobalProviderState;
use one_core::popup_window::{PopupWindowOptions, open_popup_window};
use reqwest_client::ReqwestClient;
use rust_i18n::t;

use super::app_settings::AppSettings;
use super::proxy::{GlobalProxySettings, ProxyType};
use crate::auth::get_auth_service;
use crate::setting_tab::build_app_http_client;

pub(super) fn render_global_proxy_settings_item(cx: &mut App) -> gpui::AnyElement {
    h_flex()
        .w_full()
        .justify_between()
        .items_center()
        .gap_3()
        .child(
            v_flex()
                .gap_1()
                .flex_1()
                .child(
                    div()
                        .text_sm()
                        .child(t!("Settings.General.Proxy.title").to_string()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("Settings.General.Proxy.description").to_string()),
                ),
        )
        .child(
            Button::new("settings-global-proxy")
                .icon(IconName::Globe)
                .label(t!("Settings.General.Proxy.open").to_string())
                .on_click(|_, window, cx| {
                    show_global_proxy_settings_window(window, cx);
                }),
        )
        .into_any_element()
}

#[derive(Clone, PartialEq)]
struct ProxyTypeOption {
    value: ProxyType,
    label: SharedString,
}

impl SelectItem for ProxyTypeOption {
    type Value = ProxyType;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }
}

struct GlobalProxySettingsView {
    focus_handle: FocusHandle,
    enabled: bool,
    proxy_type_select: Entity<SelectState<Vec<ProxyTypeOption>>>,
    host_input: Entity<InputState>,
    port_input: Entity<InputState>,
    username_input: Entity<InputState>,
    password_input: Entity<InputState>,
    testing: bool,
    status_message: Option<(bool, String)>,
}

impl GlobalProxySettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let current = AppSettings::global(cx).global_proxy.clone();
        let proxy_types = vec![
            ProxyTypeOption {
                value: ProxyType::Http,
                label: "HTTP".into(),
            },
            ProxyTypeOption {
                value: ProxyType::Https,
                label: "HTTPS".into(),
            },
            ProxyTypeOption {
                value: ProxyType::Socks5,
                label: "SOCKS5".into(),
            },
        ];
        let selected_index = match current.proxy_type {
            ProxyType::Http => 0,
            ProxyType::Https => 1,
            ProxyType::Socks5 => 2,
        };
        let proxy_type_select = cx.new(|cx| {
            SelectState::new(
                proxy_types,
                Some(IndexPath::new(selected_index)),
                window,
                cx,
            )
        });
        let host_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("127.0.0.1");
            if !current.host.is_empty() {
                state.set_value(current.host.clone(), window, cx);
            }
            state
        });
        let port_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("1080");
            state.set_value(current.port.to_string(), window, cx);
            state
        });
        let username_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("Settings.General.Proxy.username_placeholder"));
            if !current.username.is_empty() {
                state.set_value(current.username.clone(), window, cx);
            }
            state
        });
        let password_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .placeholder(t!("Settings.General.Proxy.password_placeholder"));
            if !current.password.is_empty() {
                state.set_value(current.password.clone(), window, cx);
            }
            state
        });

        Self {
            focus_handle: cx.focus_handle(),
            enabled: current.enabled,
            proxy_type_select,
            host_input,
            port_input,
            username_input,
            password_input,
            testing: false,
            status_message: None,
        }
    }

    fn build_proxy_settings(&self, cx: &App) -> GlobalProxySettings {
        GlobalProxySettings {
            enabled: self.enabled,
            proxy_type: self
                .proxy_type_select
                .read(cx)
                .selected_value()
                .copied()
                .unwrap_or_default(),
            host: self
                .host_input
                .read(cx)
                .text()
                .to_string()
                .trim()
                .to_string(),
            port: self
                .port_input
                .read(cx)
                .text()
                .to_string()
                .trim()
                .parse::<u16>()
                .unwrap_or(0),
            username: self
                .username_input
                .read(cx)
                .text()
                .to_string()
                .trim()
                .to_string(),
            password: self.password_input.read(cx).text().to_string(),
        }
    }

    fn render_form_row(
        &self,
        label: String,
        child: impl IntoElement,
        disabled: bool,
        cx: &App,
    ) -> gpui::AnyElement {
        h_flex()
            .gap_3()
            .items_center()
            .child(
                div()
                    .w(gpui::px(120.0))
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child(label),
            )
            .child(
                div()
                    .flex_1()
                    .child(child)
                    .when(disabled, |this| this.opacity(0.55)),
            )
            .into_any_element()
    }

    fn on_test(&mut self, cx: &mut Context<Self>) {
        if self.testing || !self.enabled {
            return;
        }

        let proxy_settings = self.build_proxy_settings(cx);
        let client = match build_app_http_client(&proxy_settings) {
            Ok(client) => client,
            Err(err) => {
                self.status_message = Some((false, err));
                cx.notify();
                return;
            }
        };

        self.testing = true;
        self.status_message = None;
        cx.notify();

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let test_task = Tokio::spawn(cx, async move {
                let http_client: Arc<dyn HttpClient> = client;
                test_proxy_connectivity(http_client).await
            });

            let result = match test_task.await {
                Ok(result) => result,
                Err(err) => Err(format!("代理测试任务执行失败: {}", err)),
            };

            let _ = this.update(cx, |view, cx| {
                view.testing = false;
                view.status_message = Some(match result {
                    Ok(()) => (true, t!("Settings.General.Proxy.test_success").to_string()),
                    Err(err) => (false, err),
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn on_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.testing {
            return;
        }

        let proxy_settings = self.build_proxy_settings(cx);
        let new_client = match build_app_http_client(&proxy_settings) {
            Ok(client) => client,
            Err(err) => {
                self.status_message = Some((false, err));
                cx.notify();
                return;
            }
        };

        let proxy_settings_for_apply = proxy_settings.clone();
        let new_client_for_apply = new_client.clone();
        cx.defer(move |cx| {
            let settings = AppSettings::global_mut(cx);
            settings.global_proxy = proxy_settings_for_apply;
            settings.save();
            apply_global_http_client(new_client_for_apply, cx);
        });

        window.push_notification(t!("Settings.General.Proxy.save_success").to_string(), cx);
        window.remove_window();
    }

    fn on_cancel(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        if self.testing {
            return;
        }
        window.remove_window();
    }
}

impl Focusable for GlobalProxySettingsView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GlobalProxySettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let disabled = !self.enabled;

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
                        .font_weight(FontWeight::MEDIUM)
                        .child(t!("Settings.General.Proxy.dialog_title").to_string()),
                ),
            )
            .child(
                div().flex_1().min_h_0().overflow_y_scrollbar().p_4().child(
                    v_flex()
                        .gap_4()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(t!("Settings.General.Proxy.dialog_desc").to_string()),
                        )
                        .child(
                            h_flex()
                                .justify_between()
                                .items_center()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(t!("Settings.General.Proxy.enable").to_string()),
                                )
                                .child(
                                    Switch::new("global-proxy-enabled")
                                        .checked(self.enabled)
                                        .on_click(cx.listener(|view, checked, _, cx| {
                                            view.enabled = *checked;
                                            view.status_message = None;
                                            cx.notify();
                                        })),
                                ),
                        )
                        .child(self.render_form_row(
                            t!("Settings.General.Proxy.type").to_string(),
                            Select::new(&self.proxy_type_select).disabled(disabled),
                            disabled,
                            cx,
                        ))
                        .child(self.render_form_row(
                            t!("Settings.General.Proxy.host").to_string(),
                            Input::new(&self.host_input).disabled(disabled),
                            disabled,
                            cx,
                        ))
                        .child(self.render_form_row(
                            t!("Settings.General.Proxy.port").to_string(),
                            Input::new(&self.port_input).disabled(disabled),
                            disabled,
                            cx,
                        ))
                        .child(self.render_form_row(
                            t!("Settings.General.Proxy.username").to_string(),
                            Input::new(&self.username_input).disabled(disabled),
                            disabled,
                            cx,
                        ))
                        .child(
                            self.render_form_row(
                                t!("Settings.General.Proxy.password").to_string(),
                                Input::new(&self.password_input)
                                    .mask_toggle()
                                    .disabled(disabled),
                                disabled,
                                cx,
                            ),
                        )
                        .when_some(self.status_message.clone(), |this, (success, message)| {
                            this.child(
                                div()
                                    .text_sm()
                                    .text_color(if success {
                                        cx.theme().muted_foreground
                                    } else {
                                        cx.theme().danger
                                    })
                                    .child(message),
                            )
                        }),
                ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .justify_end()
                    .gap_2()
                    .p_4()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("proxy-test")
                            .small()
                            .label(if self.testing {
                                t!("Settings.General.Proxy.testing").to_string()
                            } else {
                                t!("Settings.General.Proxy.test").to_string()
                            })
                            .disabled(self.testing || !self.enabled)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.on_test(cx);
                            })),
                    )
                    .child(
                        Button::new("proxy-cancel")
                            .small()
                            .label(t!("Common.cancel").to_string())
                            .disabled(self.testing)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.on_cancel(window, cx);
                            })),
                    )
                    .child(
                        Button::new("proxy-save")
                            .small()
                            .primary()
                            .label(t!("Common.save").to_string())
                            .disabled(self.testing)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.on_save(window, cx);
                            })),
                    ),
            )
    }
}

fn show_global_proxy_settings_window(window: &mut Window, cx: &mut App) {
    open_popup_window(
        window,
        PopupWindowOptions::new(t!("Settings.General.Proxy.dialog_title").to_string())
            .size(560.0, 460.0),
        move |window, cx| cx.new(|cx| GlobalProxySettingsView::new(window, cx)),
        cx,
    );
}

fn apply_global_http_client(http_client: Arc<ReqwestClient>, cx: &mut App) {
    let auth_service = get_auth_service(cx);
    let http_for_auth: Arc<dyn HttpClient> = http_client.clone();
    auth_service.replace_http_client(http_for_auth, AppSettings::global(cx));

    if let Some(provider_state) = cx.try_global::<GlobalProviderState>() {
        provider_state.set_cloud_client(auth_service.cloud_client());
        provider_state.manager().clear_cache();
    }

    cx.set_http_client(http_client);
}

async fn test_proxy_connectivity(http_client: Arc<dyn HttpClient>) -> Result<(), String> {
    let request = Request::builder()
        .method(Method::HEAD)
        .uri("https://www.gstatic.com/generate_204")
        .header("User-Agent", "omnihub-updater")
        .body(AsyncBody::empty())
        .map_err(|err| format!("构建代理测试请求失败: {}", err))?;

    let response = http_client
        .send(request)
        .await
        .map_err(|err| format!("代理连接测试失败: {}", err))?;

    if !response.status().is_success() {
        return Err(format!("代理测试返回异常状态码: {}", response.status()));
    }

    Ok(())
}

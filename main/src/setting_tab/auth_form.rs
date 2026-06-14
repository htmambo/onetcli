//! 设置页 - 账号登录/注册表单（轮 9c 重构抽取）。
//!
//! 仅供父模块 `SettingsPanel::render` 使用，render 函数标 `pub(super)`。

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, AsyncApp, Entity, FontWeight, IntoElement, ParentElement, Styled,
    Window, div, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputState};
use gpui_component::{Icon, IconName, Sizable, h_flex, v_flex};
use one_core::cloud_sync::UserInfo;
use rust_i18n::t;

use super::global_user::GlobalCurrentUser;
use crate::auth::{PasswordAuthAction, get_auth_service};
use crate::onetcli_app::GlobalHomePage;
use crate::sync_server_theme;

/// 同步认证表单状态（独立 Entity，通过 lazy init 创建）
struct SyncAuthForm {
    email_input: Entity<InputState>,
    password_input: Entity<InputState>,
    confirm_password_input: Entity<InputState>,
    error: Entity<Option<String>>,
    is_sign_up: bool,
    is_submitting: bool,
}

struct GlobalSyncAuthForm(Entity<SyncAuthForm>);
impl gpui::Global for GlobalSyncAuthForm {}

impl SyncAuthForm {
    fn new(window: &mut Window, cx: &mut App) -> Self {
        Self {
            email_input: cx
                .new(|cx| InputState::new(window, cx).placeholder(t!("Auth.email_placeholder"))),
            password_input: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(t!("Auth.password_placeholder"))
                    .masked(true)
            }),
            confirm_password_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder(t!("Auth.confirm_password_placeholder"))
            }),
            error: cx.new(|_| None),
            is_sign_up: false,
            is_submitting: false,
        }
    }

    fn init(window: &mut Window, cx: &mut App) -> Entity<Self> {
        if !cx.has_global::<GlobalSyncAuthForm>() {
            let form = cx.new(|cx| Self::new(window, cx));
            cx.set_global(GlobalSyncAuthForm(form));
        }
        cx.global::<GlobalSyncAuthForm>().0.clone()
    }

    fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalSyncAuthForm>().map(|g| g.0.clone())
    }
}

/// 在 SyncAuthForm 中执行登录/注册认证
fn auth_submit(cx: &mut App) {
    let Some(form) = SyncAuthForm::global(cx) else {
        return;
    };
    let state = form.read(cx);
    let email = state.email_input.read(cx).text().to_string();
    let password = state.password_input.read(cx).text().to_string();
    let confirm_password = state.confirm_password_input.read(cx).text().to_string();
    let is_sign_up = state.is_sign_up;
    let _ = state;

    if email.is_empty() {
        form.update(cx, |this, cx| {
            this.error.update(cx, |v, cx| {
                *v = Some(t!("Auth.email_required").to_string());
                cx.notify();
            });
        });
        return;
    }
    if password.is_empty() {
        form.update(cx, |this, cx| {
            this.error.update(cx, |v, cx| {
                *v = Some(t!("Auth.password_required").to_string());
                cx.notify();
            });
        });
        return;
    }
    if is_sign_up && password != confirm_password {
        form.update(cx, |this, cx| {
            this.error.update(cx, |v, cx| {
                *v = Some(t!("Auth.password_mismatch").to_string());
                cx.notify();
            });
        });
        return;
    }

    form.update(cx, |this, cx| {
        this.is_submitting = true;
        this.error.update(cx, |v, cx| {
            *v = None;
            cx.notify();
        });
        cx.notify();
    });

    let action = if is_sign_up {
        PasswordAuthAction::SignUp
    } else {
        PasswordAuthAction::Login
    };
    let auth = get_auth_service(cx);
    let form_weak = form.downgrade();
    let home_page = cx
        .try_global::<GlobalHomePage>()
        .map(|h| h.home_page.clone());

    cx.spawn(async move |cx: &mut AsyncApp| {
        let result = match action {
            PasswordAuthAction::Login => auth.login_with_password(&email, &password).await,
            PasswordAuthAction::SignUp => auth.sign_up_with_password(&email, &password).await,
        };

        let _ = form_weak.update(cx, |this, cx| {
            this.is_submitting = false;
            match &result {
                Ok(user) => {
                    GlobalCurrentUser::set_user(Some(user.clone()), cx);
                    cx.notify();
                }
                Err(error) => {
                    tracing::error!("密码登录失败: {}", error);
                    this.error.update(cx, |v, cx| {
                        *v = Some(error.clone());
                        cx.notify();
                    });
                }
            }
        });

        if let Some(home_page) = home_page
            && let Ok(user) = result
        {
            let _ = home_page.update(cx, |h, cx| {
                h.handle_auth_state_restored(user, cx);
            });
        }
    })
    .detach();
}

/// 渲染已登录用户信息（同步分组内）
pub(super) fn render_logged_in_user_sync(user: &UserInfo, _cx: &mut App) -> AnyElement {
    let display_name = user.display_name();
    let secondary_identity = user
        .secondary_identity()
        .unwrap_or_else(|| user.email.clone());

    v_flex()
        .gap_3()
        .p_3()
        .rounded_md()
        .bg(sync_server_theme::surface_alt())
        .border_1()
        .border_color(sync_server_theme::border())
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(Icon::new(IconName::User).with_size(px(16.)))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(sync_server_theme::text())
                        .child(display_name),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(sync_server_theme::text_muted())
                .child(secondary_identity),
        )
        .child(
            h_flex().justify_end().mt_1().child(
                Button::new("sync-logout-button")
                    .label(t!("Auth.logout"))
                    .ghost()
                    .text_color(sync_server_theme::danger())
                    .on_click(move |_, _window, cx| {
                        let auth = get_auth_service(cx);
                        let home_page = cx
                            .try_global::<GlobalHomePage>()
                            .map(|h| h.home_page.clone());
                        cx.spawn(async move |cx: &mut AsyncApp| {
                            auth.sign_out().await;
                            let _ = cx.update(|cx| {
                                GlobalCurrentUser::set_user(None, cx);
                            });
                            if let Some(home_page) = home_page {
                                let _ = home_page.update(cx, |h, cx| {
                                    h.handle_auth_state_cleared(cx);
                                });
                            }
                        })
                        .detach();
                    }),
            ),
        )
        .into_any_element()
}

/// 渲染未登录时的登录/注册表单（同步分组内）
pub(super) fn render_auth_form_sync(window: &mut Window, cx: &mut App) -> AnyElement {
    let form = SyncAuthForm::init(window, cx);

    v_flex()
        .gap_3()
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(sync_server_theme::text_muted())
                .child(t!("Settings.General.Sync.account_auth").to_string()),
        )
        .child(
            v_flex()
                .gap_2()
                .child(Input::new(&form.read(cx).email_input).w_full())
                .child(
                    Input::new(&form.read(cx).password_input)
                        .w_full()
                        .mask_toggle(),
                )
                .when(form.read(cx).is_sign_up, |this| {
                    this.child(Input::new(&form.read(cx).confirm_password_input).w_full())
                }),
        )
        .child(
            h_flex()
                .gap_2()
                .child(
                    Button::new("auth-submit")
                        .label(if form.read(cx).is_sign_up {
                            t!("Auth.sign_up")
                        } else {
                            t!("Auth.login")
                        })
                        .w_full()
                        .on_click(move |_, _window, cx: &mut App| {
                            auth_submit(cx);
                        }),
                )
                .child(
                    Button::new("auth-switch")
                        .label(if form.read(cx).is_sign_up {
                            t!("Auth.switch_to_login")
                        } else {
                            t!("Auth.switch_to_sign_up")
                        })
                        .ghost()
                        .on_click({
                            let f = form.clone();
                            move |_, _window, cx: &mut App| {
                                f.update(cx, |this, cx| {
                                    this.is_sign_up = !this.is_sign_up;
                                    this.error.update(cx, |v, cx| {
                                        *v = None;
                                        cx.notify();
                                    });
                                    cx.notify();
                                });
                            }
                        }),
                ),
        )
        .when_some(form.read(cx).error.read(cx).clone(), |this, msg| {
            this.child(
                div()
                    .text_xs()
                    .text_color(sync_server_theme::danger())
                    .child(msg),
            )
        })
        .into_any_element()
}

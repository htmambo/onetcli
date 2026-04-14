//! GitHub OAuth Device Flow Authorization Dialog
//!
//! 显示 user_code 和验证链接，引导用户完成 GitHub 授权。

use gpui::{
    App, ClipboardItem, Context, Entity, IntoElement, ParentElement, Render, Styled, Task, Window,
    div, px,
};
use gpui_component::{
    ActiveTheme, StyledExt, WindowExt,
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex, v_flex,
};

use crate::setting_tab::AppSettings;

const MAX_POLL_ATTEMPTS: u32 = 60;

/// 授权状态
#[derive(Clone)]
enum AuthState {
    /// 正在启动 device flow
    Starting,
    /// 等待用户授权
    WaitingForAuth {
        verification_uri: String,
        user_code: String,
    },
    /// 正在轮询 token
    Polling,
    /// 授权成功
    Success { gist_id: String },
    /// 授权失败
    Error { message: String },
}

impl Default for AuthState {
    fn default() -> Self {
        Self::Starting
    }
}

/// GitHub 授权对话框
pub struct GithubAuthDialog {
    client_id: String,
    state: AuthState,
    device_code: Option<String>,
    started: bool,
    url_opened: bool,
    _auth_task: Option<Task<()>>,
}

impl GithubAuthDialog {
    pub fn new(client_id: String) -> Self {
        Self {
            client_id,
            state: AuthState::Starting,
            device_code: None,
            started: false,
            url_opened: false,
            _auth_task: None,
        }
    }

    fn start_auth(&mut self, entity: Entity<Self>, cx: &mut App) {
        if self.started {
            return;
        }
        self.started = true;

        let client_id = self.client_id.clone();
        let http = cx.http_client();
        let this = entity.downgrade();

        let task = cx.spawn(async move |cx| {
            let client =
                one_core::cloud_sync::oauth::github_device::GithubOAuthClient::new(http, client_id);

            match client.start_device_flow().await {
                Ok(resp) => {
                    let _ = this.update(&mut *cx, |dialog, cx| {
                        dialog.device_code = Some(resp.device_code);
                        dialog.state = AuthState::WaitingForAuth {
                            verification_uri: resp.verification_uri,
                            user_code: resp.user_code,
                        };
                        cx.notify();
                    });
                }
                Err(e) => {
                    let _ = this.update(&mut *cx, |dialog, _| {
                        dialog.state = AuthState::Error {
                            message: e.to_string(),
                        };
                    });
                }
            }
        });

        self._auth_task = Some(task);
    }

    /// 开始轮询 token（由调用方触发）
    pub fn start_poll(&mut self, entity: Entity<Self>, cx: &mut App) {
        let Some(ref device_code) = self.device_code else {
            return;
        };
        if matches!(&self.state, AuthState::Polling | AuthState::Success { .. }) {
            return;
        }
        self.state = AuthState::Polling;

        let client_id = self.client_id.clone();
        let http = cx.http_client();
        let http_for_gist = cx.http_client();
        let device_code = device_code.clone();
        let this = entity.downgrade();

        let task = cx.spawn(async move |cx| {
            let client =
                one_core::cloud_sync::oauth::github_device::GithubOAuthClient::new(http, client_id);

            // 轮询 token，max_attempts 控制总次数，interval 5 秒
            match client
                .poll_for_token(&device_code, 5, MAX_POLL_ATTEMPTS)
                .await
            {
                Ok(tokens) => {
                    let tokens_for_settings = tokens.clone();
                    let gist_result = {
                        use one_core::cloud_sync::oauth::{create_vault_gist, find_vault_gist};
                        if let Ok(Some(id)) = find_vault_gist(http_for_gist.clone(), &tokens).await
                        {
                            Ok(id)
                        } else {
                            create_vault_gist(http_for_gist, &tokens)
                                .await
                                .map_err(|e| e.to_string())
                        }
                    };

                    let _ = this.update(cx, |dialog, cx| {
                        dialog.state = match gist_result {
                            Ok(id) => {
                                let settings = AppSettings::global_mut(cx);
                                settings.sync_backend_type = "github_gist".to_string();
                                let gist =
                                    settings.gist_config.get_or_insert_with(Default::default);
                                gist.client_id = dialog.client_id.clone();
                                gist.gist_id = Some(id.clone());
                                gist.tokens = Some(tokens_for_settings.clone());
                                settings.save();
                                cx.notify();
                                AuthState::Success { gist_id: id }
                            }
                            Err(e) => AuthState::Error { message: e },
                        };
                    });
                }
                Err(e) => {
                    let err_str = e.to_string();
                    let _ = this.update(cx, |dialog, _| {
                        dialog.state = AuthState::Error { message: err_str };
                    });
                }
            }
        });

        self._auth_task = Some(task);
    }
}

impl Render for GithubAuthDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        if !self.started {
            self.start_auth(cx.entity(), cx);
        }

        match &self.state {
            AuthState::Starting => v_flex()
                .gap_4()
                .p_5()
                .w(px(420.))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("正在启动授权流程..."),
                )
                .into_any_element(),

            AuthState::WaitingForAuth {
                verification_uri,
                user_code,
            } => {
                if !self.url_opened {
                    self.url_opened = true;
                    cx.open_url(verification_uri);
                }
                let url = verification_uri.clone();
                let entity = cx.entity();
                let code = user_code.clone();
                v_flex()
                    .gap_4()
                    .p_5()
                    .w(px(420.))
                    .child(div().text_sm().font_semibold().child("GitHub 授权"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("请在浏览器中打开链接并输入验证码："),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("open-url")
                                    .with_variant(ButtonVariant::Ghost)
                                    .on_click(move |_, _, cx: &mut App| {
                                        cx.open_url(&url);
                                    })
                                    .child(verification_uri.to_string()),
                            )
                            .child(
                                Button::new("copy-code")
                                    .with_variant(ButtonVariant::Ghost)
                                    .on_click({
                                        let code_for_clip = code.clone();
                                        move |_, _, cx: &mut App| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                code_for_clip.clone(),
                                            ));
                                        }
                                    })
                                    .child("复制验证码"),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .justify_center()
                            .p_4()
                            .rounded_md()
                            .bg(cx.theme().popover)
                            .child(div().text_xl().font_bold().child(format!(
                                "{} {} {}",
                                &code[..4],
                                &code[4..8],
                                &code[8..]
                            ))),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("授权成功后点击下方按钮确认"),
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .mt_4()
                            .child(
                                Button::new("cancel-btn")
                                    .with_variant(ButtonVariant::Ghost)
                                    .on_click(|_, window, cx: &mut App| {
                                        window.close_dialog(cx);
                                    })
                                    .child("取消"),
                            )
                            .child(
                                Button::new("confirm-btn")
                                    .with_variant(ButtonVariant::Primary)
                                    .on_click(move |_, _, cx: &mut App| {
                                        entity.update(cx, |d, cx| {
                                            d.start_poll(cx.entity(), cx);
                                        });
                                    })
                                    .child("完成授权"),
                            ),
                    )
                    .into_any_element()
            }

            AuthState::Polling => v_flex()
                .gap_4()
                .p_5()
                .w(px(420.))
                .child(div().text_sm().font_semibold().child("等待授权确认..."))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("已在浏览器中完成授权？正在确认... (最多等待 5 分钟)"),
                )
                .into_any_element(),

            AuthState::Success { gist_id } => v_flex()
                .gap_4()
                .p_5()
                .w(px(420.))
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(cx.theme().success)
                        .child("授权成功！"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Gist ID: {}", gist_id)),
                )
                .child(
                    h_flex().justify_end().gap_2().mt_4().child(
                        Button::new("success-confirm-btn")
                            .with_variant(ButtonVariant::Primary)
                            .on_click(move |_, window, cx: &mut App| {
                                window.close_dialog(cx);
                            })
                            .child("确认"),
                    ),
                )
                .into_any_element(),

            AuthState::Error { message } => {
                let msg = message.clone();
                v_flex()
                    .gap_4()
                    .p_5()
                    .w(px(420.))
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().danger)
                            .child("授权失败"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(msg),
                    )
                    .into_any_element()
            }
        }
    }
}

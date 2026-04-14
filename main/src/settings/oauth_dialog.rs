//! Google Drive 和 OneDrive OAuth PKCE 授权对话框
//!
//! 通过浏览器完成 OAuth PKCE 授权流程。

use gpui::{
    App, Context, Entity, IntoElement, ParentElement, Render, Styled, Task, Window, div,
    http_client::HttpClient, px,
};
use gpui_component::{
    ActiveTheme, StyledExt, WindowExt,
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex, v_flex,
};
use std::sync::Arc;

use crate::setting_tab::{AppSettings, GoogleDriveSettings, OneDriveSettings};

/// OAuth 授权状态
#[derive(Clone)]
enum OAuthState {
    Building,
    WaitingForAuth { auth_url: String },
    Exchanging,
    Success,
    Error { message: String },
}

impl Default for OAuthState {
    fn default() -> Self {
        Self::Building
    }
}

// ============================================================================
// Google Drive 授权对话框
// ============================================================================

/// Google Drive OAuth 授权对话框
pub struct GoogleDriveAuthDialog {
    client_id: String,
    client_secret: String,
    state: OAuthState,
    _auth_task: Option<Task<()>>,
}

impl GoogleDriveAuthDialog {
    pub fn new(client_id: String, client_secret: String) -> Self {
        Self {
            client_id,
            client_secret,
            state: OAuthState::Building,
            _auth_task: None,
        }
    }

    fn build_auth_url(&self) -> (String, String, String) {
        use one_core::cloud_sync::oauth::pkce::{generate_code_challenge, generate_code_verifier};

        let verifier = generate_code_verifier();
        let challenge = generate_code_challenge(&verifier);
        let state = format!(
            "ONetCli_GDrive_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );

        let url = format!(
            "https://accounts.google.com/o/oauth2/v2/auth\
             ?client_id={}\
             &response_type=code\
             &redirect_uri={}\
             &scope={}\
             &state={}\
             &code_challenge={}\
             &code_challenge_method=S256",
            encode_param(&self.client_id),
            encode_param("http://localhost:8787/oauth/callback"),
            encode_param("https://www.googleapis.com/auth/drive.appdata"),
            encode_param(&state),
            encode_param(&challenge),
        );
        (url, state, verifier)
    }

    fn start_auth(&mut self, entity: Entity<Self>, cx: &mut App) {
        let (auth_url, state, _verifier) = self.build_auth_url();
        self.state = OAuthState::WaitingForAuth {
            auth_url: auth_url.clone(),
        };

        let state_for_exchange = state;
        let client_id = self.client_id.clone();
        let client_secret = self.client_secret.clone();
        let this = entity.downgrade();
        let http = cx.http_client();

        let task = cx.spawn(async move |cx| {
            match one_core::cloud_sync::oauth::callback_server::start_callback_server(8787, 300) {
                Ok((_port, callback)) => {
                    if !callback.is_success() {
                        let msg = callback
                            .error_description
                            .unwrap_or_else(|| "授权失败".to_string());
                        let _ = this.update(cx, |d, _| {
                            d.state = OAuthState::Error { message: msg };
                        });
                        return;
                    }

                    if callback.state.as_deref() != Some(&state_for_exchange) {
                        let _ = this.update(cx, |d, _| {
                            d.state = OAuthState::Error {
                                message: "State 不匹配".to_string(),
                            };
                        });
                        return;
                    }

                    let _ = this.update(cx, |d, _| {
                        d.state = OAuthState::Exchanging;
                    });

                    let code = callback.code;
                    let token_result = exchange_google_token(
                        http.clone(),
                        &client_id,
                        &client_secret,
                        &code,
                        "http://localhost:8787/oauth/callback",
                    )
                    .await;

                    let _ = this.update(cx, |d, cx| match token_result {
                        Ok(tokens) => {
                            let settings = AppSettings::global_mut(cx);
                            let gd = settings
                                .google_drive_config
                                .get_or_insert_with(GoogleDriveSettings::default);
                            gd.client_id = client_id;
                            gd.client_secret = client_secret;
                            gd.tokens = Some(tokens);
                            settings.save();
                            cx.notify();
                            d.state = OAuthState::Success;
                        }
                        Err(e) => {
                            d.state = OAuthState::Error { message: e };
                        }
                    });
                }
                Err(e) => {
                    let _ = this.update(cx, |d, _| {
                        d.state = OAuthState::Error {
                            message: format!("启动回调服务器失败: {}", e),
                        };
                    });
                }
            }
        });

        self._auth_task = Some(task);
    }
}

impl Render for GoogleDriveAuthDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        match &self.state {
            OAuthState::Building => {
                self.start_auth(cx.entity(), cx);
                v_flex()
                    .gap_4()
                    .p_5()
                    .w(px(420.))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("正在启动授权流程..."),
                    )
                    .into_any_element()
            }
            OAuthState::WaitingForAuth { auth_url } => {
                let url = auth_url.clone();
                v_flex()
                    .gap_4()
                    .p_5()
                    .w(px(420.))
                    .child(div().text_sm().font_semibold().child("Google Drive 授权"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("点击下方按钮在浏览器中打开授权页面："),
                    )
                    .child(
                        h_flex().gap_2().child(
                            Button::new("open-url")
                                .with_variant(ButtonVariant::Ghost)
                                .on_click(move |_, _, cx: &mut App| {
                                    cx.open_url(&url);
                                })
                                .child("打开授权页面"),
                        ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("授权完成后此窗口将自动确认。"),
                    )
                    .child(
                        h_flex().justify_end().gap_2().mt_4().child(
                            Button::new("cancel-btn")
                                .with_variant(ButtonVariant::Ghost)
                                .on_click(|_, window, cx: &mut App| {
                                    window.close_dialog(cx);
                                })
                                .child("取消"),
                        ),
                    )
                    .into_any_element()
            }
            OAuthState::Exchanging => v_flex()
                .gap_4()
                .p_5()
                .w(px(420.))
                .child(div().text_sm().font_semibold().child("正在获取访问权限..."))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("完成授权后正在交换访问令牌..."),
                )
                .into_any_element(),
            OAuthState::Success => v_flex()
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
                        .child("Google Drive 已成功连接。"),
                )
                .child(
                    h_flex().justify_end().gap_2().mt_4().child(
                        Button::new("success-confirm-btn")
                            .with_variant(ButtonVariant::Primary)
                            .on_click(|_, window, cx: &mut App| {
                                window.close_dialog(cx);
                            })
                            .child("确认"),
                    ),
                )
                .into_any_element(),
            OAuthState::Error { message } => {
                let entity = cx.entity();
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
                            .child(message.clone()),
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .mt_4()
                            .child(
                                Button::new("error-cancel-btn")
                                    .with_variant(ButtonVariant::Ghost)
                                    .on_click(|_, window, cx: &mut App| {
                                        window.close_dialog(cx);
                                    })
                                    .child("关闭"),
                            )
                            .child(
                                Button::new("retry-btn")
                                    .with_variant(ButtonVariant::Primary)
                                    .on_click(move |_, _, cx: &mut App| {
                                        entity.update(cx, |d, cx| {
                                            d.state = OAuthState::Building;
                                            d.start_auth(cx.entity(), cx);
                                        });
                                    })
                                    .child("重试"),
                            ),
                    )
                    .into_any_element()
            }
        }
    }
}

// ============================================================================
// OneDrive 授权对话框
// ============================================================================

/// OneDrive OAuth 授权对话框
pub struct OneDriveAuthDialog {
    client_id: String,
    state: OAuthState,
    _auth_task: Option<Task<()>>,
}

impl OneDriveAuthDialog {
    pub fn new(client_id: String) -> Self {
        Self {
            client_id,
            state: OAuthState::Building,
            _auth_task: None,
        }
    }

    fn build_auth_url(&self) -> (String, String, String) {
        use one_core::cloud_sync::oauth::pkce::{generate_code_challenge, generate_code_verifier};

        let verifier = generate_code_verifier();
        let challenge = generate_code_challenge(&verifier);
        let state = format!(
            "ONetCli_ODrive_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );

        let url = format!(
            "https://login.microsoftonline.com/common/oauth2/v2.0/authorize\
             ?client_id={}\
             &response_type=code\
             &redirect_uri={}\
             &scope={}\
             &state={}\
             &code_challenge={}\
             &code_challenge_method=S256",
            encode_param(&self.client_id),
            encode_param("http://localhost:8787/oauth/callback"),
            encode_param("Files.ReadWrite offline_access"),
            encode_param(&state),
            encode_param(&challenge),
        );
        (url, state, verifier)
    }

    fn start_auth(&mut self, entity: Entity<Self>, cx: &mut App) {
        let (auth_url, state, _verifier) = self.build_auth_url();
        self.state = OAuthState::WaitingForAuth {
            auth_url: auth_url.clone(),
        };

        let state_for_exchange = state;
        let client_id = self.client_id.clone();
        let this = entity.downgrade();
        let http = cx.http_client();

        let task = cx.spawn(async move |cx| {
            match one_core::cloud_sync::oauth::callback_server::start_callback_server(8787, 300) {
                Ok((_port, callback)) => {
                    if !callback.is_success() {
                        let msg = callback
                            .error_description
                            .unwrap_or_else(|| "授权失败".to_string());
                        let _ = this.update(cx, |d, _| {
                            d.state = OAuthState::Error { message: msg };
                        });
                        return;
                    }

                    if callback.state.as_deref() != Some(&state_for_exchange) {
                        let _ = this.update(cx, |d, _| {
                            d.state = OAuthState::Error {
                                message: "State 不匹配".to_string(),
                            };
                        });
                        return;
                    }

                    let _ = this.update(cx, |d, _| {
                        d.state = OAuthState::Exchanging;
                    });

                    let code = callback.code;
                    let token_result = exchange_onedrive_token(
                        http.clone(),
                        &client_id,
                        &code,
                        "http://localhost:8787/oauth/callback",
                    )
                    .await;

                    let _ = this.update(cx, |d, cx| match token_result {
                        Ok(tokens) => {
                            let settings = AppSettings::global_mut(cx);
                            let od = settings
                                .onedrive_config
                                .get_or_insert_with(OneDriveSettings::default);
                            od.client_id = client_id;
                            od.tokens = Some(tokens);
                            settings.save();
                            cx.notify();
                            d.state = OAuthState::Success;
                        }
                        Err(e) => {
                            d.state = OAuthState::Error { message: e };
                        }
                    });
                }
                Err(e) => {
                    let _ = this.update(cx, |d, _| {
                        d.state = OAuthState::Error {
                            message: format!("启动回调服务器失败: {}", e),
                        };
                    });
                }
            }
        });

        self._auth_task = Some(task);
    }
}

impl Render for OneDriveAuthDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        match &self.state {
            OAuthState::Building => {
                self.start_auth(cx.entity(), cx);
                v_flex()
                    .gap_4()
                    .p_5()
                    .w(px(420.))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("正在启动授权流程..."),
                    )
                    .into_any_element()
            }
            OAuthState::WaitingForAuth { auth_url } => {
                let url = auth_url.clone();
                v_flex()
                    .gap_4()
                    .p_5()
                    .w(px(420.))
                    .child(div().text_sm().font_semibold().child("OneDrive 授权"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("点击下方按钮在浏览器中打开授权页面："),
                    )
                    .child(
                        h_flex().gap_2().child(
                            Button::new("open-url")
                                .with_variant(ButtonVariant::Ghost)
                                .on_click(move |_, _, cx: &mut App| {
                                    cx.open_url(&url);
                                })
                                .child("打开授权页面"),
                        ),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("授权完成后此窗口将自动确认。"),
                    )
                    .child(
                        h_flex().justify_end().gap_2().mt_4().child(
                            Button::new("cancel-btn")
                                .with_variant(ButtonVariant::Ghost)
                                .on_click(|_, window, cx: &mut App| {
                                    window.close_dialog(cx);
                                })
                                .child("取消"),
                        ),
                    )
                    .into_any_element()
            }
            OAuthState::Exchanging => v_flex()
                .gap_4()
                .p_5()
                .w(px(420.))
                .child(div().text_sm().font_semibold().child("正在获取访问权限..."))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("完成授权后正在交换访问令牌..."),
                )
                .into_any_element(),
            OAuthState::Success => v_flex()
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
                        .child("OneDrive 已成功连接。"),
                )
                .child(
                    h_flex().justify_end().gap_2().mt_4().child(
                        Button::new("success-confirm-btn")
                            .with_variant(ButtonVariant::Primary)
                            .on_click(|_, window, cx: &mut App| {
                                window.close_dialog(cx);
                            })
                            .child("确认"),
                    ),
                )
                .into_any_element(),
            OAuthState::Error { message } => {
                let entity = cx.entity();
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
                            .child(message.clone()),
                    )
                    .child(
                        h_flex()
                            .justify_end()
                            .gap_2()
                            .mt_4()
                            .child(
                                Button::new("error-cancel-btn")
                                    .with_variant(ButtonVariant::Ghost)
                                    .on_click(|_, window, cx: &mut App| {
                                        window.close_dialog(cx);
                                    })
                                    .child("关闭"),
                            )
                            .child(
                                Button::new("retry-btn")
                                    .with_variant(ButtonVariant::Primary)
                                    .on_click(move |_, _, cx: &mut App| {
                                        entity.update(cx, |d, cx| {
                                            d.state = OAuthState::Building;
                                            d.start_auth(cx.entity(), cx);
                                        });
                                    })
                                    .child("重试"),
                            ),
                    )
                    .into_any_element()
            }
        }
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 简单的 URL 参数编码（替换特殊字符）
fn encode_param(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3 / 2);
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

async fn exchange_google_token(
    http: Arc<dyn HttpClient>,
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<one_core::cloud_sync::oauth::OAuthTokens, String> {
    use futures::AsyncReadExt;
    use gpui::http_client::{AsyncBody, Method, Request};

    let body = format!(
        "grant_type=authorization_code\
         &client_id={}\
         &code={}\
         &redirect_uri={}\
         &client_secret={}",
        encode_param(client_id),
        encode_param(code),
        encode_param(redirect_uri),
        encode_param(client_secret),
    );

    let req = Request::builder()
        .method(Method::POST)
        .uri("https://oauth2.googleapis.com/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(AsyncBody::from(body.into_bytes()))
        .map_err(|e| format!("请求构建失败: {}", e))?;

    let response = http
        .send(req)
        .await
        .map_err(|e| format!("网络请求失败: {}", e))?;

    let status = response.status();
    let mut bytes = Vec::new();
    response
        .into_body()
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "Token 请求失败: {}",
            String::from_utf8_lossy(&bytes)
        ));
    }

    #[derive(serde::Deserialize)]
    struct GoogleTokenResp {
        access_token: String,
        #[serde(default)]
        refresh_token: Option<String>,
        expires_in: u64,
        token_type: String,
    }

    let resp: GoogleTokenResp =
        serde_json::from_slice(&bytes).map_err(|e| format!("解析响应失败: {}", e))?;

    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + resp.expires_in as i64;

    Ok(one_core::cloud_sync::oauth::OAuthTokens {
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
        token_type: resp.token_type,
        expires_in: Some(resp.expires_in),
        expires_at: Some(expires_at),
    })
}

async fn exchange_onedrive_token(
    http: Arc<dyn HttpClient>,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<one_core::cloud_sync::oauth::OAuthTokens, String> {
    use futures::AsyncReadExt;
    use gpui::http_client::{AsyncBody, Method, Request};

    let body = format!(
        "grant_type=authorization_code\
         &client_id={}\
         &code={}\
         &redirect_uri={}",
        encode_param(client_id),
        encode_param(code),
        encode_param(redirect_uri),
    );

    let req = Request::builder()
        .method(Method::POST)
        .uri("https://login.microsoftonline.com/common/oauth2/v2.0/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(AsyncBody::from(body.into_bytes()))
        .map_err(|e| format!("请求构建失败: {}", e))?;

    let response = http
        .send(req)
        .await
        .map_err(|e| format!("网络请求失败: {}", e))?;

    let status = response.status();
    let mut bytes = Vec::new();
    response
        .into_body()
        .read_to_end(&mut bytes)
        .await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "Token 请求失败: {}",
            String::from_utf8_lossy(&bytes)
        ));
    }

    #[derive(serde::Deserialize)]
    struct MsTokenResp {
        access_token: String,
        #[serde(default)]
        refresh_token: Option<String>,
        expires_in: u64,
        token_type: String,
    }

    let resp: MsTokenResp =
        serde_json::from_slice(&bytes).map_err(|e| format!("解析响应失败: {}", e))?;

    let expires_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
        + resp.expires_in as i64;

    Ok(one_core::cloud_sync::oauth::OAuthTokens {
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
        token_type: resp.token_type,
        expires_in: Some(resp.expires_in),
        expires_at: Some(expires_at),
    })
}

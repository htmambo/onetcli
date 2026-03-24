//! 用户认证模块
//!
//! 提供云端认证集成，包括登录、登出、会话持久化等功能。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::http_client::HttpClient;
use gpui::prelude::FluentBuilder;
use gpui::{App, AppContext as _, Context, Entity, FontWeight, ParentElement, Styled, Window, px};

/// 全局静态会话过期标志
///
/// 用于在客户端回调中（无法访问 GPUI 全局状态时）通知 UI 层会话已过期。
/// UI 层定期检查此标志，若为 true 则弹出登录对话框。
static SESSION_EXPIRED: AtomicBool = AtomicBool::new(false);
use gpui_component::button::Button;
use gpui_component::dialog::DialogButtonProps;
use gpui_component::{
    ActiveTheme, Sizable, WindowExt,
    input::{Input, InputState},
    v_flex,
};
use one_core::cloud_sync::{
    AuthResponse, CloudApiClient, CloudApiError, SessionExpiredCallback, UserInfo,
    sync_server::{SyncServerClient, SyncServerConfig as SyncServerClientConfig},
};
use rust_i18n::t;
use tracing::{info, warn};

use crate::setting_tab::AppSettings;

// ============================================================================
// 全局认证服务
// ============================================================================

/// 全局认证服务包装器
#[derive(Clone)]
pub struct GlobalAuthService(pub Arc<AuthService>);

impl gpui::Global for GlobalAuthService {}

/// 初始化全局认证服务
pub fn init(cx: &mut App) {
    let http = cx.http_client();
    let service = Arc::new(AuthService::new_with_http(http, cx));
    cx.set_global(GlobalAuthService(service));
}

/// 获取全局认证服务
pub fn get_auth_service(cx: &App) -> Arc<AuthService> {
    cx.global::<GlobalAuthService>().0.clone()
}

/// 检查会话是否已过期（并重置标志）
///
/// UI 层在 render 或定时器中调用此方法检测会话过期。
/// 返回 true 表示会话已过期，需要弹出登录对话框。
pub fn check_and_reset_session_expired() -> bool {
    let expired = SESSION_EXPIRED.swap(false, Ordering::SeqCst);
    if expired {
        warn!("检测到会话过期标志，准备弹出登录对话框");
    }
    expired
}

// ============================================================================
// 认证服务
// ============================================================================

/// 认证服务，管理云端客户端和用户状态
pub struct AuthService {
    cloud_client: Arc<SyncServerClient>,
}

impl AuthService {
    /// 获取云端 API 客户端
    ///
    /// 用于访问云端数据同步功能。
    pub fn cloud_client(&self) -> Arc<dyn CloudApiClient> {
        self.cloud_client.clone()
    }

    /// 使用配置和 HttpClient 创建认证服务
    fn new_with_http(http: Arc<dyn HttpClient>, _cx: &App) -> Self {
        let settings = AppSettings::load();
        let sync_server_base_url = SyncServerClient::normalize_base_url(&settings.sync_server_url);
        if SyncServerClient::is_valid_base_url(&sync_server_base_url) {
            info!("认证服务使用 sync_server 后端: {}", sync_server_base_url);
        } else {
            warn!("未配置有效的 sync_server 地址，认证和同步请求可能失败");
        }
        let cloud_client = Arc::new(SyncServerClient::new(
            SyncServerClientConfig {
                base_url: sync_server_base_url,
            },
            http,
        ));

        // 设置会话过期回调：刷新 token 失败时通过静态标志通知 UI
        let callback: SessionExpiredCallback = Arc::new(|| {
            tracing::warn!("会话已过期，需要重新登录");
            SESSION_EXPIRED.store(true, Ordering::SeqCst);
        });
        cloud_client.set_session_expired_callback(callback);

        // 设置 token 刷新回调：确保自动刷新后的最新 refresh token 能落盘持久化
        cloud_client.set_token_refreshed_callback(Arc::new(|auth_resp| {
            save_auth_data(
                &auth_resp.access_token,
                &auth_resp.refresh_token,
                &auth_resp.user_id,
                auth_resp.expires_at,
            );
            info!(
                "自动刷新令牌已持久化: user_id={} expires_at={}",
                auth_resp.user_id, auth_resp.expires_at
            );
        }));

        Self { cloud_client }
    }

    /// 检查是否已配置有效的 sync_server 地址。
    pub fn has_valid_sync_server_url(&self) -> bool {
        self.cloud_client.has_valid_base_url()
    }

    /// 返回“必须先配置同步地址”的提示文案。
    pub fn sync_server_url_required_message(&self) -> String {
        t!("Auth.sync_server_url_required").to_string()
    }

    /// 确保当前已配置有效的 sync_server 地址。
    pub fn ensure_sync_server_url_configured(&self) -> Result<(), String> {
        if self.has_valid_sync_server_url() {
            Ok(())
        } else {
            Err(self.sync_server_url_required_message())
        }
    }

    /// 更新 sync_server 地址，返回值表示是否发生变化。
    pub fn update_sync_server_url(&self, value: &str) -> bool {
        let normalized = SyncServerClient::normalize_base_url(value);
        let changed = self.cloud_client.set_base_url(&normalized);
        if !changed {
            return false;
        }

        self.cloud_client.clear_auth();
        clear_auth_data();

        if SyncServerClient::is_valid_base_url(&normalized) {
            info!("sync_server 地址已更新: {}", normalized);
        } else {
            warn!("sync_server 地址已更新，但当前值无效: {}", normalized);
        }

        true
    }

    /// 尝试恢复会话
    pub async fn try_restore_session(&self) -> Option<UserInfo> {
        if let Err(message) = self.ensure_sync_server_url_configured() {
            warn!("跳过恢复会话：{}", message);
            return None;
        }

        info!("开始尝试恢复会话");
        let auth_data = load_auth_data();
        let Some((access_token, refresh_token, user_id, expires_at)) = auth_data else {
            warn!("恢复会话失败: 本地无认证数据");
            return None;
        };
        info!(
            "已读取本地认证数据: user_id={} expires_at={}",
            user_id, expires_at
        );

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let needs_refresh = expires_at <= now + 60;
        info!(
            "令牌过期检查: now={} expires_at={} diff={}s needs_refresh={}",
            now,
            expires_at,
            expires_at - now,
            needs_refresh
        );

        let cloud_client = self.cloud_client();

        if needs_refresh {
            info!("访问令牌需要刷新: now={} expires_at={}", now, expires_at);
            const MAX_RETRIES: u32 = 3;
            let mut last_error = None;
            for attempt in 1..=MAX_RETRIES {
                match cloud_client.refresh_token(&refresh_token).await {
                    Ok(auth_resp) => {
                        save_auth_data(
                            &auth_resp.access_token,
                            &auth_resp.refresh_token,
                            &auth_resp.user_id,
                            auth_resp.expires_at,
                        );
                        info!(
                            "令牌刷新成功: user_id={} new_expires_at={}",
                            auth_resp.user_id, auth_resp.expires_at
                        );
                        last_error = None;
                        break;
                    }
                    Err(error) => {
                        if error.is_auth_error() {
                            warn!("令牌刷新认证失败，清除本地认证数据: {}", error);
                            self.cloud_client.clear_auth();
                            clear_auth_data();
                            return None;
                        }

                        warn!(
                            "令牌刷新失败（第 {}/{} 次），稍后重试: {}",
                            attempt, MAX_RETRIES, error
                        );
                        last_error = Some(error);
                        if attempt < MAX_RETRIES {
                            smol::Timer::after(std::time::Duration::from_secs(attempt as u64))
                                .await;
                        }
                    }
                }
            }

            if let Some(error) = last_error {
                warn!(
                    "令牌刷新重试耗尽，保留本地认证数据，本次跳过恢复会话: {}",
                    error
                );
                return None;
            }
        } else {
            self.cloud_client.set_auth_with_expiry(
                access_token,
                refresh_token.clone(),
                user_id,
                expires_at,
            );
            info!("访问令牌有效（剩余 {}s），已设置认证状态", expires_at - now);
        }

        match cloud_client.get_current_user().await {
            Ok(Some(user)) => {
                info!("恢复会话成功: user_id={} email={}", user.id, user.email);
                Some(user)
            }
            Ok(None) => {
                warn!("恢复会话失败: 用户信息为空，清除本地认证数据");
                self.cloud_client.clear_auth();
                clear_auth_data();
                None
            }
            Err(error) => {
                if error.is_auth_error() {
                    warn!("恢复会话失败: 认证错误，清除本地认证数据: {}", error);
                    self.cloud_client.clear_auth();
                    clear_auth_data();
                } else {
                    warn!("恢复会话失败: 获取用户信息错误（保留本地数据）: {}", error);
                }
                None
            }
        }
    }

    /// 登出
    pub async fn sign_out(&self) {
        info!("用户登出");
        let _ = self.cloud_client().sign_out().await;
        self.cloud_client.clear_auth();
        clear_auth_data();
    }

    async fn finish_auth(&self, auth_resp: AuthResponse) -> Result<UserInfo, String> {
        save_auth_data(
            &auth_resp.access_token,
            &auth_resp.refresh_token,
            &auth_resp.user_id,
            auth_resp.expires_at,
        );

        match self.cloud_client().get_current_user().await {
            Ok(Some(user)) => Ok(user),
            Ok(None) => Ok(UserInfo {
                id: auth_resp.user_id,
                email: auth_resp.email,
                username: None,
                avatar_url: None,
                created_at: 0,
            }),
            Err(error) => Err(error.to_string()),
        }
    }

    /// 使用邮箱密码登录
    pub async fn login_with_password(
        &self,
        email: &str,
        password: &str,
    ) -> Result<UserInfo, String> {
        self.ensure_sync_server_url_configured()?;
        let auth_resp = self
            .cloud_client()
            .sign_in_with_password(email, password)
            .await
            .map_err(|error| error.to_string())?;
        self.finish_auth(auth_resp).await
    }

    /// 使用邮箱密码注册
    pub async fn sign_up_with_password(
        &self,
        email: &str,
        password: &str,
    ) -> Result<UserInfo, String> {
        self.ensure_sync_server_url_configured()?;
        let auth_resp = self
            .cloud_client()
            .sign_up(email, password)
            .await
            .map_err(|error| match error {
                CloudApiError::EmailConfirmationRequired(email) => {
                    t!("Auth.email_confirmation_required", email = email).to_string()
                }
                other => other.to_string(),
            })?;
        self.finish_auth(auth_resp).await
    }
}

// ============================================================================
// 认证持久化
// ============================================================================

/// 获取认证数据存储路径
fn get_auth_file_path() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|p| p.join("one-hub").join("auth.json"))
}

/// 保存认证数据到本地
pub fn save_auth_data(access_token: &str, refresh_token: &str, user_id: &str, expires_at: i64) {
    if let Some(path) = get_auth_file_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let data = serde_json::json!({
            "access_token": access_token,
            "refresh_token": refresh_token,
            "user_id": user_id,
            "expires_at": expires_at,
        });

        if let Ok(json) = serde_json::to_string(&data) {
            match std::fs::write(&path, json) {
                Ok(()) => info!(
                    "认证数据已保存: user_id={} expires_at={} path={}",
                    user_id,
                    expires_at,
                    path.display()
                ),
                Err(error) => warn!("认证数据保存失败: {}", error),
            }
        }
    } else {
        warn!("无法获取认证数据存储路径");
    }
}

/// 从本地加载认证数据
pub fn load_auth_data() -> Option<(String, String, String, i64)> {
    let path = get_auth_file_path()?;
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                warn!("读取认证数据失败: {} path={}", error, path.display());
            }
            return None;
        }
    };
    let data: serde_json::Value = serde_json::from_str(&content).ok()?;

    let access_token = data.get("access_token")?.as_str()?.to_string();
    let refresh_token = data.get("refresh_token")?.as_str()?.to_string();
    let user_id = data.get("user_id")?.as_str()?.to_string();
    let expires_at = data.get("expires_at").and_then(|v| v.as_i64()).unwrap_or(0);

    info!(
        "加载本地认证数据: user_id={} expires_at={} token_len={} refresh_token_len={}",
        user_id,
        expires_at,
        access_token.len(),
        refresh_token.len()
    );

    Some((access_token, refresh_token, user_id, expires_at))
}

/// 清除本地认证数据
pub fn clear_auth_data() {
    if let Some(path) = get_auth_file_path() {
        match std::fs::remove_file(&path) {
            Ok(()) => info!("本地认证数据已清除: path={}", path.display()),
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    warn!("清除本地认证数据失败: {}", error);
                }
            }
        }
    }
}

// ============================================================================
// 登录/注册对话框
// ============================================================================

/// 密码认证动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordAuthAction {
    Login,
    SignUp,
}

/// 显示密码登录/注册对话框
pub fn show_password_auth_dialog<V: 'static>(
    window: &mut Window,
    cx: &mut Context<V>,
    view: Entity<V>,
    on_submit: impl Fn(&mut V, PasswordAuthAction, String, String, &mut Context<V>) + 'static,
) {
    let email_input =
        cx.new(|cx| InputState::new(window, cx).placeholder(t!("Auth.email_placeholder")));
    let password_input =
        cx.new(|cx| InputState::new(window, cx).placeholder(t!("Auth.password_placeholder")));
    let confirm_password_input = cx
        .new(|cx| InputState::new(window, cx).placeholder(t!("Auth.confirm_password_placeholder")));
    let error_message = cx.new(|_| Option::<String>::None);
    let sign_up_mode = cx.new(|_| false);

    let on_submit = std::rc::Rc::new(on_submit);

    let email_for_ok = email_input.clone();
    let password_for_ok = password_input.clone();
    let confirm_password_for_ok = confirm_password_input.clone();
    let error_for_ok = error_message.clone();
    let sign_up_mode_for_ok = sign_up_mode.clone();

    let email_for_render = email_input.clone();
    let password_for_render = password_input.clone();
    let confirm_password_for_render = confirm_password_input.clone();
    let error_for_render = error_message.clone();
    let sign_up_mode_for_render = sign_up_mode.clone();

    window.open_dialog(cx, move |dialog, _window, cx| {
        let is_sign_up = *sign_up_mode_for_render.read(cx);
        let submit_label = if is_sign_up {
            t!("Auth.sign_up").to_string()
        } else {
            t!("Auth.login").to_string()
        };
        let switch_label = if is_sign_up {
            t!("Auth.switch_to_login").to_string()
        } else {
            t!("Auth.switch_to_sign_up").to_string()
        };

        let view_clone = view.clone();
        let on_submit_ok = on_submit.clone();
        let email_ok = email_for_ok.clone();
        let password_ok = password_for_ok.clone();
        let confirm_password_ok = confirm_password_for_ok.clone();
        let error_ok = error_for_ok.clone();
        let sign_up_mode_ok = sign_up_mode_for_ok.clone();

        dialog
            .title(submit_label.clone())
            .width(px(400.))
            .confirm()
            .button_props(DialogButtonProps::default().ok_text(submit_label))
            .on_ok(move |_, _window, cx| {
                let email = email_ok.read(cx).text().to_string();
                let password = password_ok.read(cx).text().to_string();
                let confirm_password = confirm_password_ok.read(cx).text().to_string();
                let is_sign_up = *sign_up_mode_ok.read(cx);

                if email.is_empty() {
                    error_ok.update(cx, |msg, cx| {
                        *msg = Some(t!("Auth.email_required").to_string());
                        cx.notify();
                    });
                    return false;
                }

                if password.is_empty() {
                    error_ok.update(cx, |msg, cx| {
                        *msg = Some(t!("Auth.password_required").to_string());
                        cx.notify();
                    });
                    return false;
                }

                if is_sign_up && password != confirm_password {
                    error_ok.update(cx, |msg, cx| {
                        *msg = Some(t!("Auth.password_mismatch").to_string());
                        cx.notify();
                    });
                    return false;
                }

                let action = if is_sign_up {
                    PasswordAuthAction::SignUp
                } else {
                    PasswordAuthAction::Login
                };

                view_clone.update(cx, |this, cx| {
                    on_submit_ok(this, action, email, password, cx);
                });
                true
            })
            .child(
                v_flex()
                    .gap_4()
                    .p_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                gpui::div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(t!("Auth.email").to_string()),
                            )
                            .child(Input::new(&email_for_render)),
                    )
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                gpui::div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(t!("Auth.password").to_string()),
                            )
                            .child(Input::new(&password_for_render)),
                    )
                    .when(is_sign_up, |this| {
                        this.child(
                            v_flex()
                                .gap_1()
                                .child(
                                    gpui::div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(t!("Auth.confirm_password").to_string()),
                                )
                                .child(Input::new(&confirm_password_for_render)),
                        )
                    })
                    .child({
                        let sign_up_mode_toggle = sign_up_mode_for_render.clone();
                        let error_toggle = error_for_render.clone();
                        Button::new("switch-auth-mode")
                            .xsmall()
                            .label(switch_label)
                            .on_click(move |_, _window, cx| {
                                sign_up_mode_toggle.update(cx, |mode, cx| {
                                    *mode = !*mode;
                                    cx.notify();
                                });
                                error_toggle.update(cx, |msg, cx| {
                                    *msg = None;
                                    cx.notify();
                                });
                            })
                    })
                    .when_some(error_for_render.read(cx).clone(), |this, msg| {
                        this.child(
                            gpui::div()
                                .text_sm()
                                .text_color(cx.theme().danger)
                                .child(msg),
                        )
                    }),
            )
    });
}

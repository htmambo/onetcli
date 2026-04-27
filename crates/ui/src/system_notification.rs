//! 跨平台系统级通知
//!
//! 通过各平台的原生通知系统发送通知，不依赖窗口可见状态：
//! - Linux/macOS: notify-rust（libnotify / UserNotifications）
//! - Windows: 暂不支持（可后续扩展）

use gpui::App;

/// 系统通知选项
#[derive(Debug, Clone)]
pub struct SystemNotificationOptions {
    /// 通知唯一标识（用于去重）
    pub id: String,
    /// 通知标题
    pub title: String,
    /// 通知正文
    pub body: String,
    #[allow(dead_code)]
    _priv: (),
}

impl SystemNotificationOptions {
    /// 创建带标题和正文的通知
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: String::new(),
            title: title.into(),
            body: body.into(),
            _priv: (),
        }
    }

    /// 创建带标题、正文和 ID 的通知
    pub fn with_id(
        title: impl Into<String>,
        body: impl Into<String>,
        id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            body: body.into(),
            _priv: (),
        }
    }

    /// 设置通知 ID
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
}

/// 发送系统通知（跨平台）
///
/// 通过各平台原生通知系统发送，不依赖窗口可见状态。
pub fn show_system_notification(opts: SystemNotificationOptions, cx: &App) {
    let title = opts.title;
    let body = opts.body;
    let id = opts.id;
    let _ = id; // 用于后续去重扩展

    cx.background_executor()
        .spawn(async move {
            show_notification(&title, &body);
        })
        .detach();
}

fn show_notification(title: &str, body: &str) {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        let result = notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .show();

        if let Err(e) = result {
            tracing::warn!("系统通知发送失败: {}", e);
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (title, body);
    }
}

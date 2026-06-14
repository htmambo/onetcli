//! 全局 HTTP/HTTPS/SOCKS5 代理设置模型。
//!
//! 抽取自 `setting_tab.rs`（轮 5 重构）。`ProxyType` 与
//! `GlobalProxySettings` 通过父模块以 `pub(crate) use` 重导出，
//! 维持 `crate::setting_tab::*` 引用路径不变。

use gpui::http_client::Url;
use rust_i18n::t;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProxyType {
    Http,
    Https,
    #[default]
    Socks5,
}

impl ProxyType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProxyType::Http => "http",
            ProxyType::Https => "https",
            ProxyType::Socks5 => "socks5",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalProxySettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub proxy_type: ProxyType,
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_proxy_port")]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

fn default_proxy_port() -> u16 {
    1080
}

impl Default for GlobalProxySettings {
    fn default() -> Self {
        Self {
            enabled: false,
            proxy_type: ProxyType::default(),
            host: String::new(),
            port: default_proxy_port(),
            username: String::new(),
            password: String::new(),
        }
    }
}

impl GlobalProxySettings {
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        if self.host.trim().is_empty() {
            return Err(t!("Settings.proxy.validation_host_empty").to_string());
        }

        if self.port == 0 {
            return Err(t!("Settings.proxy.validation_port_empty").to_string());
        }

        if self.username.trim().is_empty() && !self.password.is_empty() {
            return Err(t!("Settings.proxy.validation_password_requires_username").to_string());
        }

        Ok(())
    }

    pub fn to_proxy_url(&self) -> Result<Option<Url>, String> {
        if !self.enabled {
            return Ok(None);
        }

        self.validate()?;

        let base = format!(
            "{}://{}:{}",
            self.proxy_type.as_str(),
            self.host.trim(),
            self.port
        );
        let mut url = Url::parse(&base)
            .map_err(|err| t!("Settings.proxy.validation_url_format", error = err))?;

        if !self.username.trim().is_empty() {
            url.set_username(self.username.trim())
                .map_err(|_| t!("Settings.proxy.validation_username_format"))?;
        }

        if !self.password.is_empty() {
            url.set_password(Some(&self.password))
                .map_err(|_| t!("Settings.proxy.validation_password_format"))?;
        }

        Ok(Some(url))
    }
}

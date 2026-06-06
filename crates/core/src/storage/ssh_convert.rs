//! SSH 类型转换模块
//!
//! 提供 `one_core::storage::models` 中的存储类型到 `ssh` crate 中连接类型的统一转换，
//! 消除 terminal、sftp_view、ssh_form_window、db 等模块中的重复转换逻辑。
//!
//! 由于 `SshAuth`、`ProxyType` 等目标类型来自外部 `ssh` crate，
//! 无法实现 `From<StorageType> for ForeignType`（孤儿规则），
//! 因此使用独立函数 + `SshParams` 扩展方法的方式提供统一转换入口。

use ssh::{
    JumpServerConnectConfig, ProxyConnectConfig, ProxyType as SshProxyType, SshAuth,
    SshConnectConfig,
};
use std::time::Duration;

use super::models::{
    JumpServerConfig, ProxyConfig, ProxyType as StorageProxyType, SshAuthMethod, SshParams,
};

/// 将存储层的认证方式转换为 SSH 连接层的认证方式。
///
/// 注意：`SshAuthMethod::PrivateKey` 没有证书路径字段，
/// 转换后 `certificate_path` 默认为 `None`。
pub(crate) fn ssh_auth_from_method(method: SshAuthMethod) -> SshAuth {
    match method {
        SshAuthMethod::Password { password } => SshAuth::Password(password),
        SshAuthMethod::PrivateKey {
            ssh_private_key,
            passphrase,
        } => SshAuth::PrivateKey {
            key_content: ssh_private_key,
            passphrase,
            certificate_path: None,
        },
        SshAuthMethod::Agent => SshAuth::Agent,
        SshAuthMethod::AutoPublicKey => SshAuth::AutoPublicKey,
    }
}

/// 将存储层的代理类型转换为 SSH 连接层的代理类型。
pub(crate) fn ssh_proxy_type_from_storage(proxy_type: StorageProxyType) -> SshProxyType {
    match proxy_type {
        StorageProxyType::Socks5 => SshProxyType::Socks5,
        StorageProxyType::Http => SshProxyType::Http,
    }
}

/// 将存储层的跳板机配置转换为 SSH 连接层的跳板机配置。
pub(crate) fn ssh_jump_config_from_storage(jump: JumpServerConfig) -> JumpServerConnectConfig {
    JumpServerConnectConfig {
        host: jump.host,
        port: jump.port,
        username: jump.username,
        auth: ssh_auth_from_method(jump.auth_method),
    }
}

/// 将存储层的代理配置转换为 SSH 连接层的代理配置。
pub(crate) fn ssh_proxy_config_from_storage(proxy: ProxyConfig) -> ProxyConnectConfig {
    ProxyConnectConfig {
        proxy_type: ssh_proxy_type_from_storage(proxy.proxy_type),
        host: proxy.host,
        port: proxy.port,
        username: proxy.username,
        password: proxy.password,
    }
}

/// SSH 连接配置构建扩展方法。
///
/// 从 `SshParams` 一次性构建完整的 `SshConnectConfig`，
/// 包含认证、跳板机、代理、超时等所有字段的转换。
impl SshParams {
    /// 从存储层的 SSH 参数构建连接层的 SSH 配置。
    pub fn to_connect_config(&self) -> SshConnectConfig {
        self.to_connect_config_with_options(false)
    }

    /// 从存储层的 SSH 参数构建连接层的 SSH 配置，支持自定义选项。
    pub fn to_connect_config_with_options(&self, auto_accept_new_keys: bool) -> SshConnectConfig {
        SshConnectConfig {
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            auth: ssh_auth_from_method(self.auth_method.clone()),
            timeout: self.connect_timeout.map(Duration::from_secs),
            keepalive_interval: self.keepalive_interval.map(Duration::from_secs),
            keepalive_max: self.keepalive_max,
            enable_legacy_kex: self.enable_legacy_kex,
            jump_server: self.jump_server.clone().map(ssh_jump_config_from_storage),
            proxy: self.proxy.clone().map(ssh_proxy_config_from_storage),
            keyboard_interactive_responder: None,
            auto_accept_new_keys,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_auth_method_password_converts() {
        let method = SshAuthMethod::Password {
            password: "s3cret".to_string(),
        };
        let auth = ssh_auth_from_method(method);
        assert!(matches!(auth, SshAuth::Password(ref p) if p == "s3cret"));
    }

    #[test]
    fn ssh_auth_method_private_key_converts() {
        let method = SshAuthMethod::PrivateKey {
            ssh_private_key:
                "-----BEGIN OPENSSH PRIVATE KEY-----\ncontent\n-----END OPENSSH PRIVATE KEY-----"
                    .to_string(),
            passphrase: Some("pass123".to_string()),
        };
        let auth = ssh_auth_from_method(method);
        match auth {
            SshAuth::PrivateKey {
                key_content,
                passphrase,
                certificate_path,
            } => {
                assert!(key_content.contains("BEGIN OPENSSH PRIVATE KEY"));
                assert_eq!(passphrase, Some("pass123".to_string()));
                assert!(certificate_path.is_none());
            }
            _ => panic!("expected PrivateKey variant"),
        }
    }

    #[test]
    fn ssh_auth_method_agent_converts() {
        let method = SshAuthMethod::Agent;
        let auth = ssh_auth_from_method(method);
        assert!(matches!(auth, SshAuth::Agent));
    }

    #[test]
    fn ssh_auth_method_auto_public_key_converts() {
        let method = SshAuthMethod::AutoPublicKey;
        let auth = ssh_auth_from_method(method);
        assert!(matches!(auth, SshAuth::AutoPublicKey));
    }

    #[test]
    fn proxy_type_socks5_converts() {
        let pt = ssh_proxy_type_from_storage(StorageProxyType::Socks5);
        assert!(matches!(pt, SshProxyType::Socks5));
    }

    #[test]
    fn proxy_type_http_converts() {
        let pt = ssh_proxy_type_from_storage(StorageProxyType::Http);
        assert!(matches!(pt, SshProxyType::Http));
    }

    #[test]
    fn jump_server_config_converts() {
        let config = JumpServerConfig {
            host: "jump.example.com".to_string(),
            port: 2222,
            username: "jumpuser".to_string(),
            auth_method: SshAuthMethod::Agent,
        };
        let result = ssh_jump_config_from_storage(config);
        assert_eq!(result.host, "jump.example.com");
        assert_eq!(result.port, 2222);
        assert_eq!(result.username, "jumpuser");
        assert!(matches!(result.auth, SshAuth::Agent));
    }

    #[test]
    fn proxy_config_converts() {
        let config = ProxyConfig {
            proxy_type: StorageProxyType::Http,
            host: "proxy.example.com".to_string(),
            port: 8080,
            username: Some("proxyuser".to_string()),
            password: None,
        };
        let result = ssh_proxy_config_from_storage(config);
        assert!(matches!(result.proxy_type, SshProxyType::Http));
        assert_eq!(result.host, "proxy.example.com");
        assert_eq!(result.port, 8080);
        assert_eq!(result.username, Some("proxyuser".to_string()));
        assert!(result.password.is_none());
    }

    #[test]
    fn ssh_params_to_connect_config_full() {
        let params = SshParams {
            host: "host.example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            auth_method: SshAuthMethod::Password {
                password: "pass".to_string(),
            },
            credential_ref: None,
            connect_timeout: Some(30),
            keepalive_interval: Some(60),
            keepalive_max: Some(3),
            enable_legacy_kex: false,
            default_directory: None,
            init_script: None,
            disable_shell_integration: None,
            sftp_local_directory: None,
            sftp_remote_directory: None,
            jump_server: Some(JumpServerConfig {
                host: "jump.example.com".to_string(),
                port: 22,
                username: "jumpuser".to_string(),
                auth_method: SshAuthMethod::Agent,
            }),
            proxy: Some(ProxyConfig {
                proxy_type: StorageProxyType::Socks5,
                host: "proxy.example.com".to_string(),
                port: 1080,
                username: None,
                password: None,
            }),
        };

        let config = params.to_connect_config();
        assert_eq!(config.host, "host.example.com");
        assert_eq!(config.port, 22);
        assert_eq!(config.username, "user");
        assert!(matches!(config.auth, SshAuth::Password(ref p) if p == "pass"));
        assert_eq!(config.timeout, Some(Duration::from_secs(30)));
        assert_eq!(config.keepalive_interval, Some(Duration::from_secs(60)));
        assert_eq!(config.keepalive_max, Some(3));
        assert!(!config.enable_legacy_kex);
        assert!(config.jump_server.is_some());
        assert!(config.proxy.is_some());
    }

    #[test]
    fn ssh_params_to_connect_config_minimal() {
        let params = SshParams {
            host: "host.example.com".to_string(),
            port: 22,
            username: "user".to_string(),
            auth_method: SshAuthMethod::AutoPublicKey,
            credential_ref: None,
            connect_timeout: None,
            keepalive_interval: None,
            keepalive_max: None,
            enable_legacy_kex: false,
            default_directory: None,
            init_script: None,
            disable_shell_integration: None,
            sftp_local_directory: None,
            sftp_remote_directory: None,
            jump_server: None,
            proxy: None,
        };

        let config = params.to_connect_config();
        assert_eq!(config.host, "host.example.com");
        assert!(config.timeout.is_none());
        assert!(config.jump_server.is_none());
        assert!(config.proxy.is_none());
    }
}

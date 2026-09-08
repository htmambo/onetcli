// S4：russh 库内部使用 unsafe；本 crate 在 `ssh.rs` 内通过 russh FFI 调用
// 必须直接处理 `ChannelMsg` 联合类型（unsafe 必要）。
#![allow(unsafe_code)]

rust_i18n::i18n!("locales", fallback = "en");

mod dynamic_socks;
mod session_manager;
mod socks5;
mod ssh;

pub use dynamic_socks::{DynamicSocksConfig, DynamicSocksTunnel, start_dynamic_socks_forward};
pub use session_manager::SshSessionManager;
pub use ssh::{
    AuthFailureMessages, ChannelEvent, JumpServerConnectConfig, KeyboardInteractivePrompt,
    KeyboardInteractiveRequest, KeyboardInteractiveResponder, KeyboardInteractiveTarget,
    LocalPortForwardConfig, LocalPortForwardTunnel, ProxyConnectConfig, ProxyType, PtyConfig,
    RusshChannel, RusshClient, ShellIntegrationSetup, SshAuth, SshChannel, SshClient,
    SshConnectConfig, SshConnectionStage, authenticate_session,
    authenticate_session_with_fallbacks, authenticate_with_strategy, build_client_config, defaults,
    expand_auto_publickey_auth, format_connection_progress_message, start_local_port_forward,
    start_local_port_forward_with_config, take_key_change_fingerprints, verify_server_key,
};

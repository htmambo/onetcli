rust_i18n::i18n!("terminal", fallback = "en");

pub mod history;
#[cfg(unix)]
pub mod local_pty_client;
pub mod local_pty_host;
#[cfg(unix)]
pub mod local_pty_host_unix;
#[cfg(windows)]
pub mod local_pty_host_windows;
pub mod local_pty_protocol;
pub mod osc;
pub mod pty_backend;
pub mod serial_backend;
pub mod ssh_backend;
pub mod terminal;
pub mod types;

#[cfg(unix)]
pub use local_pty_client::{kill_detached_sessions, LocalPtyClient};

/// No-op on non-Unix platforms (Windows).
#[cfg(not(unix))]
pub fn kill_detached_sessions(_session_ids: Vec<String>) {}

#[cfg(unix)]
pub use local_pty_host::run_local_pty_host;
pub use local_pty_protocol::{LocalPtyHostEvent, LocalPtyHostRequest};
pub use pty_backend::{GpuiEventProxy, TerminalEvent};
pub use serial_backend::SerialBackend;
pub use ssh_backend::SshBackend;
pub use terminal::TerminalScrollProxy;
pub use types::{LocalConfig, TerminalBackend, TerminalCloseMode, TerminalSize};

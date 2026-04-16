pub mod local_pty_client;
pub mod local_pty_host;
pub mod local_pty_host_unix;
#[cfg(windows)]
pub mod local_pty_host_windows;
pub mod local_pty_protocol;
pub mod pty_backend;
pub mod serial_backend;
pub mod ssh_backend;
pub mod terminal;
pub mod types;

pub use pty_backend::{GpuiEventProxy, TerminalEvent};
pub use serial_backend::SerialBackend;
pub use ssh_backend::SshBackend;
pub use terminal::TerminalScrollProxy;
pub use local_pty_host::run_local_pty_host;
pub use types::{LocalConfig, TerminalBackend, TerminalCloseMode, TerminalSize};

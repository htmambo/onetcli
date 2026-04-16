use std::{env, path::PathBuf};

use serde::{Deserialize, Serialize};

/// 终端关闭模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalCloseMode {
    /// 直接终止 PTY / 子进程
    Kill,
    /// 仅断开连接，保留 PTY 会话供后续恢复
    Detach,
}

/// Terminal backend trait - abstracts local PTY and SSH backends
pub trait TerminalBackend: Send {
    fn write(&self, data: Vec<u8>);
    fn resize(&self, size: TerminalSize);
    fn close(&self, mode: TerminalCloseMode);
}

/// Local terminal configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalConfig {
    /// Shell command (default: system default shell)
    pub shell: Option<String>,
    /// Working directory
    pub working_dir: Option<String>,
    /// Environment variables
    pub env: Vec<(String, String)>,
    /// 临时文件路径，用于通过 PROMPT_COMMAND 追踪当前工作目录
    pub cwd_file: Option<String>,
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            shell: None,
            working_dir: default_working_dir(),
            env: default_env(),
            cwd_file: None,
        }
    }
}

fn default_working_dir() -> Option<String> {
    #[cfg(target_os = "windows")]
    let home = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME"));

    #[cfg(not(target_os = "windows"))]
    let home = env::var_os("HOME").or_else(|| env::var_os("USERPROFILE"));

    home.or_else(|| env::current_dir().ok().map(|path| path.into_os_string()))
        .map(|path| PathBuf::from(path).to_string_lossy().into_owned())
}

fn default_env() -> Vec<(String, String)> {
    let mut env_vars = Vec::with_capacity(if cfg!(target_os = "windows") { 2 } else { 7 });
    env_vars.push(("TERM".to_string(), "xterm-256color".to_string()));
    env_vars.push(("COLORTERM".to_string(), "truecolor".to_string()));

    #[cfg(not(target_os = "windows"))]
    {
        env_vars.push(("CLICOLOR".to_string(), "1".to_string()));
        env_vars.push(("CLICOLOR_FORCE".to_string(), "1".to_string()));
        env_vars.push((
            "LANG".to_string(),
            env::var("LANG").unwrap_or_else(|_| "zh_CN.UTF-8".to_string()),
        ));
    }

    #[cfg(target_os = "macos")]
    {
        // 避免继承外层 Terminal.app 会话，导致 zsh 将 OSC 序列写入 ~/.zsh_sessions。
        env_vars.push(("SHELL_SESSIONS_DISABLE".to_string(), "1".to_string()));
        env_vars.push(("TERM_SESSION_ID".to_string(), String::new()));
    }

    env_vars
}

/// Terminal dimensions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSize {
    pub rows: u16,
    pub cols: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::default_env;

    #[cfg(target_os = "macos")]
    #[test]
    fn default_env_disables_apple_shell_sessions() {
        let env_vars = default_env();

        assert!(env_vars
            .iter()
            .any(|(key, value)| key == "SHELL_SESSIONS_DISABLE" && value == "1"));
        assert!(env_vars
            .iter()
            .any(|(key, value)| key == "TERM_SESSION_ID" && value.is_empty()));
    }
}

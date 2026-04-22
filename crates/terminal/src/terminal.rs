//! Terminal 模型层
//!
//! 独立的 Terminal Entity，负责：
//! - PTY/SSH 后端通信
//! - 终端状态管理（Term grid、选择、滚动）
//! - 事件发送（Title、Bell、ChildExit 等）
//!
//! 与 TerminalView 分离，TerminalView 只负责视图逻辑。

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point as AlacPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::{Flags, LineLength};
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::tty::{self, Options as PtyOptions};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use futures::StreamExt;
use gpui::*;
use one_core::gpui_tokio::Tokio;
use one_core::storage::models::{
    ActiveConnections, ProxyType as StorageProxyType, SerialParams, SshAuthMethod, StoredConnection,
};
use std::cell::Cell;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::collections::HashSet;
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::time::interval;

#[cfg(any(test, target_os = "windows"))]
use std::env;
#[cfg(any(test, target_os = "windows"))]
use std::ffi::OsStr;
#[cfg(any(test, target_os = "windows"))]
use std::path::Path;
#[cfg(any(test, not(target_os = "linux")))]
use std::sync::atomic::{AtomicU64, Ordering};

use crate::history::{
    collect_history_search_results, collect_history_suggestions_with_cwd, parse_shell_history,
    push_rich_history_entry, HistoryEntry, ShellHistoryFormat, PERSISTED_HISTORY_LIMIT,
    SESSION_HISTORY_LIMIT,
};
#[cfg(unix)]
use crate::local_pty_client::{LocalPtyClient, LocalPtyClientBackend};
#[cfg(unix)]
use crate::local_pty_protocol::LocalPtyHostEvent;
use crate::pty_backend::{GpuiEventProxy, LocalPtyBackend};
#[cfg(unix)]
use anyhow::Context as _;
#[cfg(not(target_os = "windows"))]
use crate::shell_integration::embedded_shell_integration_script;

use crate::{
    LocalConfig, SerialBackend, SshBackend, TerminalBackend, TerminalCloseMode, TerminalEvent,
    TerminalSize,
};
use ssh::{ChannelEvent, RusshClient, SshChannel, SshClient};
pub use ssh::{
    JumpServerConnectConfig, ProxyConnectConfig, ProxyType, PtyConfig, SshAuth, SshConnectConfig,
    SshConnectionStage, SshSessionManager,
};

const DEFAULT_COLS: usize = 80;
const DEFAULT_ROWS: usize = 24;
pub const DEFAULT_RECOVERY_SCROLLBACK_LINES: usize = 2000;
pub const MAX_RECOVERY_SCROLLBACK_LINES: usize = 5000;
const HISTORY_RESTORED_BANNER: &str =
    "\r\n\r\n\x1b[30;47m * \x1b[0m\x1b[97;100m 历史记录已恢复 \x1b[0m\r\n\r\n";
const HISTORY_RESTORED_BANNER_COMPACT: &str = "*历史记录已恢复";

fn is_osc_palette_line(line: &str) -> bool {
    // OSC 4 调色板序列：\x1b]4;n;rgb:R/G/B
    // 检查整行是否以此序列开头（已 trim 过的行）
    // 场景：grid 中 OSC 4 序列可能嵌入在其他字符中间（如 "➜  ~ 4;0;rgb:14/09/19"），
    // 因为 \x1b 可能被渲染为空格或被截断，所以改用前缀匹配而非 contains
    // 格式：OSC 4;<index>;rgb:R/G/B，<index> 必须是数字
    line.starts_with("\x1b]4;")
        || (line.starts_with("4;")
            && line[2..].chars().next().map_or(false, |c| c.is_ascii_digit())
            && line.contains(";rgb:"))
}

fn is_history_restored_banner_line(line: &str) -> bool {
    let compact: String = line.chars().filter(|ch| !ch.is_whitespace()).collect();
    compact == HISTORY_RESTORED_BANNER_COMPACT
}

fn normalize_recovery_scrollback_lines(lines: usize) -> usize {
    lines.min(MAX_RECOVERY_SCROLLBACK_LINES)
}

/// 判断是否使用 hosted 本地 PTY 模式。
/// 通过环境变量 `ONETCLI_HOSTED_LOCAL_PTY` 控制，默认关闭（fallback 到旧实现）。
#[cfg(unix)]
fn use_hosted_local_pty() -> bool {
    std::env::var("ONETCLI_HOSTED_LOCAL_PTY").is_ok_and(|v| v == "1" || v == "true")
}

fn serialize_term_for_recovery(term: &Term<GpuiEventProxy>, max_lines: usize) -> Option<String> {
    let max_lines = normalize_recovery_scrollback_lines(max_lines);
    if max_lines == 0 || term.mode().contains(TermMode::ALT_SCREEN) {
        return None;
    }

    let history_size = term.history_size();
    let screen_lines = term.screen_lines();
    let columns = term.columns();
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for line_idx in 0..(history_size + screen_lines) {
        let grid_line = Line((line_idx as i32) - (history_size as i32));
        let row = &term.grid()[grid_line];
        let line_length = row.line_length();

        if line_length.0 > 0 {
            for cell in row[..line_length].iter() {
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }

                current_line.push(cell.c);
                if let Some(zerowidth) = cell.zerowidth() {
                    current_line.extend(zerowidth.iter().copied());
                }
            }
        }

        let is_wrapline = columns > 0 && row[Column(columns - 1)].flags.contains(Flags::WRAPLINE);
        if is_wrapline {
            current_line.clear();
            continue;
        }
        // 清理一些不需要的内容
        if current_line.contains("type onetcli_prompt_hook")
            || current_line.contains("cd --")
            || current_line.contains("export PROMPT_COMMAND;")
            || (current_line.contains("PROMPT_COMMAND")
                && current_line.contains("onetcli_prompt_hook"))
        {
            current_line.clear();
            continue;
        }

        lines.push(current_line.trim_end_matches(' ').to_string());
        current_line.clear();
    }

    if !current_line.is_empty() {
        lines.push(current_line.trim_end_matches(' ').to_string());
    }

    // 过滤掉恢复 banner 和 OSC 调色板序列，避免恢复时被重新解析
    lines.retain(|s| {
        !s.is_empty() && !is_history_restored_banner_line(s) && !is_osc_palette_line(s)
    });

    while matches!(lines.last(), Some(last) if last.is_empty()) {
        lines.pop();
    }

    if lines.len() > max_lines {
        lines = lines.split_off(lines.len() - max_lines);
    }

    (!lines.is_empty()).then(|| lines.join("\r\n"))
}

fn replay_term_output(
    term: &Arc<FairMutex<Term<GpuiEventProxy>>>,
    data: &[u8],
    osc_tx: Option<&UnboundedSender<TerminalEvent>>,
) {
    let mut processor: Processor<StdSyncHandler> = Processor::new();
    processor.advance(&mut *term.lock(), data);

    // 从 PTY 输出中实时解析 OSC 7，触发路径更新
    if let Some(tx) = osc_tx {
        use crate::osc::extract_osc_events;
        for osc_event in extract_osc_events(data) {
            if let crate::osc::OscEvent::WorkingDirChanged(path) = osc_event {
                let _ = tx.send(TerminalEvent::WorkingDirChanged(path));
            }
        }
    }
}

fn apply_term_escape_sequence(term: &Arc<FairMutex<Term<GpuiEventProxy>>>, data: &[u8]) {
    replay_term_output(term, data, None);
}

fn normalize_working_dir(path: &str) -> Option<String> {
    let path = path.trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// 将路径中的 `~` 替换为实际的 home 目录路径。
pub fn expand_tilde(path: &str) -> String {
    let home = std::env::var("HOME").ok();
    if let Some(ref home) = home {
        if path == "~" {
            return home.clone();
        }
        if let Some(rest) = path.strip_prefix("~/") {
            return format!("{}/{}", home, rest);
        }
        if let Some(rest) = path.strip_prefix("~\\") {
            return format!("{}\\{}", home, rest);
        }
    }
    path.to_string()
}

/// Terminal 发出的事件，供 TerminalView 订阅
#[derive(Debug, Clone)]
pub enum TerminalModelEvent {
    /// 终端内容已更新，需要重新渲染
    Wakeup,
    /// shell 开始渲染新的 prompt（OSC 133;A）
    PromptStart,
    /// shell prompt 已渲染完成，用户可以输入（OSC 133;B）
    InputStart,
    /// 终端标题已更改
    TitleChanged(String),
    /// 终端响铃
    Bell,
    /// 子进程已退出
    ChildExit(i32),
    /// 终端程序请求存储到剪贴板
    ClipboardStore(String),
    /// 远程工作目录变更（OSC 7）
    WorkingDirChanged(String),
}

/// 终端连接状态
#[derive(Clone, PartialEq, Debug)]
pub enum ConnectionState {
    Connected,
    Connecting,
    Disconnected { error: Option<String> },
}

/// 终端连接类型
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TerminalConnectionKind {
    Local,
    Ssh,
    Serial,
}

/// SSH 终端配置
#[derive(Clone)]
pub struct SshTerminalConfig {
    pub ssh_config: SshConnectConfig,
    pub pty_config: PtyConfig,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SshProcessState {
    Unknown,
    Idle,
    Busy,
}

/// 将路径安全地转为 POSIX shell 单参数，避免命令注入。
pub(crate) fn shell_escape_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }

    let mut escaped = String::with_capacity(arg.len() + 2);
    escaped.push('\'');
    for ch in arg.chars() {
        if ch == '\'' {
            escaped.push_str("'\"'\"'");
        } else {
            escaped.push(ch);
        }
    }
    escaped.push('\'');
    escaped
}

fn build_cd_command(dir: &str) -> String {
    format!("cd -- {}", shell_escape_arg(dir))
}

const SSH_PROMPT_READY_COMMAND: &str = r#"printf "\033]1337;OnetcliPromptReady=1\007""#;
const SSH_PROMPT_HOOK_NAME: &str = "onetcli_prompt_hook";

fn compose_ssh_init_commands(
    base_init_commands: Option<&str>,
    _sync_path_with_terminal: bool,
) -> Option<String> {
    let mut commands = Vec::new();

    if let Some(base_commands) = base_init_commands.filter(|commands| !commands.is_empty()) {
        commands.push(base_commands.to_string());
    }

    commands.push(build_ssh_prompt_hook_command());

    (!commands.is_empty()).then(|| commands.join("\n"))
}

fn build_ssh_prompt_hook_command() -> String {
    format!(
        "\
type {hook_name} >/dev/null 2>&1 || {{ \
{hook_name}() {{ {hook_body}; }}; \
if [ -n \"$ZSH_VERSION\" ]; then \
typeset -ga precmd_functions; \
precmd_functions+=({hook_name}); \
else \
PROMPT_COMMAND='{hook_name}'${{PROMPT_COMMAND:+\";$PROMPT_COMMAND\"}}; export PROMPT_COMMAND; \
fi; \
}}",
        hook_name = SSH_PROMPT_HOOK_NAME,
        hook_body = SSH_PROMPT_READY_COMMAND,
    )
}

fn read_local_working_dir(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|contents| normalize_working_dir(&contents))
}

#[cfg(target_os = "linux")]
fn read_local_working_dir_from_pid(pid: u32) -> Option<String> {
    let proc_cwd = std::path::PathBuf::from(format!("/proc/{pid}/cwd"));
    std::fs::read_link(proc_cwd)
        .ok()
        .and_then(|path| normalize_working_dir(path.to_string_lossy().as_ref()))
}

#[cfg(target_os = "linux")]
fn parse_proc_children(contents: &str) -> Vec<u32> {
    contents
        .split_whitespace()
        .filter_map(|pid| pid.parse::<u32>().ok())
        .collect()
}

#[cfg(target_os = "linux")]
fn read_proc_children(proc_root: &std::path::Path, pid: u32) -> Vec<u32> {
    let children_path = proc_root
        .join(pid.to_string())
        .join("task")
        .join(pid.to_string())
        .join("children");
    std::fs::read_to_string(children_path)
        .map(|contents| parse_proc_children(&contents))
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn parse_proc_state(contents: &str) -> Option<char> {
    let (_, tail) = contents.rsplit_once(") ")?;
    tail.chars().next()
}

#[cfg(target_os = "linux")]
fn read_proc_state(proc_root: &std::path::Path, pid: u32) -> Option<char> {
    let stat_path = proc_root.join(pid.to_string()).join("stat");
    std::fs::read_to_string(stat_path)
        .ok()
        .and_then(|contents| parse_proc_state(&contents))
}

#[cfg(target_os = "linux")]
fn has_live_descendant_process(proc_root: &std::path::Path, pid: u32) -> bool {
    let mut seen = HashSet::new();
    let mut pending = read_proc_children(proc_root, pid);

    while let Some(child_pid) = pending.pop() {
        if !seen.insert(child_pid) {
            continue;
        }

        match read_proc_state(proc_root, child_pid) {
            Some('Z' | 'X') => {}
            Some(_) => return true,
            None => continue,
        }

        pending.extend(read_proc_children(proc_root, child_pid));
    }

    false
}

#[cfg(target_os = "macos")]
fn read_child_pids(pid: u32) -> Vec<u32> {
    let child_count =
        unsafe { libc::proc_listchildpids(pid as libc::pid_t, std::ptr::null_mut(), 0) };
    if child_count <= 0 {
        return Vec::new();
    }

    let mut children = vec![0 as libc::pid_t; child_count as usize];
    let loaded = unsafe {
        libc::proc_listchildpids(
            pid as libc::pid_t,
            children.as_mut_ptr().cast(),
            (children.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int,
        )
    };
    if loaded <= 0 {
        return Vec::new();
    }

    children.truncate(loaded as usize);
    children
        .into_iter()
        .filter_map(|child_pid| u32::try_from(child_pid).ok())
        .collect()
}

#[cfg(target_os = "macos")]
fn read_process_bsdinfo(pid: u32) -> Option<libc::proc_bsdinfo> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let loaded = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int,
        )
    };
    (loaded == std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int)
        .then(|| unsafe { info.assume_init() })
}

#[cfg(target_os = "macos")]
fn process_name_from_bsdinfo(info: &libc::proc_bsdinfo) -> Option<String> {
    let name = unsafe { std::ffi::CStr::from_ptr(info.pbi_name.as_ptr()) }
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string);

    name.or_else(|| {
        unsafe { std::ffi::CStr::from_ptr(info.pbi_comm.as_ptr()) }
            .to_str()
            .ok()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
    })
}

#[cfg(target_os = "macos")]
fn resolve_terminal_process_pid(
    pid: u32,
    process_name: Option<&str>,
    process_info_available: bool,
    children: &[u32],
) -> u32 {
    if children.len() == 1 && (!process_info_available || process_name == Some("login")) {
        return children[0];
    }
    pid
}

#[cfg(target_os = "macos")]
fn resolve_local_shell_pid(pid: u32) -> u32 {
    let deadline = Instant::now() + Duration::from_millis(150);
    loop {
        let process_info = read_process_bsdinfo(pid);
        let process_name = process_info.as_ref().and_then(process_name_from_bsdinfo);
        let process_name = process_name.as_deref().map(str::trim);
        let children = read_child_pids(pid);
        let resolved =
            resolve_terminal_process_pid(pid, process_name, process_info.is_some(), &children);
        if resolved != pid {
            return resolved;
        }

        if !children.is_empty() || Instant::now() >= deadline {
            return pid;
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "macos")]
fn has_live_descendant_process(pid: u32) -> bool {
    let mut seen = HashSet::new();
    let mut pending = read_child_pids(pid);

    while let Some(child_pid) = pending.pop() {
        if !seen.insert(child_pid) {
            continue;
        }

        match read_process_bsdinfo(child_pid) {
            Some(info) if info.pbi_status == libc::SZOMB => {}
            Some(_) => return true,
            None if bsdinfo_access_denied(child_pid) => return true,
            None => continue,
        }

        pending.extend(read_child_pids(child_pid));
    }

    false
}

#[cfg(target_os = "macos")]
fn bsdinfo_access_denied(pid: u32) -> bool {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let _ = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int,
        )
    };
    matches!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM | libc::EACCES)
    )
}

#[cfg(target_os = "macos")]
fn should_report_local_running_processes(
    startup_settled: bool,
    has_running_processes: bool,
) -> bool {
    startup_settled && has_running_processes
}

fn should_report_ssh_running_processes(
    is_connected: bool,
    ssh_process_state: SshProcessState,
    ssh_prompt_detected: bool,
    interactive_mode_active: bool,
    command_submitted_without_prompt_sync: bool,
) -> bool {
    is_connected
        && ((ssh_prompt_detected && ssh_process_state == SshProcessState::Busy)
            || (interactive_mode_active && command_submitted_without_prompt_sync))
}

fn has_ssh_interactive_program_mode(mode: TermMode) -> bool {
    mode.intersects(
        TermMode::ALT_SCREEN | TermMode::APP_CURSOR | TermMode::APP_KEYPAD | TermMode::MOUSE_MODE,
    )
}

#[cfg(target_os = "macos")]
fn note_local_user_input(
    connection_kind: TerminalConnectionKind,
    startup_settled: &Cell<bool>,
    data: &[u8],
) {
    if connection_kind == TerminalConnectionKind::Local && !data.is_empty() {
        startup_settled.set(true);
    }
}

#[cfg(not(target_os = "linux"))]
fn read_local_working_dir_from_pid(_pid: u32) -> Option<String> {
    None
}

#[cfg(any(test, not(target_os = "linux")))]
fn next_local_cwd_file_path() -> std::path::PathBuf {
    static NEXT_LOCAL_CWD_FILE_ID: AtomicU64 = AtomicU64::new(1);

    let file_id = NEXT_LOCAL_CWD_FILE_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "onetcli-cwd-{}-{}.txt",
        std::process::id(),
        file_id
    ))
}

#[cfg(not(target_os = "linux"))]
fn init_local_cwd_file(cwd_file: Option<String>) -> Option<PathBuf> {
    let cwd_file_path = cwd_file
        .map(PathBuf::from)
        .unwrap_or_else(next_local_cwd_file_path);
    let _ = std::fs::write(&cwd_file_path, "");
    Some(cwd_file_path)
}

#[cfg(target_os = "linux")]
fn init_local_cwd_file(cwd_file: Option<String>) -> Option<PathBuf> {
    let _ = cwd_file;
    None
}

#[cfg(test)]
fn build_local_cwd_tracking_init_command(cwd_file_path: &str) -> String {
    let mut command = format!(
        "ONETCLI_CWD_FILE={}; export ONETCLI_CWD_FILE; ",
        shell_escape_arg(cwd_file_path)
    );
    command.push_str("if [ -n \"$ZSH_VERSION\" ]; then ");
    command.push_str("onetcli_cwd_write() { pwd > \"$ONETCLI_CWD_FILE\" 2>/dev/null; }; ");
    command.push_str("typeset -ga precmd_functions; ");
    command.push_str(
        "case \" ${precmd_functions[*]} \" in *\" onetcli_cwd_write \"*) ;; *) precmd_functions+=(onetcli_cwd_write) ;; esac; ",
    );
    command.push_str("else ");
    command.push_str(
        "PROMPT_COMMAND='pwd > \"$ONETCLI_CWD_FILE\" 2>/dev/null'${PROMPT_COMMAND:+\";$PROMPT_COMMAND\"}; ",
    );
    command.push_str("export PROMPT_COMMAND; ");
    command.push_str("fi; ");
    command.push_str("pwd > \"$ONETCLI_CWD_FILE\" 2>/dev/null\n");
    command
}

fn note_ssh_user_input(
    connection_kind: TerminalConnectionKind,
    ssh_process_state: &Cell<SshProcessState>,
    ssh_command_submitted_without_prompt_sync: &Cell<bool>,
    data: &[u8],
) {
    if connection_kind != TerminalConnectionKind::Ssh || data.is_empty() {
        return;
    }

    let has_newline = data.iter().any(|byte| matches!(*byte, b'\r' | b'\n'));
    let should_mark_busy = matches!(ssh_process_state.get(), SshProcessState::Busy) || has_newline;
    if should_mark_busy {
        tracing::debug!(
            target: "terminal.ssh",
            has_newline,
            data_len = data.len(),
            data = %String::from_utf8_lossy(data).trim(),
            "SSH user input -> Busy"
        );
        ssh_process_state.set(SshProcessState::Busy);
        if has_newline {
            ssh_command_submitted_without_prompt_sync.set(true);
        }
    }
}

fn note_ssh_prompt_idle(
    connection_kind: TerminalConnectionKind,
    ssh_process_state: &Cell<SshProcessState>,
    ssh_command_submitted_without_prompt_sync: &Cell<bool>,
) {
    if connection_kind == TerminalConnectionKind::Ssh {
        tracing::debug!(
            target: "terminal.ssh",
            prev_state = ?ssh_process_state.get(),
            "SSH prompt idle -> Idle"
        );
        ssh_process_state.set(SshProcessState::Idle);
        ssh_command_submitted_without_prompt_sync.set(false);
    }
}

fn build_ssh_base_init_commands(
    working_dir: Option<&str>,
    default_directory: Option<&str>,
    init_script: Option<&str>,
) -> Option<String> {
    let mut commands = Vec::new();

    if let Some(work_dir) = working_dir {
        commands.push(build_cd_command(work_dir));
    } else {
        if let Some(dir) = default_directory.filter(|dir| !dir.is_empty()) {
            commands.push(build_cd_command(dir));
        }
        if let Some(script) = init_script.filter(|script| !script.is_empty()) {
            commands.push(script.to_string());
        }
    }

    (!commands.is_empty()).then(|| commands.join("\n"))
}

fn build_ssh_init_commands(
    working_dir: Option<&str>,
    default_directory: Option<&str>,
    init_script: Option<&str>,
    sync_path_with_terminal: bool,
) -> Option<String> {
    let base_init_commands =
        build_ssh_base_init_commands(working_dir, default_directory, init_script);
    compose_ssh_init_commands(base_init_commands.as_deref(), sync_path_with_terminal)
}

#[cfg(any(test, target_os = "windows"))]
fn path_if_file(path: impl Into<PathBuf>) -> Option<String> {
    let path = path.into();
    path.is_file().then(|| path.to_string_lossy().into_owned())
}

#[cfg(any(test, target_os = "windows"))]
fn find_executable_in_path(path_env: Option<&OsStr>, program: &str) -> Option<String> {
    let path_env = path_env?;
    env::split_paths(path_env)
        .map(|dir| dir.join(program))
        .find_map(path_if_file)
}

#[cfg(any(test, target_os = "windows"))]
fn resolve_default_windows_shell_from_env(
    path_env: Option<&OsStr>,
    system_root: Option<&OsStr>,
    comspec: Option<&OsStr>,
) -> String {
    if let Some(pwsh) = find_executable_in_path(path_env, "pwsh.exe") {
        return pwsh;
    }

    if let Some(system_root) = system_root {
        let powershell = Path::new(system_root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        if let Some(powershell) = path_if_file(powershell) {
            return powershell;
        }
    }

    if let Some(powershell) = find_executable_in_path(path_env, "powershell.exe") {
        return powershell;
    }

    if let Some(comspec) = comspec.and_then(path_if_file) {
        return comspec;
    }

    if let Some(system_root) = system_root {
        let cmd = Path::new(system_root).join("System32").join("cmd.exe");
        if let Some(cmd) = path_if_file(cmd) {
            return cmd;
        }
    }

    "cmd.exe".to_string()
}

#[cfg(target_os = "windows")]
fn build_local_shell(shell: Option<String>, _extra_args: Vec<String>) -> Option<tty::Shell> {
    let program = shell.unwrap_or_else(|| {
        resolve_default_windows_shell_from_env(
            env::var_os("PATH").as_deref(),
            env::var_os("SystemRoot")
                .or_else(|| env::var_os("SYSTEMROOT"))
                .as_deref(),
            env::var_os("COMSPEC").as_deref(),
        )
    });
    Some(tty::Shell::new(program, vec![]))
}

#[cfg(not(target_os = "windows"))]
fn build_local_shell(shell: Option<String>, extra_args: Vec<String>) -> Option<tty::Shell> {
    if extra_args.is_empty() {
        shell.map(|program| tty::Shell::new(program, vec![]))
    } else {
        // 有额外参数（如 --rcfile）时需显式指定 shell 程序
        let program = shell.or_else(|| std::env::var("SHELL").ok())?;
        Some(tty::Shell::new(program, extra_args))
    }
}

/// 准备本地终端的 Shell Integration 环境
///
/// 将 `shell_integration.sh` 写入进程级临时目录 `/tmp/onetcli-<pid>/`，
/// 仅对当前 OnetCli 进程内的终端会话生效，不污染全局配置。
/// 返回 `(额外环境变量, shell 额外参数)`。
#[cfg(not(target_os = "windows"))]
fn prepare_shell_integration(shell: Option<&str>) -> (Vec<(String, String)>, Vec<String>) {
    // 使用进程级临时目录，确保不影响其他会话或工具
    let session_dir = std::env::temp_dir().join(format!("onetcli-{}", std::process::id()));
    if fs::create_dir_all(&session_dir).is_err() {
        tracing::warn!(
            "无法创建临时目录 {}，跳过 Shell Integration",
            session_dir.display()
        );
        return (vec![], vec![]);
    }

    // 写入 shell_integration.sh（含交互式守卫，不影响 rsync/scp 等非交互通道）
    let integration_path = session_dir.join("shell_integration.sh");
    if let Err(e) = fs::write(&integration_path, embedded_shell_integration_script()) {
        tracing::warn!("写入 shell_integration.sh 失败: {e}");
        return (vec![], vec![]);
    }

    let mut extra_env: Vec<(String, String)> =
        vec![("ONETCLI_SHELL_INTEGRATION".into(), "1".into())];
    let mut extra_args: Vec<String> = Vec::new();

    // 判断 shell 类型：优先用显式参数，否则读 $SHELL
    let shell_name = shell
        .map(|s| s.to_ascii_lowercase())
        .or_else(|| std::env::var("SHELL").ok().map(|s| s.to_ascii_lowercase()))
        .unwrap_or_default();

    if shell_name.contains("zsh") {
        // zsh: 通过 ZDOTDIR 注入集成脚本
        let zsh_dir = session_dir.join("zsh");
        if fs::create_dir_all(&zsh_dir).is_err() {
            return (extra_env, extra_args);
        }

        let script = integration_path.display();
        let zshenv = format!(
            "ZDOTDIR=\"${{_ONETCLI_ORIG_ZDOTDIR:-$HOME}}\"\n\
             [[ -f \"$ZDOTDIR/.zshenv\" ]] && source \"$ZDOTDIR/.zshenv\"\n\
             [[ -f \"$HOME/.zshenv\" ]] && source \"$HOME/.zshenv\"\n\
              source \"{script}\"\n"
        );
        let _ = fs::write(zsh_dir.join(".zshenv"), zshenv);

        let orig = std::env::var("ZDOTDIR").unwrap_or_default();
        extra_env.push(("_ONETCLI_ORIG_ZDOTDIR".into(), orig));
        extra_env.push(("ZDOTDIR".into(), zsh_dir.display().to_string()));

        tracing::debug!(
            "已配置 zsh Shell Integration (ZDOTDIR={})",
            zsh_dir.display()
        );
    } else if shell_name.contains("bash") {
        // bash: 通过 --rcfile 注入集成脚本
        let bash_rc = session_dir.join("bash_integration.sh");
        let script = integration_path.display();
        let content = format!(
            "[[ -f \"$HOME/.bashrc\" ]] && source \"$HOME/.bashrc\"\n\
             source \"{script}\"\n"
        );
        let _ = fs::write(&bash_rc, content);

        extra_args.push("--rcfile".into());
        extra_args.push(bash_rc.display().to_string());

        tracing::debug!(
            "已配置 bash Shell Integration (--rcfile={})",
            bash_rc.display()
        );
    } else {
        tracing::debug!("未知 shell 类型 '{shell_name}'，跳过 Shell Integration 注入");
    }

    (extra_env, extra_args)
}

#[cfg(target_os = "windows")]
fn prepare_shell_integration(_shell: Option<&str>) -> (Vec<(String, String)>, Vec<String>) {
    // Windows 暂不支持 Shell Integration
    (vec![], vec![])
}

fn prepare_local_shell_launch(mut config: LocalConfig) -> (LocalConfig, Option<PathBuf>) {
    let local_cwd_file = init_local_cwd_file(config.cwd_file.clone());
    let (integration_env, shell_args) = prepare_shell_integration(config.shell.as_deref());
    config.env.extend(integration_env);
    config.shell_args.extend(shell_args);

    if let Some(path) = local_cwd_file.as_ref().and_then(|path| path.to_str()) {
        config
            .env
            .push(("ONETCLI_CWD_FILE".to_string(), path.to_string()));
    }

    (config, local_cwd_file)
}

fn history_file_candidates(preferred_shell: Option<&str>) -> Vec<(PathBuf, ShellHistoryFormat)> {
    let Some(home_dir) = dirs::home_dir() else {
        return Vec::new();
    };

    let mut candidates = Vec::new();
    let lower_shell = preferred_shell.unwrap_or_default().to_ascii_lowercase();
    let prefer_zsh = lower_shell.contains("zsh");

    let bash = (home_dir.join(".bash_history"), ShellHistoryFormat::Bash);
    let zsh = (home_dir.join(".zsh_history"), ShellHistoryFormat::Zsh);

    if prefer_zsh {
        candidates.push(bash);
        candidates.push(zsh);
    } else {
        candidates.push(zsh);
        candidates.push(bash);
    }

    candidates
}

fn load_local_history(preferred_shell: Option<&str>) -> Vec<String> {
    history_file_candidates(preferred_shell)
        .into_iter()
        .filter_map(|(path, format)| fs::read_to_string(path).ok().map(|text| (text, format)))
        .flat_map(|(text, format)| parse_shell_history(&text, format))
        .collect()
}

fn build_remote_history_load_command() -> String {
    [
        "sh -lc",
        "'",
        "if [ -f \"$HOME/.bash_history\" ]; then tail -n 512 \"$HOME/.bash_history\" 2>/dev/null || true; fi;",
        "printf \"\\n__ONETCLI_HISTORY_SPLIT__\\n\";",
        "if [ -f \"$HOME/.zsh_history\" ]; then tail -n 512 \"$HOME/.zsh_history\" 2>/dev/null || true; fi",
        "'",
    ]
    .join(" ")
}

fn parse_remote_history_output(output: &str) -> Vec<String> {
    let (bash_history, zsh_history) = output
        .split_once("\n__ONETCLI_HISTORY_SPLIT__\n")
        .unwrap_or((output, ""));

    let mut commands = parse_shell_history(bash_history, ShellHistoryFormat::Bash);
    commands.extend(parse_shell_history(zsh_history, ShellHistoryFormat::Zsh));
    commands
}

async fn load_ssh_history(config: SshConnectConfig) -> anyhow::Result<Vec<String>> {
    let mut client = RusshClient::connect(config).await?;
    let mut channel = client.open_channel().await?;
    let command = build_remote_history_load_command();
    channel.exec(&command).await?;

    let mut stdout = Vec::new();
    let mut exit_code = None;

    loop {
        match channel.recv().await {
            Some(ChannelEvent::Data(data)) => stdout.extend(data),
            Some(ChannelEvent::ExitStatus(code)) => exit_code = Some(code),
            Some(ChannelEvent::Eof) | Some(ChannelEvent::Close) | None => break,
            _ => {}
        }
    }

    let _ = channel.close().await;
    let _ = client.disconnect().await;

    if let Some(code) = exit_code {
        anyhow::ensure!(code == 0, "ssh history loader exited with status {code}");
    }

    Ok(parse_remote_history_output(&String::from_utf8_lossy(
        &stdout,
    )))
}

/// 终端模型 Entity
///
/// 负责管理终端的核心状态，包括：
/// - alacritty Term grid
/// - PTY/SSH 后端
/// - 连接状态
/// - 标题
pub struct Terminal {
    /// alacritty 终端状态
    term: Arc<FairMutex<Term<GpuiEventProxy>>>,
    /// PTY/SSH 后端
    backend: Option<Box<dyn TerminalBackend>>,

    /// 终端标题
    title: String,
    /// 当前工作目录（由 OSC 7 更新，仅 SSH 终端）
    current_working_dir: Option<String>,
    /// 本地 shell 子进程 PID（Linux 下可直接读取 /proc/<pid>/cwd）
    local_shell_pid: Option<u32>,
    /// 本地终端的工作目录跟踪文件
    local_cwd_file: Option<std::path::PathBuf>,
    /// macOS 本地 shell 启动阶段会短暂拉起辅助子进程；待首次稳定后再启用关闭拦截。
    #[cfg(target_os = "macos")]
    local_process_tree_settled: Cell<bool>,
    /// 子进程退出码
    child_exited: Option<i32>,
    /// 连接状态
    connection_state: ConnectionState,
    /// SSH 连接中的阶段提示文案
    connection_status_message: Option<String>,
    /// 连接阶段提示的起始时间，用于显示已等待时长
    connection_wait_started_at: Option<Instant>,

    /// 终端尺寸
    cols: usize,
    rows: usize,

    /// SSH 配置（用于重连）
    ssh_config: Option<SshTerminalConfig>,
    /// SSH 会话管理器（用于 FileManagerPanel 和 ServerMonitorPanel）
    ssh_session_manager: Option<Arc<SshSessionManager>>,
    /// SSH 会话的远端进程状态，由 prompt hook 与用户输入共同驱动。
    ssh_process_state: Cell<SshProcessState>,
    /// 是否已从远端收到过 OSC 133;A/B prompt 事件。
    /// 用于防御 shell integration 不工作时的永久 Busy 误报。
    ssh_prompt_detected: bool,
    /// 用户已提交命令，但会话尚未反馈“回到 prompt”。
    /// 用于补偿不支持 shell integration 的 SSH，会更保守地拦截关闭。
    ssh_command_submitted_without_prompt_sync: Cell<bool>,
    /// 串口参数（用于重连）
    serial_params: Option<SerialParams>,
    /// 事件发送器（用于 SSH 重连）
    event_tx: Option<UnboundedSender<TerminalEvent>>,
    /// 事件代理（用于设置 PtyWrite 回写通道）
    event_proxy: Option<GpuiEventProxy>,
    /// 连接 ID
    connection_id: Option<i64>,
    /// 连接名称
    connection_name: Option<String>,
    /// 初始化命令（连接成功后执行）
    init_commands: Option<String>,
    /// 当前 OnetCli 会话内记录的命令历史（富条目，含 frecency 元数据）
    session_history: VecDeque<HistoryEntry>,
    /// 从 shell 历史文件加载的持久化历史
    persisted_history: Vec<String>,
    /// 当前连接尝试代次，用于忽略过期的异步回调
    connection_generation: u64,

    /// 连接类型
    connection_kind: TerminalConnectionKind,
    /// Hosted 本地 PTY 的 session id（供恢复使用）
    local_pty_session_id: Option<String>,
}

#[derive(Clone)]
pub struct TerminalScrollProxy {
    term: Arc<FairMutex<Term<GpuiEventProxy>>>,
    event_tx: Option<UnboundedSender<TerminalEvent>>,
}

/// Snapshot of terminal scroll state, captured in a single lock acquisition
/// to ensure consistency.
#[derive(Clone, Debug)]
pub struct TerminalScrollSnapshot {
    pub display_offset: usize,
    pub history_size: usize,
    pub screen_lines: usize,
    pub columns: usize,
}

/// Snapshot of terminal render state, captured in a single lock acquisition
/// to avoid multiple independent locks that cause inconsistency and overhead.
#[derive(Clone, Debug)]
pub struct TerminalRenderSnapshot {
    pub has_selection: bool,
    pub selection_text: Option<String>,
    pub mode: TermMode,
    pub history_size: usize,
}

impl TerminalScrollProxy {
    /// Snapshot all scroll-related state in a single lock acquisition
    /// to avoid inconsistency from multiple separate locks.
    pub fn snapshot(&self) -> TerminalScrollSnapshot {
        let term = self.term.lock();
        TerminalScrollSnapshot {
            display_offset: term.grid().display_offset(),
            history_size: term.history_size(),
            screen_lines: term.screen_lines(),
            columns: term.columns(),
        }
    }

    pub fn display_offset(&self) -> usize {
        self.term.lock().grid().display_offset()
    }

    pub fn history_size(&self) -> usize {
        self.term.lock().history_size()
    }

    pub fn screen_lines(&self) -> usize {
        self.term.lock().screen_lines()
    }

    pub fn columns(&self) -> usize {
        self.term.lock().columns()
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    pub fn scroll_display_delta(&self, delta: i32) {
        if delta == 0 {
            return;
        }
        self.term
            .lock()
            .scroll_display(alacritty_terminal::grid::Scroll::Delta(delta));
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(TerminalEvent::Wakeup);
        }
    }
}

impl Terminal {
    fn new_local_disconnected(error: String, cx: &mut Context<Self>) -> Self {
        let (event_tx, event_rx) = unbounded_channel::<TerminalEvent>();
        let (term, _event_proxy, _colors) =
            Self::create_term(DEFAULT_COLS, DEFAULT_ROWS, event_tx.clone());

        Self::spawn_event_loop(event_rx, cx);

        Self {
            term,
            backend: None,
            title: String::new(),
            current_working_dir: None,
            local_shell_pid: None,
            local_cwd_file: None,
            #[cfg(target_os = "macos")]
            local_process_tree_settled: Cell::new(true),
            child_exited: None,
            connection_state: ConnectionState::Disconnected { error: Some(error) },
            connection_status_message: None,
            connection_wait_started_at: None,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            ssh_config: None,
            ssh_session_manager: None,
            ssh_process_state: Cell::new(SshProcessState::Unknown),
            ssh_prompt_detected: false,
            ssh_command_submitted_without_prompt_sync: Cell::new(false),
            serial_params: None,
            event_tx: Some(event_tx),
            event_proxy: None,
            connection_id: None,
            connection_name: None,
            init_commands: None,
            session_history: VecDeque::new(),
            persisted_history: Vec::new(),
            connection_generation: 0,
            connection_kind: TerminalConnectionKind::Local,
            local_pty_session_id: None,
        }
    }

    pub fn new_local_or_disconnected(
        config: LocalConfig,
        cx: &mut Context<Self>,
    ) -> (Self, Option<String>) {
        match Self::new_local_with_recovery(config, None, cx) {
            Ok(terminal) => (terminal, None),
            Err(error) => {
                let message = error.to_string();
                (
                    Self::new_local_disconnected(message.clone(), cx),
                    Some(message),
                )
            }
        }
    }

    pub fn new_local_with_recovery_or_disconnected(
        config: LocalConfig,
        recovery_content: Option<&str>,
        cx: &mut Context<Self>,
    ) -> (Self, Option<String>) {
        match Self::new_local_with_recovery(config, recovery_content, cx) {
            Ok(terminal) => (terminal, None),
            Err(error) => {
                let message = error.to_string();
                (
                    Self::new_local_disconnected(message.clone(), cx),
                    Some(message),
                )
            }
        }
    }

    /// 创建本地终端
    pub fn new_local(config: LocalConfig, cx: &mut Context<Self>) -> Result<Self> {
        Self::new_local_with_recovery(config, None, cx)
    }

    pub fn new_local_with_recovery(
        config: LocalConfig,
        recovery_content: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        #[cfg(unix)]
        if use_hosted_local_pty() {
            return Self::new_local_hosted(config, recovery_content, cx);
        }

        let (event_tx, event_rx) = unbounded_channel::<TerminalEvent>();
        let (term, event_proxy, _colors) =
            Self::create_term(DEFAULT_COLS, DEFAULT_ROWS, event_tx.clone());
        let (config, local_cwd_file) = prepare_local_shell_launch(config);
        let LocalConfig {
            shell,
            shell_args,
            working_dir,
            env,
            cwd_file: _,
        } = config;
        let history_shell = shell.clone();

        if let Some(content) = recovery_content.filter(|content| !content.trim().is_empty()) {
            replay_term_output(&term, content.as_bytes(), None);
            replay_term_output(&term, HISTORY_RESTORED_BANNER.as_bytes(), None);
        }

        let pty_options = PtyOptions {
            shell: build_local_shell(shell, shell_args),
            working_directory: working_dir.clone().map(Into::into),
            env: env.into_iter().collect(),
            drain_on_exit: true,
            #[cfg(target_os = "windows")]
            escape_args: true,
        };
        let local_backend = LocalPtyBackend::new(term.clone(), event_proxy, pty_options)?;
        let local_shell_pid = local_backend.child_pid();

        Self::spawn_event_loop(event_rx, cx);
        #[cfg(target_os = "macos")]
        Self::spawn_local_process_tree_settler(cx);
        Self::spawn_local_history_loader(history_shell.as_deref(), cx);

        Ok(Self {
            term,
            backend: Some(Box::new(local_backend)),
            title: String::new(),
            current_working_dir: working_dir,
            local_shell_pid,
            local_cwd_file,
            #[cfg(target_os = "macos")]
            local_process_tree_settled: Cell::new(false),
            child_exited: None,
            connection_state: ConnectionState::Connected,
            connection_status_message: None,
            connection_wait_started_at: None,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            ssh_config: None,
            ssh_session_manager: None,
            ssh_process_state: Cell::new(SshProcessState::Unknown),
            ssh_prompt_detected: false,
            ssh_command_submitted_without_prompt_sync: Cell::new(false),
            serial_params: None,
            event_tx: Some(event_tx),
            event_proxy: None,
            connection_id: None,
            connection_name: None,
            init_commands: None,
            session_history: VecDeque::new(),
            persisted_history: Vec::new(),
            connection_generation: 0,
            connection_kind: TerminalConnectionKind::Local,
            local_pty_session_id: None,
        })
    }

    #[cfg(unix)]
    fn new_local_hosted(
        config: LocalConfig,
        recovery_content: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let (event_tx, event_rx) = unbounded_channel::<TerminalEvent>();
        let (term, event_proxy, _colors) =
            Self::create_term(DEFAULT_COLS, DEFAULT_ROWS, event_tx.clone());
        let (config, local_cwd_file) = prepare_local_shell_launch(config);
        let history_shell = config.shell.clone();
        let initial_working_dir = config.working_dir.clone();

        if let Some(content) = recovery_content.filter(|content| !content.trim().is_empty()) {
            replay_term_output(&term, content.as_bytes(), None);
            replay_term_output(&term, HISTORY_RESTORED_BANNER.as_bytes(), None);
        }

        let mut client = LocalPtyClient::connect().context("连接 local-pty-host 失败")?;
        let size = TerminalSize {
            rows: DEFAULT_ROWS as u16,
            cols: DEFAULT_COLS as u16,
            pixel_width: 0,
            pixel_height: 0,
        };
        let (session_id, child_pid) = client
            .spawn_sync(config.clone(), size)
            .context("hosted local PTY spawn 失败")?;
        let (request_tx, mut host_event_rx) = client.split();

        let writeback_tx: UnboundedSender<crate::local_pty_protocol::LocalPtyHostRequest> =
            request_tx.clone();
        event_proxy.set_hosted_write_back(writeback_tx, session_id.clone());

        let term_for_host = term.clone();
        let host_event_tx = event_tx.clone();
        cx.spawn(async move |_, cx| {
            while let Some(event) = host_event_rx.recv().await {
                match event {
                    LocalPtyHostEvent::Output { data, .. } => {
                        replay_term_output(&term_for_host, &data, Some(&host_event_tx));
                        let _ = host_event_tx.send(TerminalEvent::Wakeup);
                    }
                    LocalPtyHostEvent::Exited { exit_code, .. } => {
                        let _ = host_event_tx.send(TerminalEvent::ChildExit(exit_code));
                    }
                    LocalPtyHostEvent::Error { message, .. } => {
                        tracing::error!("hosted local PTY error: {message}");
                    }
                    _ => {}
                }
            }
            cx.background_executor().spawn(async move {}).detach();
            Ok::<_, anyhow::Error>(())
        })
        .detach();

        let local_backend = LocalPtyClientBackend::new(request_tx, session_id.clone(), child_pid);
        let local_shell_pid = local_backend.child_pid();

        Self::spawn_event_loop(event_rx, cx);
        #[cfg(target_os = "macos")]
        Self::spawn_local_process_tree_settler(cx);
        Self::spawn_local_history_loader(history_shell.as_deref(), cx);

        Ok(Self {
            term,
            backend: Some(Box::new(local_backend)),
            title: String::new(),
            current_working_dir: initial_working_dir,
            local_shell_pid,
            local_cwd_file,
            #[cfg(target_os = "macos")]
            local_process_tree_settled: Cell::new(false),
            child_exited: None,
            connection_state: ConnectionState::Connected,
            connection_status_message: None,
            connection_wait_started_at: None,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            ssh_config: None,
            ssh_session_manager: None,
            ssh_process_state: Cell::new(SshProcessState::Unknown),
            ssh_prompt_detected: false,
            ssh_command_submitted_without_prompt_sync: Cell::new(false),
            serial_params: None,
            event_tx: Some(event_tx),
            event_proxy: Some(event_proxy),
            connection_id: None,
            connection_name: None,
            init_commands: None,
            session_history: VecDeque::new(),
            persisted_history: Vec::new(),
            connection_generation: 0,
            connection_kind: TerminalConnectionKind::Local,
            local_pty_session_id: Some(session_id),
        })
    }

    #[cfg(unix)]
    pub fn new_local_hosted_attach(
        config: LocalConfig,
        session_id: String,
        cx: &mut Context<Self>,
    ) -> Result<Self> {
        let (event_tx, event_rx) = unbounded_channel::<TerminalEvent>();
        let (term, event_proxy, _colors) =
            Self::create_term(DEFAULT_COLS, DEFAULT_ROWS, event_tx.clone());
        let history_shell = config.shell.clone();

        let mut client = LocalPtyClient::connect().context("连接 local-pty-host 失败")?;
        let size = TerminalSize {
            rows: DEFAULT_ROWS as u16,
            cols: DEFAULT_COLS as u16,
            pixel_width: 0,
            pixel_height: 0,
        };
        let child_pid = client
            .attach_sync(session_id.clone(), size)
            .context("hosted local PTY attach 失败")?;
        let (request_tx, mut host_event_rx) = client.split();

        let writeback_tx: UnboundedSender<crate::local_pty_protocol::LocalPtyHostRequest> =
            request_tx.clone();
        event_proxy.set_hosted_write_back(writeback_tx, session_id.clone());

        let term_for_host = term.clone();
        let host_event_tx = event_tx.clone();
        cx.spawn(async move |_, cx| {
            while let Some(event) = host_event_rx.recv().await {
                match event {
                    LocalPtyHostEvent::Output { data, .. } => {
                        replay_term_output(&term_for_host, &data, Some(&host_event_tx));
                        let _ = host_event_tx.send(TerminalEvent::Wakeup);
                    }
                    LocalPtyHostEvent::Exited { exit_code, .. } => {
                        let _ = host_event_tx.send(TerminalEvent::ChildExit(exit_code));
                    }
                    LocalPtyHostEvent::Error { message, .. } => {
                        tracing::error!("hosted local PTY error: {message}");
                    }
                    _ => {}
                }
            }
            cx.background_executor().spawn(async move {}).detach();
            Ok::<_, anyhow::Error>(())
        })
        .detach();

        let local_backend = LocalPtyClientBackend::new(request_tx, session_id.clone(), child_pid);
        let local_shell_pid = local_backend.child_pid();
        let local_cwd_file = init_local_cwd_file(config.cwd_file.clone());

        Self::spawn_event_loop(event_rx, cx);
        #[cfg(target_os = "macos")]
        Self::spawn_local_process_tree_settler(cx);
        Self::spawn_local_history_loader(history_shell.as_deref(), cx);

        Ok(Self {
            term,
            backend: Some(Box::new(local_backend)),
            title: String::new(),
            current_working_dir: config.working_dir,
            local_shell_pid,
            local_cwd_file,
            #[cfg(target_os = "macos")]
            local_process_tree_settled: Cell::new(false),
            child_exited: None,
            connection_state: ConnectionState::Connected,
            connection_status_message: None,
            connection_wait_started_at: None,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            ssh_config: None,
            ssh_session_manager: None,
            ssh_process_state: Cell::new(SshProcessState::Unknown),
            ssh_prompt_detected: false,
            ssh_command_submitted_without_prompt_sync: Cell::new(false),
            serial_params: None,
            event_tx: Some(event_tx),
            event_proxy: Some(event_proxy),
            connection_id: None,
            connection_name: None,
            init_commands: None,
            session_history: VecDeque::new(),
            persisted_history: Vec::new(),
            connection_generation: 0,
            connection_kind: TerminalConnectionKind::Local,
            local_pty_session_id: Some(session_id),
        })
    }

    /// 创建 SSH 终端
    pub fn new_ssh(
        conn: StoredConnection,
        cx: &mut Context<Self>,
        working_dir: Option<&str>,
        sync_path_with_terminal: bool,
    ) -> Self {
        Self::new_ssh_with_recovery(conn, cx, working_dir, sync_path_with_terminal, None)
    }

    pub fn new_ssh_with_recovery(
        conn: StoredConnection,
        cx: &mut Context<Self>,
        working_dir: Option<&str>,
        sync_path_with_terminal: bool,
        recovery_content: Option<&str>,
    ) -> Self {
        let ssh_params = conn
            .to_ssh_params()
            .expect("StoredConnection should contain valid SSH params");

        let auth = match ssh_params.auth_method.clone() {
            SshAuthMethod::Password { password } => SshAuth::Password(password),
            SshAuthMethod::PrivateKey {
                key_path,
                passphrase,
            } => SshAuth::PrivateKey {
                key_path,
                passphrase,
                certificate_path: None,
            },
            SshAuthMethod::Agent => SshAuth::Agent,
            SshAuthMethod::AutoPublicKey => SshAuth::AutoPublicKey,
        };

        let init_commands = build_ssh_init_commands(
            working_dir,
            ssh_params.default_directory.as_deref(),
            ssh_params.init_script.as_deref(),
            sync_path_with_terminal,
        );

        let ssh_config = SshConnectConfig {
            host: ssh_params.host,
            port: ssh_params.port,
            username: ssh_params.username,
            auth,
            timeout: ssh_params.connect_timeout.map(Duration::from_secs),
            keepalive_interval: ssh_params.keepalive_interval.map(Duration::from_secs),
            keepalive_max: ssh_params.keepalive_max,
            enable_legacy_kex: ssh_params.enable_legacy_kex,
            jump_server: ssh_params.jump_server.map(|jump| {
                let jump_auth = match jump.auth_method {
                    SshAuthMethod::Password { password } => SshAuth::Password(password),
                    SshAuthMethod::PrivateKey {
                        key_path,
                        passphrase,
                    } => SshAuth::PrivateKey {
                        key_path,
                        passphrase,
                        certificate_path: None,
                    },
                    SshAuthMethod::Agent => SshAuth::Agent,
                    SshAuthMethod::AutoPublicKey => SshAuth::AutoPublicKey,
                };
                JumpServerConnectConfig {
                    host: jump.host,
                    port: jump.port,
                    username: jump.username,
                    auth: jump_auth,
                }
            }),
            proxy: ssh_params.proxy.map(|p| {
                let proxy_type = match p.proxy_type {
                    StorageProxyType::Socks5 => ProxyType::Socks5,
                    StorageProxyType::Http => ProxyType::Http,
                };
                ProxyConnectConfig {
                    proxy_type,
                    host: p.host,
                    port: p.port,
                    username: p.username,
                    password: p.password,
                }
            }),
        };

        let pty_config = PtyConfig::default();
        let config = SshTerminalConfig {
            ssh_config,
            pty_config,
        };

        let cols = config.pty_config.width as usize;
        let rows = config.pty_config.height as usize;

        let (event_tx, event_rx) = unbounded_channel::<TerminalEvent>();
        let (term, event_proxy, _colors) = Self::create_term(cols, rows, event_tx.clone());
        let initial_working_dir = working_dir.map(str::to_string);

        if let Some(content) = recovery_content.filter(|content| !content.trim().is_empty()) {
            replay_term_output(&term, content.as_bytes(), None);
            replay_term_output(&term, HISTORY_RESTORED_BANNER.as_bytes(), None);
        }
        let (disconnect_tx, disconnect_rx) = tokio::sync::oneshot::channel::<()>();
        let connection_generation = 1;
        let ssh_session_manager = Arc::new(SshSessionManager::new(config.ssh_config.clone()));

        Self::spawn_disconnect_handler(disconnect_rx, connection_generation, cx);
        Self::spawn_event_loop(event_rx, cx);
        Self::spawn_ssh_connect(
            ssh_session_manager.clone(),
            config.clone(),
            term.clone(),
            event_proxy.clone(),
            event_tx.clone(),
            conn.id,
            Some(disconnect_tx),
            init_commands.clone(),
            connection_generation,
            cx,
        );
        Self::spawn_connection_status_tick(cx);
        Self::spawn_ssh_history_loader(config.ssh_config.clone(), cx);

        Self {
            term,
            backend: None,
            title: String::new(),
            current_working_dir: initial_working_dir,
            local_shell_pid: None,
            local_cwd_file: None,
            #[cfg(target_os = "macos")]
            local_process_tree_settled: Cell::new(true),
            child_exited: None,
            connection_state: ConnectionState::Connecting,
            connection_status_message: Some(
                SshConnectionStage::initial_for_config(&config.ssh_config).description(),
            ),
            connection_wait_started_at: Some(Instant::now()),
            cols,
            rows,
            ssh_config: Some(config.clone()),
            ssh_session_manager: Some(ssh_session_manager),
            ssh_process_state: Cell::new(SshProcessState::Unknown),
            ssh_prompt_detected: false,
            ssh_command_submitted_without_prompt_sync: Cell::new(false),
            serial_params: None,
            event_tx: Some(event_tx),
            event_proxy: Some(event_proxy),
            connection_id: conn.id,
            connection_name: Some(conn.name),
            init_commands,
            session_history: VecDeque::new(),
            persisted_history: Vec::new(),
            connection_generation,
            connection_kind: TerminalConnectionKind::Ssh,
            local_pty_session_id: None,
        }
    }

    /// 创建串口终端
    pub fn new_serial(conn: StoredConnection, cx: &mut Context<Self>) -> Self {
        let serial_params = conn
            .to_serial_params()
            .expect("StoredConnection 应包含有效的 SerialParams");

        let (event_tx, event_rx) = unbounded_channel::<TerminalEvent>();
        let (term, _event_proxy, _colors) =
            Self::create_term(DEFAULT_COLS, DEFAULT_ROWS, event_tx.clone());
        let (disconnect_tx, disconnect_rx) = tokio::sync::oneshot::channel::<()>();
        let connection_generation = 1;

        Self::spawn_disconnect_handler(disconnect_rx, connection_generation, cx);
        Self::spawn_event_loop(event_rx, cx);
        Self::spawn_serial_connect(
            serial_params.clone(),
            term.clone(),
            event_tx.clone(),
            Some(disconnect_tx),
            connection_generation,
            cx,
        );

        Self {
            term,
            backend: None,
            title: String::new(),
            current_working_dir: None,
            local_shell_pid: None,
            local_cwd_file: None,
            #[cfg(target_os = "macos")]
            local_process_tree_settled: Cell::new(true),
            child_exited: None,
            connection_state: ConnectionState::Connecting,
            connection_status_message: None,
            connection_wait_started_at: None,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
            ssh_config: None,
            ssh_session_manager: None,
            ssh_process_state: Cell::new(SshProcessState::Unknown),
            ssh_prompt_detected: false,
            ssh_command_submitted_without_prompt_sync: Cell::new(false),
            serial_params: Some(serial_params),
            event_tx: Some(event_tx),
            event_proxy: None,
            connection_id: conn.id,
            connection_name: Some(conn.name),
            init_commands: None,
            session_history: VecDeque::new(),
            persisted_history: Vec::new(),
            connection_generation,
            connection_kind: TerminalConnectionKind::Serial,
            local_pty_session_id: None,
        }
    }

    fn next_connection_generation(&mut self) -> u64 {
        self.connection_generation = self.connection_generation.wrapping_add(1).max(1);
        self.connection_generation
    }

    fn is_current_connection_generation(&self, generation: u64) -> bool {
        self.connection_generation == generation
    }

    fn create_term(
        cols: usize,
        rows: usize,
        event_tx: UnboundedSender<TerminalEvent>,
    ) -> (
        Arc<FairMutex<Term<GpuiEventProxy>>>,
        GpuiEventProxy,
        alacritty_terminal::term::color::Colors,
    ) {
        let term_config = TermConfig {
            scrolling_history: 10000,
            ..Default::default()
        };
        let event_proxy = GpuiEventProxy::new(event_tx);
        let term = Term::new(
            term_config,
            &TermDimensions { cols, rows },
            event_proxy.clone(),
        );
        let colors = term.colors().clone();
        (Arc::new(FairMutex::new(term)), event_proxy, colors)
    }

    fn spawn_local_history_loader(preferred_shell: Option<&str>, cx: &mut Context<Self>) {
        let preferred_shell = preferred_shell.map(str::to_string);
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let history = load_local_history(preferred_shell.as_deref());
            let _ = this.update(cx, |terminal, cx| {
                terminal.set_persisted_history(history, cx);
            });
        })
        .detach();
    }

    fn spawn_ssh_history_loader(config: SshConnectConfig, cx: &mut Context<Self>) {
        let task = Tokio::spawn(cx, async move { load_ssh_history(config).await });

        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let Ok(Ok(history)) = task.await else {
                return;
            };
            let _ = this.update(cx, |terminal, cx| {
                terminal.set_persisted_history(history, cx);
            });
        })
        .detach();
    }

    fn spawn_event_loop(mut event_rx: UnboundedReceiver<TerminalEvent>, cx: &mut Context<Self>) {
        let _entity = cx.entity().downgrade();
        let (render_tx, mut render_rx) = futures::channel::mpsc::unbounded::<TerminalEvent>();

        // 后台事件聚合任务 - 8ms 节流
        Tokio::spawn(cx, async move {
            let mut render_interval = interval(Duration::from_millis(8));
            let mut pending_wakeup = false;
            let mut pending_events: Vec<TerminalEvent> = Vec::new();

            loop {
                tokio::select! {
                    result = event_rx.recv() => {
                        match result {
                            None => break,
                            Some(event) => {
                                match &event {
                                    TerminalEvent::Wakeup => pending_wakeup = true,
                                    _ => pending_events.push(event),
                                }
                            }
                        }
                    }
                    _ = render_interval.tick() => {
                        // 先发送非 Wakeup 事件
                        for event in pending_events.drain(..) {
                            if render_tx.unbounded_send(event).is_err() {
                                return;
                            }
                        }
                        // 最后发送 Wakeup
                        if pending_wakeup {
                            pending_wakeup = false;
                            if render_tx.unbounded_send(TerminalEvent::Wakeup).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        })
        .detach();

        // GPUI 线程事件处理
        cx.spawn(async move |this, cx| {
            while let Some(event) = render_rx.next().await {
                if this
                    .update(cx, |this, cx| {
                        this.handle_terminal_event(event, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn spawn_disconnect_handler(
        disconnect_rx: tokio::sync::oneshot::Receiver<()>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _ = disconnect_rx.await;
            let _ = entity.update(cx, |this, cx| {
                if !this.is_current_connection_generation(generation) {
                    return;
                }
                this.connection_state = ConnectionState::Disconnected { error: None };
                this.connection_status_message = None;
                this.connection_wait_started_at = None;
                this.backend = None;
                this.child_exited = Some(0);
                this.reset_ssh_process_tracking();
                this.set_connection_active(false, cx);
                cx.emit(TerminalModelEvent::ChildExit(0));
                cx.emit(TerminalModelEvent::Wakeup);
            });
        })
        .detach();
    }

    fn spawn_connection_status_tick(cx: &mut Context<Self>) {
        let entity = cx.entity().downgrade();
        cx.spawn(async move |_, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let keep_running = entity
                .update(cx, |this, cx| {
                    if matches!(this.connection_state, ConnectionState::Connecting)
                        && this.connection_wait_started_at.is_some()
                    {
                        cx.emit(TerminalModelEvent::Wakeup);
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if !keep_running {
                break;
            }
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    fn spawn_local_process_tree_settler(cx: &mut Context<Self>) {
        let entity = cx.entity().downgrade();
        cx.spawn(async move |_, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;

            let keep_running = entity
                .update(cx, |this, _cx| {
                    if this.connection_kind != TerminalConnectionKind::Local {
                        return false;
                    }

                    if this.local_process_tree_settled.get() {
                        return false;
                    }

                    let Some(pid) = this.local_shell_pid.map(resolve_local_shell_pid) else {
                        this.local_process_tree_settled.set(true);
                        return false;
                    };

                    if !has_live_descendant_process(pid) {
                        this.local_process_tree_settled.set(true);
                        return false;
                    }

                    true
                })
                .unwrap_or(false);

            if !keep_running {
                break;
            }
        })
        .detach();
    }

    fn spawn_ssh_connect(
        session_manager: Arc<SshSessionManager>,
        config: SshTerminalConfig,
        term: Arc<FairMutex<Term<GpuiEventProxy>>>,
        event_proxy: GpuiEventProxy,
        event_tx: UnboundedSender<TerminalEvent>,
        connection_id: Option<i64>,
        on_disconnect: Option<tokio::sync::oneshot::Sender<()>>,
        init_commands: Option<String>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        // 创建 SSH 后端需要的通知通道
        let (notify_tx, mut notify_rx) = unbounded_channel::<()>();
        let (progress_tx, mut progress_rx) = unbounded_channel::<SshConnectionStage>();

        let task = Tokio::spawn(cx, async move {
            // 转发 SSH 通知到事件通道（必须在 tokio runtime 内部）
            let event_tx_clone = event_tx.clone();
            tokio::spawn(async move {
                while notify_rx.recv().await.is_some() {
                    let _ = event_tx_clone.send(TerminalEvent::Wakeup);
                }
            });

            let disconnect_tx = on_disconnect.map(|tx| {
                let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
                tokio::spawn(async move {
                    if receiver.await.is_ok() {
                        let _ = tx.send(());
                    }
                });
                sender
            });
            SshBackend::connect_with_progress(
                session_manager,
                config.pty_config,
                connection_id,
                term,
                event_proxy,
                event_tx,
                notify_tx,
                disconnect_tx,
                init_commands,
                move |stage| {
                    let _ = progress_tx.send(stage);
                },
            )
            .await
        });

        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            while let Some(stage) = progress_rx.recv().await {
                if this
                    .update(cx, |this, cx| {
                        if matches!(this.connection_state, ConnectionState::Connecting) {
                            this.connection_status_message = Some(stage.description());
                            cx.emit(TerminalModelEvent::Wakeup);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.handle_ssh_result(result, generation, cx);
            });
        })
        .detach();
    }

    fn handle_ssh_result(
        &mut self,
        result: Result<Result<SshBackend, anyhow::Error>, tokio::task::JoinError>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if !self.is_current_connection_generation(generation) {
            if let Ok(Ok(backend)) = result {
                backend.shutdown();
            }
            return;
        }

        match result {
            Ok(Ok(backend)) => {
                self.connection_state = ConnectionState::Connected;
                self.connection_status_message = None;
                self.connection_wait_started_at = None;
                // 后端任务与这里并发执行，prompt 事件可能先于连接成功回调到达。
                // 成功分支不能重置 SSH 跟踪状态，否则会把已收到的 Idle/prompt 信号抹掉，
                // 导致 top 等前台程序运行时无法正确拦截关闭。
                tracing::debug!(
                    target: "terminal.ssh",
                    ssh_process_state = ?self.ssh_process_state.get(),
                    ssh_prompt_detected = self.ssh_prompt_detected,
                    "SSH connected, preserving ssh process tracking state"
                );
                self.set_connection_active(true, cx);
                // 连接后重新调整终端大小
                self.term.lock().resize(TermDimensions {
                    cols: self.cols,
                    rows: self.rows,
                });
                tracing::info!(
                    "SSH 连接成功，同步终端尺寸到远程: {}x{}",
                    self.cols,
                    self.rows
                );
                backend.resize(TerminalSize {
                    rows: self.rows as u16,
                    cols: self.cols as u16,
                    pixel_width: 0,
                    pixel_height: 0,
                });
                self.backend = Some(Box::new(backend));
            }
            Ok(Err(e)) => {
                self.connection_state = ConnectionState::Disconnected {
                    error: Some(format_connection_error(&e)),
                };
                self.connection_status_message = None;
                self.connection_wait_started_at = None;
                self.reset_ssh_process_tracking();
                self.set_connection_active(false, cx);
            }
            Err(e) => {
                self.connection_state = ConnectionState::Disconnected {
                    error: Some(e.to_string()),
                };
                self.connection_status_message = None;
                self.connection_wait_started_at = None;
                self.reset_ssh_process_tracking();
                self.set_connection_active(false, cx);
            }
        }
        cx.emit(TerminalModelEvent::Wakeup);
    }

    fn spawn_serial_connect(
        params: SerialParams,
        term: Arc<FairMutex<Term<GpuiEventProxy>>>,
        event_tx: UnboundedSender<TerminalEvent>,
        on_disconnect: Option<tokio::sync::oneshot::Sender<()>>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let disconnect_tx = on_disconnect.map(|tx| {
            let (sender, receiver) = tokio::sync::oneshot::channel::<()>();
            Tokio::spawn(cx, async move {
                if receiver.await.is_ok() {
                    let _ = tx.send(());
                }
            })
            .detach();
            sender
        });

        let result = SerialBackend::connect(params, term, event_tx, disconnect_tx);

        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            let _ = this.update(cx, |this, cx| {
                this.handle_serial_result(result, generation, cx);
            });
        })
        .detach();
    }

    fn handle_serial_result(
        &mut self,
        result: anyhow::Result<SerialBackend>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        if !self.is_current_connection_generation(generation) {
            if let Ok(backend) = result {
                backend.shutdown();
            }
            return;
        }

        match result {
            Ok(backend) => {
                self.connection_state = ConnectionState::Connected;
                self.set_connection_active(true, cx);
                self.backend = Some(Box::new(backend));
                tracing::info!("串口连接成功");
            }
            Err(e) => {
                self.connection_state = ConnectionState::Disconnected {
                    error: Some(e.to_string()),
                };
                self.set_connection_active(false, cx);
            }
        }
        cx.emit(TerminalModelEvent::Wakeup);
    }

    fn set_connection_active(&self, active: bool, cx: &mut Context<Self>) {
        let Some(connection_id) = self.connection_id else {
            return;
        };

        let global_state = cx.global_mut::<ActiveConnections>();
        if active {
            global_state.add(connection_id);
        } else {
            global_state.remove(connection_id);
        }
    }

    fn record_history_entry(&mut self, command: &str, cx: &mut Context<Self>) {
        let entry =
            HistoryEntry::new(command.to_string()).with_cwd(self.current_working_dir.clone());
        if push_rich_history_entry(&mut self.session_history, entry, SESSION_HISTORY_LIMIT) {
            cx.emit(TerminalModelEvent::Wakeup);
        }
    }

    fn set_persisted_history(&mut self, history: Vec<String>, cx: &mut Context<Self>) {
        let history = history
            .into_iter()
            .filter_map(|command| crate::history::normalize_history_command(&command))
            .rev()
            .take(PERSISTED_HISTORY_LIMIT)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();

        if self.persisted_history != history {
            self.persisted_history = history;
            cx.emit(TerminalModelEvent::Wakeup);
        }
    }

    fn sync_local_working_dir(&mut self, cx: &mut Context<Self>) {
        if self.connection_kind != TerminalConnectionKind::Local {
            return;
        }

        let Some(path) = self.latest_working_dir() else {
            return;
        };

        if self.current_working_dir.as_deref() == Some(path.as_str()) {
            return;
        }

        self.current_working_dir = Some(path.clone());
        cx.emit(TerminalModelEvent::WorkingDirChanged(path));
    }

    fn handle_terminal_event(&mut self, event: TerminalEvent, cx: &mut Context<Self>) {
        tracing::debug!(
            target: "terminal.history_prompt.osc",
            event = ?event,
            "terminal model handling terminal event"
        );
        match event {
            TerminalEvent::Wakeup => {
                self.sync_local_working_dir(cx);
                cx.emit(TerminalModelEvent::Wakeup);
            }
            TerminalEvent::PromptStart => {
                note_ssh_prompt_idle(
                    self.connection_kind,
                    &self.ssh_process_state,
                    &self.ssh_command_submitted_without_prompt_sync,
                );
                self.ssh_prompt_detected = true;
                cx.emit(TerminalModelEvent::PromptStart);
            }
            TerminalEvent::InputStart => {
                note_ssh_prompt_idle(
                    self.connection_kind,
                    &self.ssh_process_state,
                    &self.ssh_command_submitted_without_prompt_sync,
                );
                self.ssh_prompt_detected = true;
                cx.emit(TerminalModelEvent::InputStart);
            }
            TerminalEvent::CommandStart => {
                // 不直接修改 ssh_process_state，因为 bash DEBUG trap
                // 可能在 PS1 的命令 substitution 中误触发。
            }
            TerminalEvent::TitleChanged(title) => {
                self.title = title.clone();
                cx.emit(TerminalModelEvent::TitleChanged(title));
            }
            TerminalEvent::Bell => {
                cx.emit(TerminalModelEvent::Bell);
            }
            TerminalEvent::ChildExit(code) => {
                self.child_exited = Some(code);
                cx.emit(TerminalModelEvent::ChildExit(code));
            }
            TerminalEvent::ClipboardStore(_ty, data) => {
                cx.emit(TerminalModelEvent::ClipboardStore(data));
            }
            TerminalEvent::ClipboardLoad(_ty) => {
                // 剪贴板加载由 TerminalView 处理
            }
            TerminalEvent::WorkingDirChanged(path) => {
                self.current_working_dir = Some(path.clone());
                cx.emit(TerminalModelEvent::WorkingDirChanged(path));
            }
            TerminalEvent::SshPromptReady => {
                note_ssh_prompt_idle(
                    self.connection_kind,
                    &self.ssh_process_state,
                    &self.ssh_command_submitted_without_prompt_sync,
                );
                self.ssh_prompt_detected = true;
            }
            TerminalEvent::CommandFinished { exit_code } => {
                tracing::debug!("命令执行完毕，退出码: {}", exit_code);
                if let Some(last) = self.session_history.back_mut() {
                    last.exit_code = Some(exit_code);
                }
                note_ssh_prompt_idle(
                    self.connection_kind,
                    &self.ssh_process_state,
                    &self.ssh_command_submitted_without_prompt_sync,
                );
            }
            TerminalEvent::CommandRecorded(command) => {
                self.record_history_entry(&command, cx);
            }
        }
    }

    // ========== 公共 API ==========

    /// 获取 Term 的共享引用
    pub fn term(&self) -> &Arc<FairMutex<Term<GpuiEventProxy>>> {
        &self.term
    }

    /// Snapshot of terminal render state, captured in a single lock acquisition
    /// to avoid multiple independent locks that cause inconsistency and overhead.
    pub fn render_snapshot(&self) -> TerminalRenderSnapshot {
        let term = self.term.lock();
        TerminalRenderSnapshot {
            has_selection: term.selection.is_some(),
            selection_text: term.selection_to_string(),
            mode: *term.mode(),
            history_size: term.history_size(),
        }
    }

    /// 获取终端标题
    pub fn title(&self) -> &str {
        &self.title
    }

    /// 获取子进程退出码
    pub fn child_exited(&self) -> Option<i32> {
        self.child_exited
    }

    /// 获取连接状态
    pub fn connection_state(&self) -> &ConnectionState {
        &self.connection_state
    }

    /// 获取连接名称
    pub fn connection_name(&self) -> Option<&str> {
        self.connection_name.as_deref()
    }

    /// 获取连接 ID
    pub fn connection_id(&self) -> Option<i64> {
        self.connection_id
    }

    /// 获取当前工作目录（由 OSC 7 更新，仅 SSH 终端）
    pub fn current_working_dir(&self) -> Option<&str> {
        self.current_working_dir.as_deref()
    }

    /// 获取最新工作目录。
    pub fn latest_working_dir(&self) -> Option<String> {
        if let Some(pid) = self.local_shell_pid {
            if let Some(path) = read_local_working_dir_from_pid(pid) {
                return Some(path);
            }
        }

        if let Some(path) = self.local_cwd_file.as_deref() {
            if let Some(path) = read_local_working_dir(path) {
                return Some(path);
            }
        }

        self.current_working_dir.clone()
    }

    /// 获取 hosted 本地 PTY 的 session id（如果有）。
    pub fn local_pty_session_id(&self) -> Option<&str> {
        self.local_pty_session_id.as_deref()
    }

    /// 获取工作目录的显示形式，将 `~` 替换为实际 home 路径。
    pub fn working_dir_display(&self) -> Option<String> {
        self.latest_working_dir().map(|path| expand_tilde(&path))
    }

    /// 是否存在会在关闭时被中断的本地子进程。
    pub fn has_running_processes(&self) -> bool {
        if self.child_exited.is_some() {
            return false;
        }

        #[cfg(target_os = "linux")]
        {
            if self.connection_kind == TerminalConnectionKind::Local {
                return self.local_shell_pid.is_some_and(|pid| {
                    has_live_descendant_process(std::path::Path::new("/proc"), pid)
                });
            }
        }

        #[cfg(target_os = "macos")]
        {
            if self.connection_kind == TerminalConnectionKind::Local {
                let has_running_processes = self
                    .local_shell_pid
                    .map(resolve_local_shell_pid)
                    .is_some_and(has_live_descendant_process);
                return should_report_local_running_processes(
                    self.local_process_tree_settled.get(),
                    has_running_processes,
                );
            }
        }

        if self.connection_kind == TerminalConnectionKind::Ssh {
            let ssh_process_state = self.ssh_process_state.get();
            let interactive_mode_active = has_ssh_interactive_program_mode(self.mode());
            let command_submitted_without_prompt_sync =
                self.ssh_command_submitted_without_prompt_sync.get() && !self.ssh_prompt_detected;
            let result = should_report_ssh_running_processes(
                matches!(self.connection_state, ConnectionState::Connected),
                ssh_process_state,
                self.ssh_prompt_detected,
                interactive_mode_active,
                command_submitted_without_prompt_sync,
            );
            if result {
                tracing::debug!(
                    target: "terminal.ssh",
                    connection_state = ?self.connection_state,
                    ssh_process_state = ?ssh_process_state,
                    ssh_prompt_detected = self.ssh_prompt_detected,
                    interactive_mode_active,
                    command_submitted_without_prompt_sync,
                    "SSH has_running_processes = true (blocking close)"
                );
            }
            return result;
        }

        false
    }

    /// 获取 SSH prompt 检测状态（用于诊断 shell integration 是否生效）。
    pub fn ssh_prompt_detected(&self) -> bool {
        self.ssh_prompt_detected
    }

    /// 获取当前连接阶段提示
    pub fn connection_status_message(&self) -> Option<&str> {
        self.connection_status_message.as_deref()
    }

    /// 获取当前连接阶段提示和等待时长
    pub fn connection_status_label(&self) -> Option<String> {
        let message = self.connection_status_message.as_deref()?;
        let elapsed_secs = self
            .connection_wait_started_at
            .map(|started_at| started_at.elapsed().as_secs())
            .unwrap_or(0);
        Some(ssh::format_connection_progress_message(
            message,
            elapsed_secs,
        ))
    }

    /// 捕获整个终端内容为纯文本
    pub fn visible_content(&self) -> String {
        let term = self.term.lock();
        serialize_term_for_recovery(&term, 500).unwrap_or_default()
    }

    pub fn recovery_content(&self, max_lines: usize) -> Option<String> {
        let term = self.term.lock();
        serialize_term_for_recovery(&term, max_lines)
    }

    pub fn history_suggestions(&self, prefix: &str, limit: usize) -> Vec<String> {
        collect_history_suggestions_with_cwd(
            &self.session_history,
            &self.persisted_history,
            prefix,
            limit,
            self.current_working_dir.as_deref(),
        )
    }

    pub fn history_search_results(&self, query: &str, limit: usize) -> Vec<String> {
        collect_history_search_results(&self.session_history, &self.persisted_history, query, limit)
    }

    pub fn record_command(&mut self, command: &str, cx: &mut Context<Self>) {
        self.record_history_entry(command, cx);
    }

    /// 获取 SSH 连接配置（仅 SSH 终端）
    pub fn ssh_config(&self) -> Option<&SshTerminalConfig> {
        self.ssh_config.as_ref()
    }

    pub fn ssh_session_manager(&self) -> Option<&Arc<SshSessionManager>> {
        self.ssh_session_manager.as_ref()
    }

    /// 获取连接类型
    pub fn connection_kind(&self) -> TerminalConnectionKind {
        self.connection_kind
    }

    /// 是否可以重连
    pub fn can_reconnect(&self) -> bool {
        self.ssh_config.is_some() || self.serial_params.is_some()
    }

    /// 写入用户输入到终端，并更新与关闭提示相关的 SSH 状态。
    pub fn write_user_input(&self, data: &[u8]) {
        #[cfg(target_os = "macos")]
        note_local_user_input(self.connection_kind, &self.local_process_tree_settled, data);
        note_ssh_user_input(
            self.connection_kind,
            &self.ssh_process_state,
            &self.ssh_command_submitted_without_prompt_sync,
            data,
        );

        self.write(data);
    }

    /// 原始写入数据到终端，不更新用户输入相关状态。
    pub fn write(&self, data: &[u8]) {
        if let Some(ref backend) = self.backend {
            backend.write(data.to_vec());
        }
    }

    /// 直接把控制序列应用到本地终端模型，不经过 shell stdin。
    pub fn apply_escape_sequence(&mut self, data: &[u8], cx: &mut Context<Self>) {
        apply_term_escape_sequence(&self.term, data);
        cx.emit(TerminalModelEvent::Wakeup);
    }

    /// 调整终端大小
    pub fn resize(&mut self, cols: usize, rows: usize, pixel_width: u16, pixel_height: u16) {
        if self.cols == cols && self.rows == rows {
            return;
        }

        tracing::info!(
            "Terminal::resize: {}x{} -> {}x{}, pixel={}x{}",
            self.cols,
            self.rows,
            cols,
            rows,
            pixel_width,
            pixel_height
        );

        self.cols = cols;
        self.rows = rows;

        self.term.lock().resize(TermDimensions { cols, rows });

        if let Some(ref backend) = self.backend {
            backend.resize(TerminalSize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width,
                pixel_height,
            });
        }
    }

    /// 重新连接 SSH 或串口
    pub fn reconnect(&mut self, cx: &mut Context<Self>) {
        if let Some(config) = self.ssh_config.clone() {
            let Some(event_tx) = self.event_tx.clone() else {
                return;
            };
            let Some(event_proxy) = self.event_proxy.clone() else {
                return;
            };
            let session_manager = self
                .ssh_session_manager
                .clone()
                .unwrap_or_else(|| Arc::new(SshSessionManager::new(config.ssh_config.clone())));
            self.ssh_session_manager = Some(session_manager.clone());

            self.connection_state = ConnectionState::Connecting;
            self.connection_status_message =
                Some(SshConnectionStage::initial_for_config(&config.ssh_config).description());
            self.connection_wait_started_at = Some(Instant::now());
            self.reset_ssh_process_tracking();
            self.set_connection_active(false, cx);
            if let Some(backend) = self.backend.take() {
                backend.shutdown();
            }

            let generation = self.next_connection_generation();
            let term = self.term.clone();
            let connection_id = self.connection_id;
            let init_commands = self.init_commands.clone();
            let entity = cx.entity().downgrade();
            cx.spawn(async move |_, cx| {
                let _ = session_manager.disconnect().await;
                let _ = entity.update(cx, |terminal, cx| {
                    if !terminal.is_current_connection_generation(generation) {
                        return;
                    }

                    let (disconnect_tx, disconnect_rx) = tokio::sync::oneshot::channel::<()>();
                    Self::spawn_disconnect_handler(disconnect_rx, generation, cx);
                    Self::spawn_ssh_connect(
                        session_manager.clone(),
                        config.clone(),
                        term.clone(),
                        event_proxy.clone(),
                        event_tx.clone(),
                        connection_id,
                        Some(disconnect_tx),
                        init_commands.clone(),
                        generation,
                        cx,
                    );
                });
            })
            .detach();
            Self::spawn_connection_status_tick(cx);
        } else if let Some(params) = self.serial_params.clone() {
            let Some(event_tx) = self.event_tx.clone() else {
                return;
            };

            self.connection_state = ConnectionState::Connecting;
            self.connection_status_message = None;
            self.connection_wait_started_at = None;
            self.set_connection_active(false, cx);
            if let Some(backend) = self.backend.take() {
                backend.shutdown();
            }
            let generation = self.next_connection_generation();

            let (disconnect_tx, disconnect_rx) = tokio::sync::oneshot::channel::<()>();
            Self::spawn_disconnect_handler(disconnect_rx, generation, cx);
            Self::spawn_serial_connect(
                params,
                self.term.clone(),
                event_tx,
                Some(disconnect_tx),
                generation,
                cx,
            );
        } else {
            return;
        }

        cx.emit(TerminalModelEvent::Wakeup);
    }

    fn reset_ssh_process_tracking(&mut self) {
        self.ssh_process_state.set(SshProcessState::Unknown);
        self.ssh_prompt_detected = false;
        self.ssh_command_submitted_without_prompt_sync.set(false);
    }

    /// 更新 SSH 终端的路径同步设置。
    ///
    /// 路径同步由 shell 集成脚本发出 OSC 7，这里保留接口以兼容设置同步流程。
    pub fn set_sync_path_with_terminal(&mut self, _enabled: bool) {
        if self.connection_kind != TerminalConnectionKind::Ssh {
            return;
        }
    }

    /// 关闭终端（默认 Kill 模式，兼容旧调用）
    pub fn shutdown(&self) {
        self.close(TerminalCloseMode::Kill);
    }

    /// 按指定模式关闭终端
    pub fn close(&self, mode: TerminalCloseMode) {
        if let Some(ref backend) = self.backend {
            backend.close(mode);
        }
    }

    // ========== 选择操作 ==========

    /// 获取选中的文本
    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string()
    }

    /// 清除选择
    pub fn clear_selection(&mut self) {
        self.term.lock().selection = None;
    }

    /// 全选
    pub fn select_all(&mut self) {
        let mut term = self.term.lock();
        let start = AlacPoint::new(Line(-(term.history_size() as i32)), Column(0));
        let end = AlacPoint::new(
            Line(term.screen_lines() as i32 - 1),
            Column(term.columns() - 1),
        );
        term.selection = Some(Selection::new(SelectionType::Simple, start, Side::Left));
        if let Some(selection) = &mut term.selection {
            selection.update(end, Side::Right);
        }
    }

    /// 开始选择
    pub fn start_selection(&mut self, selection_type: SelectionType, point: AlacPoint, side: Side) {
        let mut term = self.term.lock();
        let point_with_offset = AlacPoint::new(
            point.line - term.grid().display_offset() as i32,
            point.column,
        );
        term.selection = Some(Selection::new(selection_type, point_with_offset, side));
    }

    /// 更新选择
    pub fn update_selection(&mut self, point: AlacPoint, side: Side) {
        let mut term = self.term.lock();
        let point_with_offset = AlacPoint::new(
            point.line - term.grid().display_offset() as i32,
            point.column,
        );
        if let Some(selection) = &mut term.selection {
            selection.update(point_with_offset, side);
        }
    }

    // ========== 滚动操作 ==========

    /// 滚动终端
    pub fn scroll(&mut self, delta: i32) {
        self.term
            .lock()
            .scroll_display(alacritty_terminal::grid::Scroll::Delta(delta));
    }

    /// 获取滚动代理（供视图层的滚动条使用）
    pub fn scroll_proxy(&self) -> TerminalScrollProxy {
        TerminalScrollProxy {
            term: self.term.clone(),
            event_tx: self.event_tx.clone(),
        }
    }

    // ========== Vi 模式 ==========

    /// 切换 Vi 模式
    pub fn toggle_vi_mode(&mut self) {
        self.term.lock().toggle_vi_mode();
    }

    /// 是否处于 Vi 模式
    pub fn in_vi_mode(&self) -> bool {
        self.term.lock().mode().contains(TermMode::VI)
    }

    /// 获取终端模式
    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }
}

fn format_connection_error(err: &anyhow::Error) -> String {
    format!("{err:#}")
}

impl EventEmitter<TerminalModelEvent> for Terminal {}

#[cfg(test)]
mod tests {
    use super::{
        apply_term_escape_sequence, build_cd_command, build_ssh_base_init_commands,
        build_ssh_init_commands, compose_ssh_init_commands, format_connection_error,
        is_osc_palette_line,
        resolve_default_windows_shell_from_env, shell_escape_arg,
        should_report_ssh_running_processes, SshProcessState, Terminal, SSH_PROMPT_HOOK_NAME,
        SSH_PROMPT_READY_COMMAND,
    };
    use alacritty_terminal::vte::ansi::{NamedColor, Rgb};
    use crate::history::{
        collect_history_suggestions, normalize_history_command, parse_shell_history,
        push_history_entry, HistoryEntry, ShellHistoryFormat,
    };
    use anyhow::anyhow;
    use std::collections::VecDeque;
    use std::fs;
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn shell_escape_arg_handles_single_quote() {
        let escaped = shell_escape_arg("a'b");
        assert_eq!(escaped, "'a'\"'\"'b'");
    }

    #[test]
    fn build_cd_command_escapes_injection_chars() {
        let cmd = build_cd_command("dir; rm -rf /");
        assert_eq!(cmd, "cd -- 'dir; rm -rf /'");
    }

    #[test]
    fn build_cd_command_escapes_newline() {
        let cmd = build_cd_command("a\nb");
        assert_eq!(cmd, "cd -- 'a\nb'");
    }

    #[test]
    fn build_ssh_init_commands_ignores_sync_path_switch_for_script_integration() {
        let enabled = build_ssh_init_commands(None, Some("/tmp"), Some("echo ready"), true)
            .expect("启用路径同步时应保留基础初始化命令");

        let disabled = build_ssh_init_commands(None, Some("/tmp"), Some("echo ready"), false)
            .expect("禁用路径同步时仍应保留其它初始化命令");
        assert_eq!(enabled, disabled);
        assert!(disabled.contains("echo ready"));
    }

    #[test]
    fn build_ssh_base_init_commands_prioritizes_explicit_working_dir() {
        let commands =
            build_ssh_base_init_commands(Some("/workspace"), Some("/default"), Some("echo ready"))
                .expect("显式工作目录应生成初始化命令");

        assert!(commands.contains("cd -- '/workspace'"));
        assert!(!commands.contains("/default"));
        assert!(!commands.contains("echo ready"));
    }

    #[test]
    fn compose_ssh_init_commands_supports_sync_only_mode() {
        let commands =
            compose_ssh_init_commands(None, true).expect("启用同步时应生成 SSH prompt hook");
        assert!(commands.contains(SSH_PROMPT_HOOK_NAME));
        assert!(commands.contains(SSH_PROMPT_READY_COMMAND));

        assert!(
            compose_ssh_init_commands(None, false)
                .expect("关闭路径同步时仍应生成 SSH prompt hook")
                .contains(SSH_PROMPT_READY_COMMAND),
            "关闭路径同步时仍应注入 SSH 空闲探针"
        );

        let with_base = compose_ssh_init_commands(Some("echo ready"), true)
            .expect("带基础命令时应继续注入 SSH prompt hook");
        assert!(with_base.contains("echo ready"));
        assert!(with_base.contains(SSH_PROMPT_HOOK_NAME));
    }

    #[test]
    fn should_report_ssh_running_processes_requires_detected_prompt() {
        assert!(
            should_report_ssh_running_processes(true, SshProcessState::Busy, true, false, false),
            "已连接且检测到 prompt 时，Busy 应阻止关闭"
        );
        assert!(
            !should_report_ssh_running_processes(true, SshProcessState::Busy, false, false, false),
            "未检测到 prompt 的 Busy 不能作为可靠的阻止关闭信号"
        );
    }

    #[test]
    fn should_report_ssh_running_processes_supports_submitted_command_fallback() {
        assert!(
            should_report_ssh_running_processes(true, SshProcessState::Busy, false, true, true),
            "未同步到 prompt 时，已提交命令应保守地阻止关闭"
        );
        assert!(
            !should_report_ssh_running_processes(true, SshProcessState::Busy, false, false, true),
            "无交互程序模式时，已提交命令不应单独阻止关闭"
        );
    }

    #[test]
    fn should_report_ssh_running_processes_ignores_idle_unknown_and_disconnected() {
        assert!(!should_report_ssh_running_processes(
            true,
            SshProcessState::Idle,
            true,
            false,
            false
        ));
        assert!(!should_report_ssh_running_processes(
            true,
            SshProcessState::Unknown,
            true,
            false,
            false
        ));
        assert!(!should_report_ssh_running_processes(
            false,
            SshProcessState::Busy,
            true,
            true,
            true
        ));
    }

    #[test]
    fn resolve_default_windows_shell_prefers_pwsh_from_path() {
        let temp_dir =
            std::env::temp_dir().join(format!("onetcli-terminal-test-{}", std::process::id()));
        fs::create_dir_all(&temp_dir).expect("应创建临时目录");

        let pwsh = temp_dir.join("pwsh.exe");
        let cmd = temp_dir.join("cmd.exe");
        fs::write(&pwsh, b"").expect("应创建 pwsh 占位文件");
        fs::write(&cmd, b"").expect("应创建 cmd 占位文件");

        let path_env = std::ffi::OsString::from(temp_dir.as_os_str());
        let resolved = resolve_default_windows_shell_from_env(
            Some(path_env.as_os_str()),
            None,
            Some(cmd.as_os_str()),
        );

        assert_eq!(resolved, pwsh.to_string_lossy());
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn resolve_default_windows_shell_falls_back_to_comspec() {
        let temp_dir = std::env::temp_dir().join(format!(
            "onetcli-terminal-test-comspec-{}",
            std::process::id()
        ));
        fs::create_dir_all(&temp_dir).expect("应创建临时目录");

        let cmd = temp_dir.join("cmd.exe");
        fs::write(&cmd, b"").expect("应创建 cmd 占位文件");

        let resolved = resolve_default_windows_shell_from_env(None, None, Some(cmd.as_os_str()));

        assert_eq!(resolved, cmd.to_string_lossy());
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn prepare_shell_integration_writes_lf_only_script() {
        let session_dir = std::env::temp_dir().join(format!("onetcli-{}", std::process::id()));
        let integration_path = session_dir.join("shell_integration.sh");
        let _ = fs::remove_dir_all(&session_dir);

        let (_env, _args) = super::prepare_shell_integration(Some("/bin/bash"));

        let script = fs::read_to_string(&integration_path).expect("应写入本地 integration 脚本");
        assert!(
            !script.contains('\r'),
            "本地 shell integration 脚本应统一写成 LF，避免 Windows 工件污染"
        );

        let _ = fs::remove_dir_all(&session_dir);
    }

    #[test]
    fn normalize_history_command_trims_and_rejects_blank_input() {
        assert_eq!(
            normalize_history_command("  git status  "),
            Some("git status".to_string())
        );
        assert_eq!(normalize_history_command("   "), None);
        assert_eq!(normalize_history_command("\n\t"), None);
    }

    #[test]
    fn parse_shell_history_supports_zsh_extended_format() {
        let commands = parse_shell_history(
            ": 1710000000:0;git status\n: 1710000001:0;cargo test\n",
            ShellHistoryFormat::Zsh,
        );

        assert_eq!(commands, vec!["git status", "cargo test"]);
    }

    #[test]
    fn push_history_entry_dedupes_adjacent_duplicates() {
        let mut entries = VecDeque::new();
        push_history_entry(&mut entries, "git status", 5);
        push_history_entry(&mut entries, "git status", 5);
        push_history_entry(&mut entries, "cargo test", 5);

        let commands: Vec<_> = entries.iter().map(|e| e.command.as_str()).collect();
        assert_eq!(commands, vec!["git status", "cargo test"]);
    }

    #[test]
    fn collect_history_suggestions_prioritizes_session_history() {
        let session: VecDeque<HistoryEntry> = ["git status", "git stash", "cargo test"]
            .iter()
            .map(|c| HistoryEntry::new(c.to_string()))
            .collect();
        let persisted = vec![
            "git status".to_string(),
            "git switch main".to_string(),
            "git commit".to_string(),
        ];

        let matches = collect_history_suggestions(&session, &persisted, "git s", 4);

        // session 中的结果优先（frecency 更高），且去重
        assert!(matches.contains(&"git stash".to_string()));
        assert!(matches.contains(&"git status".to_string()));
        assert!(matches.contains(&"git switch main".to_string()));
    }

    #[test]
    fn collect_history_suggestions_skips_empty_prefix() {
        let session: VecDeque<HistoryEntry> = [HistoryEntry::new("git status".to_string())].into();
        let persisted = vec!["git switch".to_string()];

        let matches = collect_history_suggestions(&session, &persisted, "   ", 5);

        assert!(matches.is_empty());
    }

    #[test]
    fn is_osc_palette_line_filters_ansi_color_sequences() {
        // 完整转义序列（ESC ] 4;...）
        assert!(is_osc_palette_line("\x1b]4;0;rgb:14/09/19"));
        assert!(is_osc_palette_line("\x1b]4;15;rgb:ff/ff/ff"));
        // ESC 被渲染为空格后以 4; 开头（trim 后）
        assert!(is_osc_palette_line("4;0;rgb:14/09/19"));
        assert!(is_osc_palette_line("4;1;rgb:75/20/94"));
        // 普通内容和命令不匹配
        assert!(!is_osc_palette_line("➜  ~"));
        assert!(!is_osc_palette_line("ls -la"));
        assert!(!is_osc_palette_line("4")); // 有 4 但不是调色板序列
        assert!(!is_osc_palette_line("4;rgb:14/09/19")); // 缺颜色索引
    }

    #[test]
    fn apply_term_escape_sequence_updates_palette_in_terminal_model() {
        let (event_tx, _event_rx) = unbounded_channel();
        let (term, _event_proxy, _colors) = Terminal::create_term(80, 24, event_tx);
        let target = Rgb {
            r: 0x12,
            g: 0x34,
            b: 0x56,
        };

        assert_ne!(term.lock().colors()[NamedColor::Red], Some(target));

        apply_term_escape_sequence(&term, b"\x1b]4;1;rgb:12/34/56\x07");

        assert_eq!(term.lock().colors()[NamedColor::Red], Some(target));
    }

    #[test]
    fn format_connection_error_keeps_anyhow_context_chain() {
        let err = anyhow!("channel open failed")
            .context("shell setup channel failed")
            .context("SSH connect failed");

        let message = format_connection_error(&err);

        assert!(
            message.contains("SSH connect failed"),
            "格式化结果应保留顶层上下文，实际: {message}"
        );
        assert!(
            message.contains("shell setup channel failed"),
            "格式化结果应保留中间上下文，实际: {message}"
        );
        assert!(
            message.contains("channel open failed"),
            "格式化结果应保留底层错误，实际: {message}"
        );
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct TermDimensions {
    cols: usize,
    rows: usize,
}

impl Dimensions for TermDimensions {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

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
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::tty::{self, Options as PtyOptions};
use futures::StreamExt;
use gpui::*;
use one_core::gpui_tokio::Tokio;
use one_core::storage::models::{
    ActiveConnections, ProxyType as StorageProxyType, SerialParams, SshAuthMethod, StoredConnection,
};
use std::cell::Cell;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::collections::HashSet;
#[cfg(any(test, not(target_os = "linux")))]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::time::interval;

#[cfg(any(test, target_os = "windows"))]
use std::env;
#[cfg(any(test, target_os = "windows"))]
use std::ffi::OsStr;
#[cfg(any(test, target_os = "windows"))]
use std::path::{Path, PathBuf};

use crate::pty_backend::{GpuiEventProxy, LocalPtyBackend};

use crate::{LocalConfig, SerialBackend, SshBackend, TerminalBackend, TerminalEvent, TerminalSize};
pub use ssh::{
    JumpServerConnectConfig, ProxyConnectConfig, ProxyType, PtyConfig, SshAuth, SshConnectConfig,
    SshConnectionStage,
};

/// Terminal 发出的事件，供 TerminalView 订阅
#[derive(Debug, Clone)]
pub enum TerminalModelEvent {
    /// 终端内容已更新，需要重新渲染
    Wakeup,
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

const DEFAULT_COLS: usize = 80;
const DEFAULT_ROWS: usize = 24;

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

const OSC7_PROMPT_COMMAND: &str = r#"printf "\033]7;file://%s%s\007" "${HOSTNAME:-}" "$PWD""#;
const SSH_PROMPT_READY_COMMAND: &str = r#"printf "\033]1337;OnetcliPromptReady=1\007""#;
const SSH_PROMPT_HOOK_NAME: &str = "onetcli_prompt_hook";

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

fn compose_ssh_init_commands(
    base_init_commands: Option<&str>,
    sync_path_with_terminal: bool,
) -> Option<String> {
    let mut commands = Vec::new();

    if let Some(base_commands) = base_init_commands.filter(|commands| !commands.is_empty()) {
        commands.push(base_commands.to_string());
    }

    commands.push(build_ssh_prompt_hook_command(sync_path_with_terminal));

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

fn build_ssh_prompt_hook_command(sync_path_with_terminal: bool) -> String {
    let mut hook_body = Vec::new();
    if sync_path_with_terminal {
        hook_body.push(OSC7_PROMPT_COMMAND);
    }
    hook_body.push(SSH_PROMPT_READY_COMMAND);
    let hook_body = hook_body.join("; ");

    // 使用守卫变量防止重复执行。ONETCLI_PROMPT_HOOK_SETUP 变量用于检测当前 shell 会话是否已初始化过 hook，
    // 避免在 .zshrc 等文件被重复 source 时导致 hook 被多次注册和输出。
    format!(
        "\
if [ -z \"$ONETCLI_PROMPT_HOOK_SETUP\" ]; then \
export ONETCLI_PROMPT_HOOK_SETUP=1; \
{hook_name}() {{ {hook_body}; }}; \
if [ -n \"$ZSH_VERSION\" ]; then \
typeset -ga precmd_functions; \
case \" ${{precmd_functions[*]}} \" in *\" {hook_name} \"*) ;; *) precmd_functions+=({hook_name}) ;; esac; \
else \
PROMPT_COMMAND='{hook_name}'${{PROMPT_COMMAND:+\";$PROMPT_COMMAND\"}}; export PROMPT_COMMAND; \
fi; \
{hook_name}; \
fi",
        hook_name = SSH_PROMPT_HOOK_NAME,
        hook_body = hook_body,
    )
}

fn normalize_working_dir(path: &str) -> Option<String> {
    let path = path.trim();
    (!path.is_empty()).then(|| path.to_string())
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

fn note_ssh_user_input(
    connection_kind: TerminalConnectionKind,
    ssh_process_state: &Cell<SshProcessState>,
    data: &[u8],
) {
    if connection_kind != TerminalConnectionKind::Ssh || data.is_empty() {
        return;
    }

    let should_mark_busy = matches!(ssh_process_state.get(), SshProcessState::Busy)
        || data.iter().any(|byte| matches!(*byte, b'\r' | b'\n'));
    if should_mark_busy {
        ssh_process_state.set(SshProcessState::Busy);
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

#[cfg(any(test, not(target_os = "linux")))]
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
fn build_local_shell(shell: Option<String>) -> Option<tty::Shell> {
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
fn build_local_shell(shell: Option<String>) -> Option<tty::Shell> {
    shell.map(|program| tty::Shell::new(program, vec![]))
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
    /// SSH 会话的远端进程状态，由 prompt hook 与用户输入共同驱动。
    ssh_process_state: Cell<SshProcessState>,
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
    /// SSH 基础初始化命令（不含 OSC7，用于运行时重建）
    ssh_base_init_commands: Option<String>,
    /// 初始化命令（连接成功后执行）
    init_commands: Option<String>,

    /// 连接类型
    connection_kind: TerminalConnectionKind,
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
            ssh_process_state: Cell::new(SshProcessState::Unknown),
            serial_params: None,
            event_tx: Some(event_tx),
            event_proxy: None,
            connection_id: None,
            connection_name: None,
            ssh_base_init_commands: None,
            init_commands: None,
            connection_kind: TerminalConnectionKind::Local,
        }
    }

    pub fn new_local_or_disconnected(
        config: LocalConfig,
        cx: &mut Context<Self>,
    ) -> (Self, Option<String>) {
        match Self::new_local(config, cx) {
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
        let (event_tx, event_rx) = unbounded_channel::<TerminalEvent>();
        let (term, event_proxy, _colors) =
            Self::create_term(DEFAULT_COLS, DEFAULT_ROWS, event_tx.clone());
        let LocalConfig {
            shell,
            working_dir,
            env,
            cwd_file,
        } = config;

        let pty_options = PtyOptions {
            shell: build_local_shell(shell),
            working_directory: working_dir.clone().map(Into::into),
            env: env.into_iter().collect(),
            drain_on_exit: true,
            #[cfg(target_os = "windows")]
            escape_args: true,
        };
        let local_backend = LocalPtyBackend::new(term.clone(), event_proxy, pty_options)?;
        let local_shell_pid = local_backend.child_pid();
        #[cfg(not(target_os = "linux"))]
        let local_cwd_file = {
            let cwd_file_path = cwd_file
                .map(std::path::PathBuf::from)
                .unwrap_or_else(next_local_cwd_file_path);
            let _ = std::fs::write(&cwd_file_path, "");
            // local_backend.write(
            //     build_local_cwd_tracking_init_command(&cwd_file_path.to_string_lossy())
            //         .into_bytes(),
            // );
            Some(cwd_file_path)
        };
        #[cfg(target_os = "linux")]
        let local_cwd_file = {
            let _ = cwd_file;
            None
        };

        Self::spawn_event_loop(event_rx, cx);
        #[cfg(target_os = "macos")]
        Self::spawn_local_process_tree_settler(cx);

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
            ssh_process_state: Cell::new(SshProcessState::Unknown),
            serial_params: None,
            event_tx: Some(event_tx),
            event_proxy: None, // 本地终端的 event_proxy 已在 LocalPtyBackend 中设置
            connection_id: None,
            connection_name: None,
            ssh_base_init_commands: None,
            init_commands: None,
            connection_kind: TerminalConnectionKind::Local,
        })
    }

    /// 创建 SSH 终端
    pub fn new_ssh(
        conn: StoredConnection,
        cx: &mut Context<Self>,
        working_dir: Option<&str>,
        sync_path_with_terminal: bool,
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

        // 构建初始化命令
        let ssh_base_init_commands = build_ssh_base_init_commands(
            working_dir,
            ssh_params.default_directory.as_deref(),
            ssh_params.init_script.as_deref(),
        );
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
        let (disconnect_tx, disconnect_rx) = tokio::sync::oneshot::channel::<()>();

        Self::spawn_disconnect_handler(disconnect_rx, cx);
        Self::spawn_event_loop(event_rx, cx);
        Self::spawn_ssh_connect(
            config.clone(),
            term.clone(),
            event_proxy.clone(),
            event_tx.clone(),
            Some(disconnect_tx),
            init_commands.clone(),
            cx,
        );
        Self::spawn_connection_status_tick(cx);

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
            connection_status_message: Some(
                SshConnectionStage::initial_for_config(&config.ssh_config).description(),
            ),
            connection_wait_started_at: Some(Instant::now()),
            cols,
            rows,
            ssh_config: Some(config),
            ssh_process_state: Cell::new(SshProcessState::Unknown),
            serial_params: None,
            event_tx: Some(event_tx),
            event_proxy: Some(event_proxy),
            connection_id: conn.id,
            connection_name: Some(conn.name),
            ssh_base_init_commands,
            init_commands,
            connection_kind: TerminalConnectionKind::Ssh,
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

        Self::spawn_disconnect_handler(disconnect_rx, cx);
        Self::spawn_event_loop(event_rx, cx);
        Self::spawn_serial_connect(
            serial_params.clone(),
            term.clone(),
            event_tx.clone(),
            Some(disconnect_tx),
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
            ssh_process_state: Cell::new(SshProcessState::Unknown),
            serial_params: Some(serial_params),
            event_tx: Some(event_tx),
            event_proxy: None,
            connection_id: conn.id,
            connection_name: Some(conn.name),
            ssh_base_init_commands: None,
            init_commands: None,
            connection_kind: TerminalConnectionKind::Serial,
        }
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
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let _ = disconnect_rx.await;
            let _ = entity.update(cx, |this, cx| {
                this.connection_state = ConnectionState::Disconnected { error: None };
                this.connection_status_message = None;
                this.connection_wait_started_at = None;
                this.backend = None;
                this.child_exited = Some(0);
                this.ssh_process_state.set(SshProcessState::Unknown);
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
        config: SshTerminalConfig,
        term: Arc<FairMutex<Term<GpuiEventProxy>>>,
        event_proxy: GpuiEventProxy,
        event_tx: UnboundedSender<TerminalEvent>,
        on_disconnect: Option<tokio::sync::oneshot::Sender<()>>,
        init_commands: Option<String>,
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
                config.ssh_config,
                config.pty_config,
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
                this.handle_ssh_result(result, cx);
            });
        })
        .detach();
    }

    fn handle_ssh_result(
        &mut self,
        result: Result<Result<SshBackend, anyhow::Error>, tokio::task::JoinError>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Ok(backend)) => {
                self.connection_state = ConnectionState::Connected;
                self.connection_status_message = None;
                self.connection_wait_started_at = None;
                self.ssh_process_state.set(SshProcessState::Unknown);
                self.set_connection_active(true, cx);
                // 连接后重新调整终端大小
                self.term.lock().resize(TermDimensions {
                    cols: self.cols,
                    rows: self.rows,
                });
                // 重要：将当前终端尺寸同步到新连接的 SSH 后端
                // 因为远程 PTY 是用 PtyConfig 默认尺寸（80x24）创建的，
                // 需要调整到当前实际尺寸
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
                    error: Some(e.to_string()),
                };
                self.connection_status_message = None;
                self.connection_wait_started_at = None;
                self.ssh_process_state.set(SshProcessState::Unknown);
                self.set_connection_active(false, cx);
            }
            Err(e) => {
                self.connection_state = ConnectionState::Disconnected {
                    error: Some(e.to_string()),
                };
                self.connection_status_message = None;
                self.connection_wait_started_at = None;
                self.ssh_process_state.set(SshProcessState::Unknown);
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
                this.handle_serial_result(result, cx);
            });
        })
        .detach();
    }

    fn handle_serial_result(
        &mut self,
        result: anyhow::Result<SerialBackend>,
        cx: &mut Context<Self>,
    ) {
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

    fn handle_terminal_event(&mut self, event: TerminalEvent, cx: &mut Context<Self>) {
        match event {
            TerminalEvent::Wakeup => {
                cx.emit(TerminalModelEvent::Wakeup);
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
                self.ssh_process_state.set(SshProcessState::Idle);
            }
        }
    }

    // ========== 公共 API ==========

    /// 获取 Term 的共享引用
    pub fn term(&self) -> &Arc<FairMutex<Term<GpuiEventProxy>>> {
        &self.term
    }

    /// 获取终端标题
    pub fn title(&self) -> &str {
        &self.title
    }

    /// 获取子进程退出码
    pub fn child_exited(&self) -> Option<i32> {
        self.child_exited
    }

    /// 是否存在会在关闭时被中断的本地子进程。
    ///
    /// Linux / macOS 本地终端支持精确检测：shell 停在提示符时返回 false，
    /// shell 下仍有存活子进程（如 vim、top、sleep、后台任务）时返回 true。
    /// SSH 终端通过远端 prompt hook 判断：回到提示符时视为空闲，用户提交命令后直到下次提示符前视为运行中。
    /// 串口等其它连接类型仍返回 false，避免把“会话仍然打开”误判为“任务仍在运行”。
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
            return matches!(self.connection_state, ConnectionState::Connected)
                && self.ssh_process_state.get() == SshProcessState::Busy;
        }

        false
    }

    /// 获取连接状态
    pub fn connection_state(&self) -> &ConnectionState {
        &self.connection_state
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
    ///
    /// 本地终端优先读取 cwd 跟踪文件，避免恢复时仍停留在初始目录。
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

    /// 获取 SSH 连接配置（仅 SSH 终端）
    pub fn ssh_config(&self) -> Option<&SshTerminalConfig> {
        self.ssh_config.as_ref()
    }

    /// 获取连接类型
    pub fn connection_kind(&self) -> TerminalConnectionKind {
        self.connection_kind
    }

    /// 是否可以重连
    pub fn can_reconnect(&self) -> bool {
        self.ssh_config.is_some() || self.serial_params.is_some()
    }

    /// 写入数据到终端
    pub fn write(&self, data: &[u8]) {
        #[cfg(target_os = "macos")]
        note_local_user_input(self.connection_kind, &self.local_process_tree_settled, data);
        note_ssh_user_input(self.connection_kind, &self.ssh_process_state, data);

        if let Some(ref backend) = self.backend {
            backend.write(data.to_vec());
        }
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

            self.connection_state = ConnectionState::Connecting;
            self.connection_status_message =
                Some(SshConnectionStage::initial_for_config(&config.ssh_config).description());
            self.connection_wait_started_at = Some(Instant::now());
            self.ssh_process_state.set(SshProcessState::Unknown);

            let (disconnect_tx, disconnect_rx) = tokio::sync::oneshot::channel::<()>();
            Self::spawn_disconnect_handler(disconnect_rx, cx);
            Self::spawn_ssh_connect(
                config,
                self.term.clone(),
                event_proxy,
                event_tx,
                Some(disconnect_tx),
                self.init_commands.clone(),
                cx,
            );
            Self::spawn_connection_status_tick(cx);
        } else if let Some(params) = self.serial_params.clone() {
            let Some(event_tx) = self.event_tx.clone() else {
                return;
            };

            self.connection_state = ConnectionState::Connecting;
            self.connection_status_message = None;
            self.connection_wait_started_at = None;

            let (disconnect_tx, disconnect_rx) = tokio::sync::oneshot::channel::<()>();
            Self::spawn_disconnect_handler(disconnect_rx, cx);
            Self::spawn_serial_connect(
                params,
                self.term.clone(),
                event_tx,
                Some(disconnect_tx),
                cx,
            );
        } else {
            return;
        }

        cx.emit(TerminalModelEvent::Wakeup);
    }

    /// 更新 SSH 终端的路径同步设置。
    ///
    /// 更新 `init_commands` 以影响后续新建连接或重连。
    pub fn set_sync_path_with_terminal(&mut self, enabled: bool) {
        if self.connection_kind != TerminalConnectionKind::Ssh {
            return;
        }

        self.init_commands =
            compose_ssh_init_commands(self.ssh_base_init_commands.as_deref(), enabled);
    }

    /// 关闭终端
    pub fn shutdown(&self) {
        if let Some(ref backend) = self.backend {
            backend.shutdown();
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

    /// 捕获整个终端内容（完整 grid 缓冲区 + 回滚历史）为纯文本
    pub fn visible_content(&self) -> String {
        let term = self.term.lock();
        let history_size = term.history_size();
        let screen_lines = term.screen_lines();
        let cols = term.columns();

        let mut lines = Vec::new();

        // 遍历整个 grid：从最旧的回滚行到最新屏幕行
        for line_idx in 0..(history_size + screen_lines) {
            let grid_line = Line((line_idx as i32) - (history_size as i32));
            let row = &term.grid()[grid_line];
            let text: String = row[..].iter().map(|cell| cell.c).collect();
            let trimmed = text.trim_end().to_string();
            if !trimmed.is_empty() {
                lines.push(trimmed);
            }
        }

        // 限制输出大小（最多 500 行，保留最新内容）
        const MAX_LINES: usize = 500;
        if lines.len() > MAX_LINES {
            lines = lines[lines.len() - MAX_LINES..].to_vec();
        }

        lines.join("\n")
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

impl EventEmitter<TerminalModelEvent> for Terminal {}

#[cfg(test)]
mod tests {
    use super::{
        build_cd_command, build_local_cwd_tracking_init_command, build_ssh_base_init_commands,
        build_ssh_init_commands, build_ssh_prompt_hook_command, compose_ssh_init_commands,
        next_local_cwd_file_path, note_ssh_user_input, read_local_working_dir,
        resolve_default_windows_shell_from_env, shell_escape_arg, LocalPtyBackend, SshProcessState,
        TerminalConnectionKind, OSC7_PROMPT_COMMAND, SSH_PROMPT_HOOK_NAME,
        SSH_PROMPT_READY_COMMAND,
    };
    use alacritty_terminal::tty::Options as PtyOptions;
    #[cfg(target_os = "macos")]
    use gpui::{AppContext, TestAppContext};
    use std::cell::Cell;
    use std::fs;
    #[cfg(target_os = "linux")]
    use std::path::Path;
    #[cfg(target_os = "macos")]
    use std::process::{Child, Command};
    #[cfg(target_os = "macos")]
    use std::thread;
    #[cfg(target_os = "macos")]
    use std::time::{Duration, Instant};

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
    fn build_ssh_init_commands_respects_sync_path_switch() {
        let enabled = build_ssh_init_commands(None, Some("/tmp"), Some("echo ready"), true)
            .expect("启用路径同步时应生成初始化命令");
        assert!(enabled.contains(OSC7_PROMPT_COMMAND));
        assert!(enabled.contains(SSH_PROMPT_READY_COMMAND));

        let disabled = build_ssh_init_commands(None, Some("/tmp"), Some("echo ready"), false)
            .expect("禁用路径同步时仍应保留其它初始化命令");
        assert!(!disabled.contains(OSC7_PROMPT_COMMAND));
        assert!(disabled.contains(SSH_PROMPT_READY_COMMAND));
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
        assert!(commands.contains(OSC7_PROMPT_COMMAND));
        assert!(commands.contains(SSH_PROMPT_READY_COMMAND));

        assert!(
            compose_ssh_init_commands(None, false)
                .expect("关闭路径同步时仍应生成 SSH prompt hook")
                .contains(SSH_PROMPT_READY_COMMAND),
            "关闭路径同步时仍应注入 SSH 空闲探针"
        );
    }

    #[test]
    fn ssh_prompt_hook_command_supports_zsh_and_bash_style_hooks() {
        let commands = build_ssh_prompt_hook_command(true);
        assert!(commands.contains("precmd_functions"));
        assert!(commands.contains("PROMPT_COMMAND"));
        assert!(commands.contains(SSH_PROMPT_HOOK_NAME));
        assert!(commands.contains(OSC7_PROMPT_COMMAND));
        assert!(commands.contains(SSH_PROMPT_READY_COMMAND));
    }

    #[test]
    fn ssh_user_input_marks_busy_only_after_command_submission() {
        let state = Cell::new(SshProcessState::Idle);

        note_ssh_user_input(TerminalConnectionKind::Ssh, &state, b"top");
        assert_eq!(state.get(), SshProcessState::Idle);

        note_ssh_user_input(TerminalConnectionKind::Ssh, &state, b"\r");
        assert_eq!(state.get(), SshProcessState::Busy);

        state.set(SshProcessState::Idle);
        note_ssh_user_input(TerminalConnectionKind::Local, &state, b"top\r");
        assert_eq!(state.get(), SshProcessState::Idle);
    }

    #[test]
    fn ssh_terminal_running_processes_follow_remote_state() {
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (term, _event_proxy, _colors) =
            super::Terminal::create_term(super::DEFAULT_COLS, super::DEFAULT_ROWS, event_tx);

        let terminal = super::Terminal {
            term,
            backend: None,
            title: String::new(),
            current_working_dir: None,
            local_shell_pid: None,
            local_cwd_file: None,
            #[cfg(target_os = "macos")]
            local_process_tree_settled: Cell::new(true),
            child_exited: None,
            connection_state: super::ConnectionState::Connected,
            connection_status_message: None,
            connection_wait_started_at: None,
            cols: super::DEFAULT_COLS,
            rows: super::DEFAULT_ROWS,
            ssh_config: None,
            ssh_process_state: Cell::new(SshProcessState::Busy),
            serial_params: None,
            event_tx: None,
            event_proxy: None,
            connection_id: None,
            connection_name: None,
            ssh_base_init_commands: None,
            init_commands: None,
            connection_kind: TerminalConnectionKind::Ssh,
        };
        assert!(terminal.has_running_processes());

        terminal.ssh_process_state.set(SshProcessState::Idle);
        assert!(!terminal.has_running_processes());
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

    #[test]
    fn read_local_working_dir_trims_trailing_newlines() {
        let temp_path =
            std::env::temp_dir().join(format!("onetcli-cwd-test-{}", std::process::id()));
        fs::write(&temp_path, "/tmp/demo\n").expect("应写入 cwd 文件");

        assert_eq!(
            read_local_working_dir(&temp_path).as_deref(),
            Some("/tmp/demo")
        );

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn build_local_cwd_tracking_init_command_covers_zsh_and_bash() {
        let command = build_local_cwd_tracking_init_command("/tmp/demo");

        assert!(command.contains("ONETCLI_CWD_FILE='/tmp/demo'"));
        assert!(command.contains("typeset -ga precmd_functions;"));
        assert!(command.contains("precmd_functions+=(onetcli_cwd_write)"));
        assert!(command.contains("pwd > \"$ONETCLI_CWD_FILE\" 2>/dev/null"));
        assert!(command.contains("PROMPT_COMMAND='pwd > \"$ONETCLI_CWD_FILE\" 2>/dev/null'"));
    }

    #[test]
    fn next_local_cwd_file_path_is_unique_per_terminal() {
        let first = next_local_cwd_file_path();
        let second = next_local_cwd_file_path();

        assert_ne!(first, second);
        assert!(first.file_name().unwrap_or_default() != second.file_name().unwrap_or_default());
    }

    #[cfg(target_os = "linux")]
    fn write_proc_entry(root: &Path, pid: u32, state: char, children: &[u32]) {
        let proc_dir = root.join(pid.to_string());
        let task_dir = proc_dir.join("task").join(pid.to_string());
        fs::create_dir_all(&task_dir).expect("应创建伪 proc 目录");
        fs::write(
            proc_dir.join("stat"),
            format!("{pid} (fake process) {state} 0 0 0 0\n"),
        )
        .expect("应写入伪 stat 文件");
        let children_text = children
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        fs::write(task_dir.join("children"), children_text).expect("应写入伪 children 文件");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parse_proc_state_supports_names_with_spaces() {
        let state = super::parse_proc_state("123 (ssh worker) S 0 0 0 0");
        assert_eq!(state, Some('S'));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn has_live_descendant_process_detects_non_zombie_children() {
        let temp_dir =
            std::env::temp_dir().join(format!("onetcli-proc-tree-live-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("应创建伪 proc 根目录");

        write_proc_entry(&temp_dir, 100, 'S', &[200]);
        write_proc_entry(&temp_dir, 200, 'S', &[]);

        assert!(super::has_live_descendant_process(&temp_dir, 100));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn has_live_descendant_process_ignores_zombie_children() {
        let temp_dir =
            std::env::temp_dir().join(format!("onetcli-proc-tree-zombie-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("应创建伪 proc 根目录");

        write_proc_entry(&temp_dir, 100, 'S', &[200]);
        write_proc_entry(&temp_dir, 200, 'Z', &[]);

        assert!(!super::has_live_descendant_process(&temp_dir, 100));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[cfg(target_os = "macos")]
    fn spawn_sleep_child(seconds: u64) -> Child {
        Command::new("sleep")
            .arg(seconds.to_string())
            .spawn()
            .expect("应创建 sleep 子进程")
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn has_live_descendant_process_detects_non_zombie_children() {
        let mut child = spawn_sleep_child(3);

        assert!(super::has_live_descendant_process(std::process::id()));

        let _ = child.kill();
        let _ = child.wait();
    }

    #[cfg(target_os = "macos")]
    unsafe fn fork_exit_child(delay_ms: u32) -> libc::pid_t {
        let pid = libc::fork();
        assert!(pid >= 0, "fork 应成功");
        if pid == 0 {
            libc::usleep(delay_ms * 1000);
            libc::_exit(0);
        }
        pid
    }

    #[cfg(target_os = "macos")]
    fn wait_until_terminal_unblocked() {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if !super::has_live_descendant_process(std::process::id()) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("子进程退出后终端仍被错误识别为存在活动进程");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn has_live_descendant_process_ignores_zombie_children() {
        let pid = unsafe { fork_exit_child(50) };

        assert!(super::has_live_descendant_process(std::process::id()));
        wait_until_terminal_unblocked();

        assert!(!super::has_live_descendant_process(std::process::id()));

        let mut status = 0;
        unsafe {
            libc::waitpid(pid, &mut status, 0);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn resolve_terminal_process_pid_skips_login_wrapper() {
        assert_eq!(
            super::resolve_terminal_process_pid(10, Some("login"), true, &[20]),
            20
        );
        assert_eq!(
            super::resolve_terminal_process_pid(10, Some("login"), true, &[]),
            10
        );
        assert_eq!(
            super::resolve_terminal_process_pid(10, Some("zsh"), true, &[20]),
            10
        );
        assert_eq!(
            super::resolve_terminal_process_pid(10, None, false, &[20]),
            20
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn startup_helpers_do_not_trigger_close_prompt_before_terminal_settles() {
        assert!(!super::should_report_local_running_processes(false, true));
        assert!(!super::should_report_local_running_processes(false, false));
        assert!(super::should_report_local_running_processes(true, true));
        assert!(!super::should_report_local_running_processes(true, false));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn local_user_input_marks_startup_as_settled() {
        let settled = Cell::new(false);

        super::note_local_user_input(TerminalConnectionKind::Local, &settled, b"top\r");
        assert!(settled.get());

        settled.set(false);
        super::note_local_user_input(TerminalConnectionKind::Ssh, &settled, b"top\r");
        assert!(!settled.get());

        super::note_local_user_input(TerminalConnectionKind::Local, &settled, b"");
        assert!(!settled.get());
    }

    #[cfg(target_os = "macos")]
    fn macos_process_tree_lines(pid: u32, depth: usize, lines: &mut Vec<String>) {
        let indent = "  ".repeat(depth);
        let name = super::read_process_bsdinfo(pid)
            .as_ref()
            .and_then(super::process_name_from_bsdinfo)
            .unwrap_or_else(|| "<unavailable>".to_string());
        let children = super::read_child_pids(pid);
        lines.push(format!(
            "{indent}pid={pid} name={name} children={:?}",
            children
        ));
        for child_pid in children {
            macos_process_tree_lines(child_pid, depth + 1, lines);
        }
    }

    #[cfg(target_os = "macos")]
    fn macos_process_tree_snapshot(pid: u32) -> String {
        let mut lines = Vec::new();
        macos_process_tree_lines(pid, 0, &mut lines);
        lines.join(" | ")
    }

    #[cfg(target_os = "macos")]
    fn wait_for_local_process_state(
        local_pid: u32,
        timeout: Duration,
        expected_running: bool,
    ) -> u32 {
        let deadline = Instant::now() + timeout;
        loop {
            let resolved_pid = super::resolve_local_shell_pid(local_pid);
            let is_running = super::has_live_descendant_process(resolved_pid);
            if is_running == expected_running {
                return resolved_pid;
            }

            if Instant::now() >= deadline {
                let tree = macos_process_tree_snapshot(local_pid);
                panic!(
                    "等待本地终端进程状态超时: local_pid={local_pid}, resolved_pid={resolved_pid}, expected_running={expected_running}, actual_running={is_running}, tree={tree}"
                );
            }

            thread::sleep(Duration::from_millis(50));
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_idle_local_terminal_has_no_blocking_processes() {
        let config = super::LocalConfig::default();
        let pty_options = PtyOptions {
            shell: super::build_local_shell(config.shell),
            working_directory: config.working_dir.clone().map(Into::into),
            env: config.env.into_iter().collect(),
            drain_on_exit: true,
        };
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (term, event_proxy, _colors) =
            super::Terminal::create_term(super::DEFAULT_COLS, super::DEFAULT_ROWS, event_tx);
        let backend = LocalPtyBackend::new(term, event_proxy, pty_options).expect("应创建本地 PTY");
        let local_pid = backend.child_pid().expect("本地 PTY 应返回 child pid");

        let resolved_pid = wait_for_local_process_state(local_pid, Duration::from_secs(3), false);
        let tree = macos_process_tree_snapshot(local_pid);
        let has_children = super::has_live_descendant_process(resolved_pid);

        backend.shutdown();

        assert!(
            !has_children,
            "空闲本地终端不应被识别为存在活动进程: local_pid={local_pid}, resolved_pid={resolved_pid}, tree={tree}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_local_terminal_detects_blocking_process_after_command() {
        let config = super::LocalConfig::default();
        let pty_options = PtyOptions {
            shell: super::build_local_shell(config.shell),
            working_directory: config.working_dir.clone().map(Into::into),
            env: config.env.into_iter().collect(),
            drain_on_exit: true,
        };
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (term, event_proxy, _colors) =
            super::Terminal::create_term(super::DEFAULT_COLS, super::DEFAULT_ROWS, event_tx);
        let backend = LocalPtyBackend::new(term, event_proxy, pty_options).expect("应创建本地 PTY");
        let local_pid = backend.child_pid().expect("本地 PTY 应返回 child pid");

        let idle_pid = wait_for_local_process_state(local_pid, Duration::from_secs(3), false);
        backend.write(b"sleep 5\r".to_vec());

        let running_pid = wait_for_local_process_state(local_pid, Duration::from_secs(3), true);
        let tree = macos_process_tree_snapshot(local_pid);

        backend.shutdown();

        assert_ne!(idle_pid, 0, "空闲阶段应解析到有效 shell pid");
        assert_ne!(running_pid, 0, "运行命令后应解析到有效 shell pid");
        assert!(
            super::has_live_descendant_process(running_pid),
            "运行命令后应识别为存在活动进程: local_pid={local_pid}, idle_pid={idle_pid}, running_pid={running_pid}, tree={tree}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn terminal_entity_reports_running_processes_after_user_input() {
        let mut cx = TestAppContext::single();
        cx.update(one_core::gpui_tokio::init);
        let terminal: gpui::Entity<super::Terminal> = cx.update(|cx: &mut gpui::App| {
            cx.new(|cx| {
                super::Terminal::new_local(super::LocalConfig::default(), cx)
                    .expect("应创建本地终端")
            })
        });

        let local_pid = cx.read(|app| {
            terminal
                .read(app)
                .local_shell_pid
                .expect("本地终端应记录 child pid")
        });
        wait_for_local_process_state(local_pid, Duration::from_secs(3), false);

        terminal.update(&mut cx, |terminal, _| {
            terminal.local_process_tree_settled.set(true);
            terminal.write(b"sleep 5\r");
        });

        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            cx.run_until_parked();

            if cx.read(|app| terminal.read(app).has_running_processes()) {
                break;
            }

            if Instant::now() >= deadline {
                let (child_exited, settled) = cx.read(|app| {
                    let snapshot = terminal.read(app);
                    (
                        snapshot.child_exited,
                        snapshot.local_process_tree_settled.get(),
                    )
                });
                let tree = macos_process_tree_snapshot(local_pid);
                panic!(
                    "Terminal 实体未识别到运行中进程: local_pid={local_pid}, child_exited={:?}, settled={}, tree={tree}",
                    child_exited, settled,
                );
            }

            thread::sleep(Duration::from_millis(50));
        }

        terminal.update(&mut cx, |terminal: &mut super::Terminal, _| {
            terminal.shutdown()
        });
        cx.run_until_parked();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn terminal_entity_reports_running_processes_after_top_command() {
        let mut cx = TestAppContext::single();
        cx.update(one_core::gpui_tokio::init);
        let terminal: gpui::Entity<super::Terminal> = cx.update(|cx: &mut gpui::App| {
            cx.new(|cx| {
                super::Terminal::new_local(super::LocalConfig::default(), cx)
                    .expect("应创建本地终端")
            })
        });

        let local_pid = cx.read(|app| {
            terminal
                .read(app)
                .local_shell_pid
                .expect("本地终端应记录 child pid")
        });
        wait_for_local_process_state(local_pid, Duration::from_secs(3), false);

        terminal.update(&mut cx, |terminal, _| {
            terminal.local_process_tree_settled.set(true);
            terminal.write(b"top\r");
        });

        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            cx.run_until_parked();

            if cx.read(|app| terminal.read(app).has_running_processes()) {
                break;
            }

            if Instant::now() >= deadline {
                let (child_exited, settled) = cx.read(|app| {
                    let snapshot = terminal.read(app);
                    (
                        snapshot.child_exited,
                        snapshot.local_process_tree_settled.get(),
                    )
                });
                let tree = macos_process_tree_snapshot(local_pid);
                panic!(
                    "Terminal 实体在执行 top 后仍未识别到运行中进程: local_pid={local_pid}, child_exited={:?}, settled={}, tree={tree}",
                    child_exited, settled,
                );
            }

            thread::sleep(Duration::from_millis(50));
        }

        terminal.update(&mut cx, |terminal: &mut super::Terminal, _| {
            terminal.shutdown()
        });
        cx.run_until_parked();
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

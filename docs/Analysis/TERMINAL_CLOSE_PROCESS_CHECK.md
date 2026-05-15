---
标题: SSH/终端连接关闭时的进程检查逻辑分析
创建时间: 2026-05-15
状态: ✅ 已完成
---

# SSH/终端连接关闭时的进程检查逻辑分析

## 概述

在退出或关闭 SSH/终端连接时，系统会检查是否有进程正在运行，以防止用户意外关闭正在执行任务的终端。该机制通过多种方式检测进程状态，包括：

- **本地终端（Linux）**：通过 `/proc` 文件系统检查进程树
- **本地终端（macOS）**：通过 `libc::proc_listchildpids` 系统调用检查进程树
- **SSH 连接**：通过 shell integration 的 prompt 检测和进程状态跟踪

## 核心入口点

### 1. 关闭流程入口

**文件**: `crates/terminal_view/src/view.rs:4043`

```rust
fn try_close(
    &mut self,
    _tab_id: &str,
    window: &mut Window,
    cx: &mut Context<Self>,
) -> Task<bool>
```

**逻辑流程**:
1. 调用 `has_blocking_terminal_activity(cx)` 检查是否有阻塞活动
2. 如果没有阻塞活动 → 直接关闭（调用 `shutdown_for_close`）
3. 如果有阻塞活动 → 弹出确认对话框，询问用户是否强制关闭

### 2. 阻塞活动检查

**文件**: `crates/terminal_view/src/view.rs:772`

```rust
fn has_blocking_terminal_activity(&self, cx: &App) -> bool {
    self.terminal.read(cx).has_running_processes()
}
```

直接委托给 `Terminal::has_running_processes()` 方法。

## 核心检测逻辑

### 3. 进程检测主函数

**文件**: `crates/terminal/src/terminal.rs:2591`

```rust
pub fn has_running_processes(&self) -> bool
```

**检测策略**（按连接类型分支）:

#### 3.1 子进程已退出检查（通用）

```rust
if self.child_exited.is_some() {
    return false;
}
```

如果终端的主进程已经退出，直接返回 `false`（无运行进程）。

#### 3.2 本地终端 - Linux 平台

**文件**: `crates/terminal/src/terminal.rs:2596-2603`

```rust
#[cfg(target_os = "linux")]
{
    if self.connection_kind == TerminalConnectionKind::Local {
        return self.local_shell_pid.is_some_and(|pid| {
            has_live_descendant_process(std::path::Path::new("/proc"), pid)
        });
    }
}
```

**检测方式**:
- 通过 `/proc` 文件系统读取进程树
- 检查 shell PID 的所有子孙进程
- 排除僵尸进程（状态为 'Z' 或 'X'）

**实现函数**: `has_live_descendant_process` (Linux 版本)

**文件**: `crates/terminal/src/terminal.rs:629-648`

```rust
fn has_live_descendant_process(proc_root: &std::path::Path, pid: u32) -> bool {
    let mut seen = HashSet::new();
    let mut pending = read_proc_children(proc_root, pid);

    while let Some(child_pid) = pending.pop() {
        if !seen.insert(child_pid) {
            continue;
        }

        match read_proc_state(proc_root, child_pid) {
            Some('Z' | 'X') => {}  // 僵尸进程，跳过
            Some(_) => return true, // 有活跃进程
            None => continue,
        }

        pending.extend(read_proc_children(proc_root, child_pid));
    }

    false
}
```

**关键点**:
- 使用 BFS（广度优先搜索）遍历进程树
- 通过 `/proc/<pid>/stat` 读取进程状态
- 通过 `/proc/<pid>/task/<tid>/children` 读取子进程列表

#### 3.3 本地终端 - macOS 平台

**文件**: `crates/terminal/src/terminal.rs:2605-2617`

```rust
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
```

**检测方式**:
1. 通过 `resolve_local_shell_pid` 解析实际的 shell PID（处理 `login` 进程包装）
2. 通过 `has_live_descendant_process` 检查子孙进程
3. 通过 `should_report_local_running_processes` 判断是否应该报告（需要进程树稳定）

**PID 解析函数**: `resolve_local_shell_pid`

**文件**: `crates/terminal/src/terminal.rs:726-745`

```rust
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
```

**关键点**:
- 如果进程名为 `login` 且只有一个子进程，返回子进程 PID
- 最多等待 150ms 让进程树稳定
- 使用 `libc::proc_pidinfo` 和 `PROC_PIDTBSDINFO` 获取进程信息

**进程树检查函数**: `has_live_descendant_process` (macOS 版本)

**文件**: `crates/terminal/src/terminal.rs:748-768`

```rust
fn has_live_descendant_process(pid: u32) -> bool {
    let mut seen = HashSet::new();
    let mut pending = read_child_pids(pid);

    while let Some(child_pid) = pending.pop() {
        if !seen.insert(child_pid) {
            continue;
        }

        match read_process_bsdinfo(child_pid) {
            Some(info) if info.pbi_status == libc::SZOMB => {}  // 僵尸进程
            Some(_) => return true,  // 有活跃进程
            None if bsdinfo_access_denied(child_pid) => return true,  // 权限拒绝，保守处理
            None => continue,
        }

        pending.extend(read_child_pids(child_pid));
    }

    false
}
```

**关键点**:
- 使用 `libc::proc_listchildpids` 获取子进程列表
- 使用 `libc::proc_pidinfo` 获取进程状态
- 如果无法获取进程信息（权限拒绝），保守地认为有进程在运行

**进程树稳定判断**: `should_report_local_running_processes`

**文件**: `crates/terminal/src/terminal.rs:789-794`

```rust
fn should_report_local_running_processes(
    startup_settled: bool,
    has_running_processes: bool,
) -> bool {
    startup_settled && has_running_processes
}
```

**关键点**:
- 只有在启动稳定后（`startup_settled = true`）才报告进程
- 避免在终端刚启动时误报（shell 初始化脚本可能产生短暂的子进程）

#### 3.4 SSH 连接

**文件**: `crates/terminal/src/terminal.rs:2619-2643`

```rust
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
    // ... 日志记录 ...
    return result;
}
```

**检测方式**:
- **不依赖 `pwd` 或远程进程树检查**（SSH 无法可靠地获取远程进程信息）
- 依赖 **shell integration** 的 prompt 检测和状态跟踪

**SSH 进程状态判断**: `should_report_ssh_running_processes`

**文件**: `crates/terminal/src/terminal.rs:796-806`

```rust
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
```

**判断条件**（需要同时满足）:
1. SSH 连接已建立（`is_connected = true`）
2. **以下任一条件成立**:
   - **条件 A**: prompt 已检测到 **且** 进程状态为 `Busy`
   - **条件 B**: 交互模式激活 **且** 有命令提交但未同步 prompt

**SSH 进程状态枚举**:

**文件**: `crates/terminal/src/terminal.rs:536-540`

```rust
enum SshProcessState {
    Unknown,  // 未知状态（初始状态）
    Idle,     // 空闲（prompt 就绪）
    Busy,     // 忙碌（命令执行中）
}
```

**状态转换逻辑**:

1. **用户输入 → Busy**

**文件**: `crates/terminal/src/terminal.rs:857-882`

```rust
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
```

**触发条件**:
- 用户输入包含换行符（`\r` 或 `\n`）→ 认为提交了命令
- 或者当前已经是 `Busy` 状态

2. **Prompt 就绪 → Idle**

**文件**: `crates/terminal/src/terminal.rs:884-896`

```rust
fn note_ssh_prompt_idle(
    connection_kind: TerminalConnectionKind,
    ssh_process_state: &Cell<SshProcessState>,
    ssh_command_submitted_without_prompt_sync: &Cell<bool>,
) {
    if connection_kind != TerminalConnectionKind::Ssh {
        return;
    }

    tracing::debug!(target: "terminal.ssh", "SSH prompt ready -> Idle");
    ssh_process_state.set(SshProcessState::Idle);
    ssh_command_submitted_without_prompt_sync.set(false);
}
```

**触发时机**（通过 shell integration 事件）:

**文件**: `crates/terminal/src/terminal.rs:2443-2492`

```rust
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
TerminalEvent::SshPromptReady => {
    note_ssh_prompt_idle(
        self.connection_kind,
        &self.ssh_process_state,
        &self.ssh_command_submitted_without_prompt_sync,
    );
    self.ssh_prompt_detected = true;
    cx.emit(TerminalModelEvent::SshPromptReady);
}
```

**Shell Integration 事件**:
- `PromptStart`: prompt 开始显示
- `InputStart`: 用户输入开始
- `SshPromptReady`: SSH prompt 就绪（自定义事件）

**交互模式检测**: `has_ssh_interactive_program_mode`

**文件**: `crates/terminal/src/terminal.rs:808-812`

```rust
fn has_ssh_interactive_program_mode(mode: TermMode) -> bool {
    mode.intersects(
        TermMode::ALT_SCREEN | TermMode::APP_CURSOR | TermMode::APP_KEYPAD | TermMode::MOUSE_MODE,
    )
}
```

**检测依据**:
- 终端模式包含以下任一标志：
  - `ALT_SCREEN`: 备用屏幕缓冲区（如 vim、less）
  - `APP_CURSOR`: 应用光标模式
  - `APP_KEYPAD`: 应用键盘模式
  - `MOUSE_MODE`: 鼠标模式

## 关闭确认对话框

**文件**: `crates/terminal_view/src/view.rs:4071-4109`

当检测到有运行进程时，弹出确认对话框：

```rust
window.open_dialog(cx, move |dialog, _window, _cx| {
    let tx_ok = tx_ok.clone();
    let tx_cancel = tx_cancel.clone();
    let view = view.clone();
    dialog
        .title(t!("TerminalCloseDialog.running_process_close_title"))
        .confirm()
        .child(
            div().flex().flex_col().gap_2().child(
                div()
                    .text_sm()
                    .child(t!("TerminalCloseDialog.running_process_close_message")),
            ),
        )
        .button_props(
            DialogButtonProps::default()
                .ok_text(t!("TerminalCloseDialog.running_process_close_ok"))
                .cancel_text(t!("Common.cancel")),
        )
        .on_ok(move |_event, _window, _cx| {
            view.update(_cx, |this, cx| {
                this.shutdown_for_close(cx);
            });
            // ... 发送确认信号 ...
            true
        })
        .on_cancel(move |_event, _window, _cx| {
            // ... 发送取消信号 ...
            false
        })
});
```

**对话框内容**:
- 标题: `TerminalCloseDialog.running_process_close_title`
- 消息: `TerminalCloseDialog.running_process_close_message`
- 确认按钮: `TerminalCloseDialog.running_process_close_ok`
- 取消按钮: `Common.cancel`

## 关键依赖关系

### 本地终端（macOS）

```
try_close
  └─> has_blocking_terminal_activity
      └─> has_running_processes
          └─> resolve_local_shell_pid (解析实际 shell PID)
              └─> read_process_bsdinfo (libc::proc_pidinfo)
              └─> read_child_pids (libc::proc_listchildpids)
          └─> has_live_descendant_process (检查子孙进程)
              └─> read_child_pids (libc::proc_listchildpids)
              └─> read_process_bsdinfo (libc::proc_pidinfo)
          └─> should_report_local_running_processes (判断是否报告)
```

### 本地终端（Linux）

```
try_close
  └─> has_blocking_terminal_activity
      └─> has_running_processes
          └─> has_live_descendant_process (检查子孙进程)
              └─> read_proc_children (/proc/<pid>/task/<tid>/children)
              └─> read_proc_state (/proc/<pid>/stat)
```

### SSH 连接

```
try_close
  └─> has_blocking_terminal_activity
      └─> has_running_processes
          └─> should_report_ssh_running_processes
              ├─> ssh_process_state (Idle/Busy/Unknown)
              │   ├─> note_ssh_user_input (用户输入 → Busy)
              │   └─> note_ssh_prompt_idle (prompt 就绪 → Idle)
              │       └─> TerminalEvent::PromptStart/InputStart/SshPromptReady
              ├─> ssh_prompt_detected (shell integration 检测)
              └─> has_ssh_interactive_program_mode (交互模式检测)
```

## 关键发现

### 1. SSH 不依赖 `pwd`

**重要**: SSH 连接的进程检测 **不依赖 `pwd` 命令**，而是依赖：
- Shell integration 的 prompt 检测（`PromptStart`、`InputStart`、`SshPromptReady` 事件）
- 进程状态跟踪（`SshProcessState`: `Idle`/`Busy`/`Unknown`）
- 终端模式检测（`ALT_SCREEN`、`APP_CURSOR` 等）

### 2. `pwd` 的实际用途

搜索结果显示 `pwd` 仅在以下位置使用：

**文件**: `crates/terminal_view/src/ssh_form_window.rs:1757`

```rust
init_script: Some("pwd".to_string()),
```

这是 SSH 连接初始化脚本的一部分，用于获取初始工作目录，**与进程检测无关**。

### 3. Shell Integration 的关键作用

SSH 进程检测的核心依赖是 **shell integration**：
- 通过 shell 脚本注入 OSC 序列（如 `OSC 133;A`、`OSC 133;B`）
- 终端解析这些序列并触发 `PromptStart`、`InputStart` 等事件
- 根据事件更新 `ssh_process_state` 和 `ssh_prompt_detected`

### 4. 保守的检测策略

- **macOS**: 如果无法获取进程信息（权限拒绝），保守地认为有进程在运行
- **SSH**: 如果 prompt 未检测到但用户提交了命令，保守地认为有进程在运行
- **启动稳定**: 本地终端需要等待启动稳定后才报告进程，避免误报

## 潜在问题和改进方向

### 1. SSH 依赖 Shell Integration

**问题**: 如果远程 shell 不支持 shell integration（如旧版 bash、非标准 shell），进程检测可能失效。

**改进方向**:
- 提供降级方案（如定期执行 `ps` 命令检查进程）
- 增加用户配置选项（是否启用进程检测）

### 2. macOS 权限问题

**问题**: 如果无法获取子进程信息（权限拒绝），会保守地认为有进程在运行，可能导致误报。

**改进方向**:
- 提供更细粒度的权限检查
- 增加用户提示（如"无法检测进程状态，建议授予权限"）

### 3. 启动稳定判断

**问题**: `local_process_tree_settled` 的判断逻辑可能不够精确，导致启动阶段的进程检测延迟。

**改进方向**:
- 优化启动稳定判断逻辑（如基于时间阈值 + 进程树稳定性）
- 增加用户配置选项（启动稳定等待时间）

## 总结

SSH/终端连接关闭时的进程检测逻辑是一个多层次、多平台的复杂系统：

1. **本地终端（Linux）**: 通过 `/proc` 文件系统直接检查进程树
2. **本地终端（macOS）**: 通过 `libc` 系统调用检查进程树，并处理 `login` 进程包装
3. **SSH 连接**: 通过 shell integration 的 prompt 检测和状态跟踪，**不依赖 `pwd` 或远程进程树检查**

核心设计原则是 **保守检测**：宁可误报（阻止关闭），也不漏报（意外关闭正在运行的进程）。

//! 桥接主线程泵：消费 Agent 命令通道，在 GPUI 主线程执行终端操作。

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::time::{Duration, Instant};

use alacritty_terminal::grid::Scroll;
use alacritty_terminal::term::TermMode;
use anyhow::{Result, anyhow};
use gpui::{App, AppContext, AsyncApp, Entity, IntoElement, ParentElement, Styled, div, px};
use gpui_component::WindowExt;
use gpui_component::dialog::DialogButtonProps;
use rust_i18n::t;
use tokio::sync::{mpsc, oneshot};

use terminal::terminal::{Terminal, TerminalConnectionKind};

use super::{TerminalBridge, TerminalOpRequest, WriteOutcome};
use crate::registry::TerminalViewRegistry;
use crate::risk::{RiskLevel, assess_command};

/// 命令完成轮询间隔。
const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// 本地终端判定命令结束的静默期（无 OSC 133，只能启发式）。
const QUIET_PERIOD: Duration = Duration::from_millis(800);
/// 等待下限：过短的 wait_ms 会导致连提示符都等不到。
const MIN_WAIT: Duration = Duration::from_millis(500);
/// 等待上限：超时后中断等待并把输出交回模型决定下一步。
const MAX_WAIT: Duration = Duration::from_millis(30_000);
/// 写入后回读的尾部行数。
const WRITE_TAIL_LINES: usize = 100;
/// 读取工具的行数上限（防上下文撑爆）。
pub(crate) const MAX_READ_LINES: usize = 2000;
/// 高危命令确认对话框的超时时间（超时按拒绝处理并关闭对话框）。
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(120);

/// 主线程泵循环：串行消费 Agent 命令，直到通道关闭。
pub(super) async fn pump_loop(cx: &mut AsyncApp, mut rx: mpsc::Receiver<TerminalOpRequest>) {
    while let Some(request) = rx.recv().await {
        dispatch(cx, request).await;
    }
    tracing::info!("[agent_bridge] 命令通道已关闭，泵退出");
}

/// 分发单个请求；回复失败仅记录日志（Agent 侧会收到通道断开错误）。
async fn dispatch(cx: &mut AsyncApp, request: TerminalOpRequest) {
    match request {
        TerminalOpRequest::ListTerminals { reply } => {
            let infos =
                cx.update_global::<TerminalViewRegistry, _>(|registry, cx| registry.snapshot(cx));
            let _ = reply.send(infos);
        }
        TerminalOpRequest::ReadOutput {
            terminal_id,
            max_lines,
            from_line,
            reply,
        } => {
            let result = cx.update(|cx| read_output(cx, terminal_id, max_lines, from_line));
            let _ = reply.send(result);
        }
        TerminalOpRequest::WriteCommand {
            terminal_id,
            command,
            wait_ms,
            reply,
        } => {
            let result = write_command(cx, terminal_id, &command, wait_ms).await;
            // since_last_write 跟踪：写入成功后把"当前总行数"写入共享 map
            if let Ok(ref outcome) = result {
                let line_count = outcome.line_count_before_write;
                let last_write_map = cx.update_global::<TerminalBridge, _>(|bridge, _| {
                    bridge.last_write_lines.clone()
                });
                last_write_map.set(terminal_id, line_count);
            }
            let _ = reply.send(result);
        }
        TerminalOpRequest::GetCwd { terminal_id, reply } => {
            let result = cx.update(|cx| {
                let (terminal, _) = lookup_terminal(cx, terminal_id)?;
                Ok(terminal.read(cx).latest_working_dir())
            });
            let _ = reply.send(result);
        }
        TerminalOpRequest::GetSelection { terminal_id, reply } => {
            let result = cx.update(|cx| {
                let (terminal, _) = lookup_terminal(cx, terminal_id)?;
                Ok(terminal.read(cx).selection_text())
            });
            let _ = reply.send(result);
        }
        TerminalOpRequest::Focus { terminal_id, reply } => {
            let result = focus_terminal(cx, terminal_id);
            let _ = reply.send(result);
        }
    }
}

/// 按注册表 id 定位终端实体与其窗口句柄。
fn lookup_terminal(
    cx: &App,
    terminal_id: u64,
) -> Result<(Entity<Terminal>, Option<gpui::AnyWindowHandle>)> {
    let registry = cx
        .try_global::<TerminalViewRegistry>()
        .ok_or_else(|| anyhow!("终端注册表未初始化"))?;
    let (weak, window_handle) = registry
        .get(terminal_id)
        .ok_or_else(|| anyhow!("终端 #{terminal_id} 不存在或已关闭"))?;
    let view = weak
        .upgrade()
        .ok_or_else(|| anyhow!("终端 #{terminal_id} 已关闭"))?;
    Ok((view.read(cx).terminal(), window_handle))
}

/// 读取终端回滚内容（按行截断），无回滚时退回可见屏幕。
fn read_output(cx: &App, terminal_id: u64, max_lines: usize, from_line: usize) -> Result<String> {
    let (terminal, _) = lookup_terminal(cx, terminal_id)?;
    let terminal = terminal.read(cx);
    if terminal.mode().contains(TermMode::ALT_SCREEN) {
        // alt screen（vim/top 等）下序列化文本为空，直接说明比返回空串更可诊断
        anyhow::bail!("终端 #{terminal_id} 正处于全屏交互模式，暂无法读取文本输出");
    }
    let max_lines = max_lines.clamp(1, MAX_READ_LINES);
    Ok(terminal
        .recovery_content(max_lines, from_line)
        .unwrap_or_else(|| terminal.visible_content()))
}

/// 写入命令并等待完成。
///
/// 风险门控在泵内强制执行：不信任调用方自报的等级，
/// 确认对话框展示与最终写入的是同一命令串（防确认 A 写入 B）。
async fn write_command(
    cx: &mut AsyncApp,
    terminal_id: u64,
    command: &str,
    wait_ms: u64,
) -> Result<WriteOutcome> {
    let level = assess_command(command);
    if level.needs_confirmation() {
        let approved = confirm_risk(cx, terminal_id, command, level).await;
        if !approved {
            tracing::warn!("[agent_bridge] 高危命令被用户拒绝: {command}");
            anyhow::bail!("用户拒绝执行该高危命令");
        }
    }

    let (kind, initial_hash, line_count_before_write) = cx.update(|cx| -> Result<_> {
        let (terminal, _) = lookup_terminal(cx, terminal_id)?;
        let (kind, mode) = {
            let terminal_ref = terminal.read(cx);
            (terminal_ref.connection_kind(), terminal_ref.mode())
        };
        // 写前记录总行数（since_last_write 起点）
        let line_count_before_write = terminal.read(cx).total_line_count();
        // 写前滚到底（与键盘输入路径语义一致）
        terminal
            .read(cx)
            .term()
            .lock()
            .scroll_display(Scroll::Bottom);
        let data = wrap_agent_command(command, mode);
        terminal.read(cx).write_user_input(data.as_bytes());
        Ok((
            kind,
            content_hash(terminal.read(cx)),
            line_count_before_write,
        ))
    })?;

    let timed_out = wait_for_completion(cx, terminal_id, kind, wait_ms, initial_hash).await?;
    // 回读失败（终端关闭 / 进入全屏交互模式）降级为原因说明，写入本身已生效
    let output = cx.update(
        |cx| match read_output(cx, terminal_id, WRITE_TAIL_LINES, 0) {
            Ok(text) => text,
            Err(err) => format!("(回读输出失败: {err})"),
        },
    );
    Ok(WriteOutcome {
        output,
        timed_out,
        line_count_before_write,
    })
}

/// 包装待写入命令：bracketed-paste 感知 + 末尾回车（复用粘贴路径语义）。
fn wrap_agent_command(command: &str, mode: TermMode) -> String {
    // 归一化行尾：避免 \r\n 导致多执行一次回车
    let normalized = command.replace("\r\n", "\n").replace('\r', "");
    if mode.contains(TermMode::BRACKETED_PASTE) {
        // 剥离内层 ESC，防止伪造 bracketed-paste 结束序列逃逸
        format!("\x1b[200~{}\x1b[201~\r", normalized.replace('\x1b', ""))
    } else {
        format!("{normalized}\r")
    }
}

/// 等待命令完成；返回 Ok(true) 表示超时中断。
async fn wait_for_completion(
    cx: &mut AsyncApp,
    terminal_id: u64,
    kind: TerminalConnectionKind,
    wait_ms: u64,
    initial_hash: u64,
) -> Result<bool> {
    let deadline = Instant::now() + clamp_wait(wait_ms);
    let mut last_hash = initial_hash;
    let mut last_change = Instant::now();

    loop {
        cx.background_executor().timer(POLL_INTERVAL).await;
        let Some((hash, running)) = cx.update(|cx| poll_state(cx, terminal_id, kind)) else {
            anyhow::bail!("终端 #{terminal_id} 在等待期间被关闭");
        };
        if hash != last_hash {
            last_hash = hash;
            last_change = Instant::now();
        }
        let quiet = last_change.elapsed() >= QUIET_PERIOD;
        // 有进程检测能力的平台（SSH / unix 本地）以进程退出为准且等输出冲刷完；
        // 其余（Windows 本地 / 串口）退化为纯静默期启发式
        let done = quiet && !running;
        if done {
            tracing::debug!("[agent_bridge] 终端 #{terminal_id} 命令完成（quiet={quiet}）");
            return Ok(false);
        }
        if Instant::now() >= deadline {
            tracing::warn!(
                "[agent_bridge] 终端 #{terminal_id} 等待超时，命令可能仍在运行或已进入交互模式"
            );
            return Ok(true);
        }
    }
}

/// 轮询终端状态：内容哈希 + （支持进程检测的平台）是否有进程在跑；终端消失返回 None。
fn poll_state(cx: &App, terminal_id: u64, kind: TerminalConnectionKind) -> Option<(u64, bool)> {
    let (terminal, _) = lookup_terminal(cx, terminal_id).ok()?;
    let terminal = terminal.read(cx);
    let process_check_supported = kind == TerminalConnectionKind::Ssh
        || (kind == TerminalConnectionKind::Local && cfg!(unix));
    let running = process_check_supported && terminal.has_running_processes(true);
    Some((content_hash(terminal), running))
}

/// 终端尾部内容哈希，用于静默期检测（避免全量序列化比对）。
fn content_hash(terminal: &Terminal) -> u64 {
    let tail = terminal
        .recovery_content(WRITE_TAIL_LINES, 0)
        .unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    tail.hash(&mut hasher);
    hasher.finish()
}

/// wait_ms 钳制到 [500ms, 30s]。
fn clamp_wait(wait_ms: u64) -> Duration {
    Duration::from_millis(wait_ms).clamp(MIN_WAIT, MAX_WAIT)
}

/// 聚焦终端：激活窗口并聚焦输入区。
fn focus_terminal(cx: &mut AsyncApp, terminal_id: u64) -> Result<()> {
    let (weak, window_handle) = cx.update(|cx| {
        let registry = cx
            .try_global::<TerminalViewRegistry>()
            .ok_or_else(|| anyhow!("终端注册表未初始化"))?;
        registry
            .get(terminal_id)
            .ok_or_else(|| anyhow!("终端 #{terminal_id} 不存在或已关闭"))
    })?;
    let window_handle =
        window_handle.ok_or_else(|| anyhow!("终端 #{terminal_id} 窗口句柄不可用"))?;
    cx.update_window(window_handle, |_, window, cx| {
        window.activate_window();
        if let Some(view) = weak.upgrade() {
            view.update(cx, |view, cx| view.request_focus(window, cx));
        }
    })?;
    Ok(())
}

/// 高危命令确认：弹模态对话框；无法弹窗或超时未响应时拒绝（fail-closed）。
///
/// 优先使用目标终端所在窗口；用户 120s 未响应按拒绝处理并主动关闭对话框，
/// 防止串行泵被永久冻结。
async fn confirm_risk(
    cx: &mut AsyncApp,
    terminal_id: u64,
    command: &str,
    level: RiskLevel,
) -> bool {
    let window_handle = cx.update(|cx| {
        cx.try_global::<TerminalViewRegistry>()
            .and_then(|registry| registry.get(terminal_id))
            .and_then(|(_, handle)| handle)
            .or_else(|| cx.active_window())
    });
    let Some(window_handle) = window_handle else {
        tracing::warn!("[agent_bridge] 无可用窗口，高危命令确认请求被拒绝: {command}");
        return false;
    };

    let (tx, rx) = oneshot::channel();
    let slot = Rc::new(RiskReplySlot::new(tx));
    let opened = cx.update_window(window_handle, |_, window, cx| {
        open_risk_dialog(window, cx, command, level, slot);
    });
    if let Err(err) = opened {
        tracing::warn!("[agent_bridge] 高危命令确认对话框打开失败: {err}");
        return false;
    }

    let timer = cx.background_executor().timer(CONFIRM_TIMEOUT);
    tokio::pin!(timer);
    tokio::select! {
        result = rx => result.unwrap_or(false),
        _ = &mut timer => {
            tracing::warn!("[agent_bridge] 高危命令确认超时（120s），按拒绝处理: {command}");
            // close_dialog 关闭的是该窗口栈顶对话框：极端情况下用户 120s 内打开了
            // 另一个对话框会误关栈顶（命令已按超时拒绝，方向 fail-safe）
            let _ = cx.update_window(window_handle, |_, window, cx| {
                window.close_dialog(cx);
            });
            false
        }
    }
}

/// 打开确认对话框；ok/cancel 之外的所有关闭路径由 RiskReplySlot 的 Drop 兜底为拒绝。
fn open_risk_dialog(
    window: &mut gpui::Window,
    cx: &mut App,
    command: &str,
    level: RiskLevel,
    slot: Rc<RiskReplySlot>,
) {
    let title = t!("AgentBridge.risk_confirm_title").to_string();
    let level_label = match level {
        RiskLevel::Low => t!("AgentBridge.risk_level_low"),
        RiskLevel::Medium => t!("AgentBridge.risk_level_medium"),
        RiskLevel::High => t!("AgentBridge.risk_level_high"),
        RiskLevel::Critical => t!("AgentBridge.risk_level_critical"),
    };
    let message = t!("AgentBridge.risk_confirm_message", level = level_label).to_string();
    let preview = truncate_lines(command, 10);

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let slot_ok = slot.clone();
        let slot_cancel = slot.clone();
        dialog
            .title(title.clone())
            .confirm()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_sm().child(message.clone()))
                    .child(
                        div()
                            .max_h(px(180.0))
                            .overflow_hidden()
                            .text_xs()
                            .child(preview.clone()),
                    )
                    .into_any_element(),
            )
            .button_props(
                DialogButtonProps::default()
                    .ok_text(t!("AgentBridge.risk_confirm_ok"))
                    .cancel_text(t!("AgentBridge.risk_confirm_cancel")),
            )
            .on_ok(move |_event, _window, _cx| {
                slot_ok.resolve(true);
                true
            })
            .on_cancel(move |_event, _window, _cx| {
                slot_cancel.resolve(false);
                true
            })
    });
}

/// 截断多行文本到指定行数（对话框预览用）。
fn truncate_lines(text: &str, max_lines: usize) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines {
        return text.to_string();
    }
    lines.truncate(max_lines);
    format!("{}\n...", lines.join("\n"))
}

/// 确认结果回执槽：ok/cancel 显式回传；其他关闭路径 Drop 时兜底拒绝。
struct RiskReplySlot {
    tx: RefCell<Option<oneshot::Sender<bool>>>,
}

impl RiskReplySlot {
    fn new(tx: oneshot::Sender<bool>) -> Self {
        Self {
            tx: RefCell::new(Some(tx)),
        }
    }

    fn resolve(&self, approved: bool) {
        if let Some(tx) = self.tx.borrow_mut().take() {
            let _ = tx.send(approved);
        }
    }
}

impl Drop for RiskReplySlot {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.borrow_mut().take() {
            let _ = tx.send(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_agent_command_appends_carriage_return() {
        let data = wrap_agent_command("ls -la", TermMode::empty());
        assert_eq!(data, "ls -la\r");
    }

    #[test]
    fn wrap_agent_command_normalizes_crlf() {
        let data = wrap_agent_command("echo a\r\necho b\r", TermMode::empty());
        assert_eq!(data, "echo a\necho b\r");
    }

    #[test]
    fn wrap_agent_command_bracketed_paste_wraps() {
        let data = wrap_agent_command("ls -la", TermMode::BRACKETED_PASTE);
        assert_eq!(data, "\u{1b}[200~ls -la\u{1b}[201~\r");
    }

    #[test]
    fn wrap_agent_command_bracketed_paste_strips_inner_escape() {
        let data = wrap_agent_command("ls\u{1b}[201~evil", TermMode::BRACKETED_PASTE);
        // 内层 ESC 被剥离，只剩首尾包装序列
        assert_eq!(data.matches('\u{1b}').count(), 2);
    }

    #[test]
    fn clamp_wait_bounds() {
        assert_eq!(clamp_wait(0), MIN_WAIT);
        assert_eq!(clamp_wait(1_500), Duration::from_millis(1_500));
        assert_eq!(clamp_wait(999_999), MAX_WAIT);
    }

    #[test]
    fn truncate_lines_keeps_short_text() {
        assert_eq!(truncate_lines("a\nb", 10), "a\nb");
        let long = (0..20)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let shortened = truncate_lines(&long, 10);
        assert_eq!(shortened.lines().count(), 11);
    }
}

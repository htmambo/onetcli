//! AI 终端操作员桥接层。
//!
//! Agent 运行在 tokio 任务中，无法直接触碰 GPUI 实体；
//! 所有终端操作经 `mpsc` 命令通道发往主线程泵（`pump`）执行，
//! 结果通过 `oneshot` 回传。

mod pump;

use std::sync::Arc;

use anyhow::{Result, anyhow};
use gpui::{App, Global};
use tokio::sync::{mpsc, oneshot};

use crate::registry::{TerminalInfo, TerminalViewRegistry};

/// AgentContext capability 键：终端侧栏注入 `TerminalOperatorHandle`。
pub const CAP_TERMINAL: &str = "terminal";

const BRIDGE_CHANNEL_CAPACITY: usize = 64;

/// 写入命令的执行结果。
#[derive(Debug, Clone)]
pub struct WriteOutcome {
    /// 等待结束后的屏幕尾部输出（可能截断）。
    pub output: String,
    /// 是否因超时中断等待（命令可能仍在运行或进入交互模式）。
    pub timed_out: bool,
    /// 写入命令前终端的总行数（含 history + screen）；用于 since_last_write 跟踪。
    pub line_count_before_write: usize,
}

/// 桥接全局状态：持有命令通道发送端。
struct TerminalBridge {
    tx: mpsc::Sender<TerminalOpRequest>,
    last_write_lines: Arc<LastWriteLineMap>,
}

/// 共享状态：每个 terminal_id 上次 write_to_terminal 成功后的总行数（用于 since_last_write）。
#[derive(Default)]
struct LastWriteLineMap {
    inner: std::sync::Mutex<std::collections::HashMap<u64, usize>>,
}

impl Global for TerminalBridge {}

/// 初始化桥接层（终端注册表 + 主线程泵），幂等。
///
/// 注意：泵任务随 App 生命周期常驻（detach），通道由全局单例持有，
/// 应用退出时任务随之销毁，无显式关闭路径。
pub fn init(cx: &mut App) {
    TerminalViewRegistry::init(cx);
    if cx.has_global::<TerminalBridge>() {
        return;
    }
    let (tx, rx) = mpsc::channel(BRIDGE_CHANNEL_CAPACITY);
    cx.set_global(TerminalBridge {
        tx,
        last_write_lines: Arc::new(LastWriteLineMap::default()),
    });
    cx.spawn(async move |cx| pump::pump_loop(cx, rx).await)
        .detach();
}

/// 获取注入 AgentContext 的操作句柄；桥接未初始化时返回 None。
pub fn operator_handle(cx: &App) -> Option<TerminalOperatorHandle> {
    let bridge = cx.try_global::<TerminalBridge>()?;
    Some(TerminalOperatorHandle {
        tx: bridge.tx.clone(),
        last_write_lines: bridge.last_write_lines.clone(),
    })
}

/// 桥接命令请求（每个变体携带 oneshot 回执）。
pub(crate) enum TerminalOpRequest {
    ListTerminals {
        reply: oneshot::Sender<TerminalListSnapshot>,
    },
    ReadOutput {
        terminal_id: u64,
        max_lines: usize,
        from_line: usize,
        reply: oneshot::Sender<Result<String>>,
    },
    WriteCommand {
        terminal_id: u64,
        command: String,
        wait_ms: u64,
        reply: oneshot::Sender<Result<WriteOutcome>>,
    },
    GetCwd {
        terminal_id: u64,
        reply: oneshot::Sender<Result<Option<String>>>,
    },
    GetSelection {
        terminal_id: u64,
        reply: oneshot::Sender<Result<Option<String>>>,
    },
    Focus {
        terminal_id: u64,
        reply: oneshot::Sender<Result<()>>,
    },
}

/// 面向 Agent 的终端摘要列表（带当前焦点）。
///
/// `focused_id` 是当前获得 GPUI 焦点的 TerminalView 的 id；
/// 若所有终端都未获得焦点则为 None。`terminals` 仍是完整列表（包含 `is_focused` 字段），
/// Agent 工具层在 `terminal_id` 缺省时应优先使用 `focused_id`。
#[derive(Debug, Clone)]
pub struct TerminalListSnapshot {
    pub terminals: Vec<TerminalInfo>,
    pub focused_id: Option<u64>,
}

/// 面向 Agent 的终端操作句柄（Send + Sync + Clone，仅持通道发送端）。
#[derive(Clone)]
pub struct TerminalOperatorHandle {
    tx: mpsc::Sender<TerminalOpRequest>,
    last_write_lines: Arc<LastWriteLineMap>,
}

impl LastWriteLineMap {
    fn get(&self, terminal_id: u64) -> Option<usize> {
        self.inner
            .lock()
            .ok()
            .and_then(|m| m.get(&terminal_id).copied())
    }
    fn set(&self, terminal_id: u64, line_count: usize) {
        if let Ok(mut m) = self.inner.lock() {
            m.insert(terminal_id, line_count);
        }
    }
}

impl TerminalOperatorHandle {
    /// 读取指定终端上次 write_to_terminal 成功时的总行数；首次 read 或从未 write 返回 None。
    pub fn last_write_line_count(&self, terminal_id: u64) -> Option<usize> {
        self.last_write_lines.get(terminal_id)
    }
    /// 写入命令成功后由 pump 调用，记录"当前总行数"作为下次 read 的起点。
    pub(crate) fn record_last_write_line_count(&self, terminal_id: u64, line_count: usize) {
        self.last_write_lines.set(terminal_id, line_count);
    }
}

impl TerminalOperatorHandle {
    /// 测试构造：直接给定通道发送端。
    #[cfg(test)]
    pub(crate) fn from_sender(tx: mpsc::Sender<TerminalOpRequest>) -> Self {
        Self {
            tx,
            last_write_lines: Arc::new(LastWriteLineMap::default()),
        }
    }

    /// 枚举当前存活终端（含当前焦点 id）。
    pub async fn list_terminals(&self) -> Result<TerminalListSnapshot> {
        self.roundtrip(|reply| TerminalOpRequest::ListTerminals { reply })
            .await
    }

    /// 读取终端屏幕与回滚内容（按行截断）。
    /// `from_line` 表示"自绝对行号 N 开始读"（0 = 终端最早一行）；
    /// 典型用法 `from_line = last_write_line_count` 实现"自上次 write 后的输出"。
    pub async fn read_output(
        &self,
        terminal_id: u64,
        max_lines: usize,
        from_line: usize,
    ) -> Result<String> {
        self.roundtrip(|reply| TerminalOpRequest::ReadOutput {
            terminal_id,
            max_lines,
            from_line,
            reply,
        })
        .await?
    }

    /// 写入命令并回车，等待执行完成后返回尾部输出。
    ///
    /// 高危命令的确认门控由桥接泵强制执行（与用户确认绑定同一命令串），
    /// 用户拒绝时返回错误。
    pub async fn write_command(
        &self,
        terminal_id: u64,
        command: String,
        wait_ms: u64,
    ) -> Result<WriteOutcome> {
        self.roundtrip(|reply| TerminalOpRequest::WriteCommand {
            terminal_id,
            command,
            wait_ms,
            reply,
        })
        .await?
    }

    /// 获取终端当前工作目录。
    pub async fn get_cwd(&self, terminal_id: u64) -> Result<Option<String>> {
        self.roundtrip(|reply| TerminalOpRequest::GetCwd { terminal_id, reply })
            .await?
    }

    /// 获取终端当前选区文本。
    pub async fn get_selection(&self, terminal_id: u64) -> Result<Option<String>> {
        self.roundtrip(|reply| TerminalOpRequest::GetSelection { terminal_id, reply })
            .await?
    }

    /// 聚焦指定终端（激活窗口 + 聚焦输入）。
    pub async fn focus(&self, terminal_id: u64) -> Result<()> {
        self.roundtrip(|reply| TerminalOpRequest::Focus { terminal_id, reply })
            .await?
    }

    /// 发送请求并等待回执；通道断开视为桥接已关闭。
    async fn roundtrip<T>(
        &self,
        build: impl FnOnce(oneshot::Sender<T>) -> TerminalOpRequest,
    ) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(build(tx))
            .await
            .map_err(|_| anyhow!("终端桥接已关闭"))?;
        rx.await.map_err(|_| anyhow!("终端桥接已关闭"))
    }
}

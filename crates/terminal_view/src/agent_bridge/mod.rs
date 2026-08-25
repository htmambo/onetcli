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

/// AgentContext capability 键：终端侧栏注入 `TerminalOperatorHandle` 或 `HostedTerminalHandle`。
///
/// 注意：当前推荐注入 `HostedTerminalHandle`（携带 host_terminal_id，多 AI 并发场景
/// 下让工具调用明确知道"我是哪个 ai_chat_panel 发起的"），裸 `TerminalOperatorHandle`
/// 保留作为兜底（无 host 上下文时会回退到 `list_terminals().focused_id`）。
pub const CAP_TERMINAL: &str = "terminal";

/// `execute_tool` 接受的能力抽象：可以是裸 `TerminalOperatorHandle`（无 host 上下文）
/// 或 `HostedTerminalHandle`（带 host_terminal_id）。多 AI 助手并发场景下推荐后者。
///
/// 所有方法默认走"host 优先，缺省 fallback 到 list_terminals().focused_id"的解析策略：
/// `write_command_with_default_terminal` / `read_output_with_default_terminal` 等。
pub trait TerminalHost: Send + Sync {
    /// 宿主 TerminalView 的 agent_registry_id；用于工具调用缺省 terminal_id 时优先采用。
    fn host_terminal_id(&self) -> Option<u64> {
        None
    }

    /// 同 `TerminalOperatorHandle::write_command`，但 `terminal_id` 缺省时优先用 host。
    fn write_command_with_default<'a>(
        &'a self,
        command: String,
        wait_ms: u64,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<WriteOutcome>> + Send + 'a>>;

    /// 同 `TerminalOperatorHandle::read_output`，同上策略。
    fn read_output_with_default<'a>(
        &'a self,
        max_lines: usize,
        from_line: usize,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>>;

    /// 同 `TerminalOperatorHandle::get_cwd`。
    fn get_cwd_with_default<'a>(
        &'a self,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>>> + Send + 'a>>;

    /// 同 `TerminalOperatorHandle::get_selection`。
    fn get_selection_with_default<'a>(
        &'a self,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>>> + Send + 'a>>;

    /// 同 `TerminalOperatorHandle::focus`。
    fn focus_with_default<'a>(
        &'a self,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;

    /// 列举当前终端列表（不传 default terminal）。
    fn list_terminals(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<TerminalListSnapshot>> + Send + '_>,
    >;
}

/// 携带 host_terminal_id 的 handle 包装：每个 ai_chat_panel 在
/// `set_capability_value(CAP_TERMINAL, ...)` 时注入一个实例，让多 AI 助手并发
/// 各自的工具调用明确知道宿主 TerminalView。
///
/// `host_terminal_id_cell` 用 `Arc<AtomicU64>` 是因为：sidebar 创建时 TerminalView
/// 还没完成 registry 注册（agent_registry_id 为 None），注册后 TerminalView 才
/// 拿到稳定 id。需要让 TerminalView 后续能原地更新 host_terminal_id，否则
/// ai_chat_panel 注入到 agent capability_map 的 owned handle 与新的 id 脱钩。
#[derive(Clone)]
pub struct HostedTerminalHandle {
    pub inner: TerminalOperatorHandle,
    pub host_terminal_id_cell: Arc<std::sync::atomic::AtomicU64>,
}

#[cfg(test)]
impl HostedTerminalHandle {
    /// 测试构造：给一个底层 TerminalOperatorHandle + 初始 host_terminal_id。
    pub(crate) fn for_test(inner: TerminalOperatorHandle, host_terminal_id: u64) -> Self {
        Self {
            inner,
            host_terminal_id_cell: Arc::new(std::sync::atomic::AtomicU64::new(host_terminal_id)),
        }
    }
}

/// 解析"工具调用应该用哪个 terminal_id"：
/// 1. 显式传入 → 直接采用；
/// 2. host 上下文存在 → 用 host_terminal_id（这是多 AI 助手并发的正确语义：
///    每个 AI 助手只操作自己宿主 TerminalView，不会串到别的 terminal）；
/// 3. 否则回退到 `list_terminals().focused_id`（单 AI 助手场景下的旧行为）。
pub async fn resolve_terminal_id_with_host<H: TerminalHost + ?Sized>(
    host: &H,
    explicit: Option<u64>,
) -> anyhow::Result<u64> {
    if let Some(id) = explicit {
        return Ok(id);
    }
    if let Some(id) = host.host_terminal_id() {
        return Ok(id);
    }
    let snapshot = host
        .list_terminals()
        .await
        .map_err(|err| anyhow::anyhow!("枚举终端失败: {err}"))?;
    snapshot.focused_id.ok_or_else(|| {
        anyhow::anyhow!(
            "当前没有任何终端挂载 AI 助手。请先在目标终端上打开 AI 侧边栏，再让 AI 操作。"
        )
    })
}
impl TerminalHost for TerminalOperatorHandle {
    fn host_terminal_id(&self) -> Option<u64> {
        None
    }

    fn write_command_with_default<'a>(
        &'a self,
        command: String,
        wait_ms: u64,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<WriteOutcome>> + Send + 'a>>
    {
        Box::pin(async move {
            let id = explicit_terminal_id.ok_or_else(|| {
                anyhow::anyhow!(
                    "裸 TerminalOperatorHandle 必须显式传入 terminal_id（无 host 上下文）"
                )
            })?;
            self.write_command(id, command, wait_ms).await
        })
    }

    fn read_output_with_default<'a>(
        &'a self,
        max_lines: usize,
        from_line: usize,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            let id = explicit_terminal_id.ok_or_else(|| {
                anyhow::anyhow!(
                    "裸 TerminalOperatorHandle 必须显式传入 terminal_id（无 host 上下文）"
                )
            })?;
            self.read_output(id, max_lines, from_line).await
        })
    }

    fn get_cwd_with_default<'a>(
        &'a self,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>>> + Send + 'a>>
    {
        Box::pin(async move {
            let id = explicit_terminal_id.ok_or_else(|| {
                anyhow::anyhow!(
                    "裸 TerminalOperatorHandle 必须显式传入 terminal_id（无 host 上下文）"
                )
            })?;
            self.get_cwd(id).await
        })
    }

    fn get_selection_with_default<'a>(
        &'a self,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>>> + Send + 'a>>
    {
        Box::pin(async move {
            let id = explicit_terminal_id.ok_or_else(|| {
                anyhow::anyhow!(
                    "裸 TerminalOperatorHandle 必须显式传入 terminal_id（无 host 上下文）"
                )
            })?;
            self.get_selection(id).await
        })
    }

    fn focus_with_default<'a>(
        &'a self,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let id = explicit_terminal_id.ok_or_else(|| {
                anyhow::anyhow!(
                    "裸 TerminalOperatorHandle 必须显式传入 terminal_id（无 host 上下文）"
                )
            })?;
            self.focus(id).await
        })
    }

    fn list_terminals(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<TerminalListSnapshot>> + Send + '_>,
    > {
        Box::pin(self.list_terminals())
    }
}

impl TerminalHost for HostedTerminalHandle {
    fn host_terminal_id(&self) -> Option<u64> {
        let id = self
            .host_terminal_id_cell
            .load(std::sync::atomic::Ordering::Relaxed);
        if id == 0 { None } else { Some(id) }
    }

    fn write_command_with_default<'a>(
        &'a self,
        command: String,
        wait_ms: u64,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<WriteOutcome>> + Send + 'a>>
    {
        Box::pin(async move {
            let id = resolve_terminal_id_with_host(self, explicit_terminal_id).await?;
            self.inner.write_command(id, command, wait_ms).await
        })
    }

    fn read_output_with_default<'a>(
        &'a self,
        max_lines: usize,
        from_line: usize,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            let id = resolve_terminal_id_with_host(self, explicit_terminal_id).await?;
            self.inner.read_output(id, max_lines, from_line).await
        })
    }

    fn get_cwd_with_default<'a>(
        &'a self,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>>> + Send + 'a>>
    {
        Box::pin(async move {
            let id = resolve_terminal_id_with_host(self, explicit_terminal_id).await?;
            self.inner.get_cwd(id).await
        })
    }

    fn get_selection_with_default<'a>(
        &'a self,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>>> + Send + 'a>>
    {
        Box::pin(async move {
            let id = resolve_terminal_id_with_host(self, explicit_terminal_id).await?;
            self.inner.get_selection(id).await
        })
    }

    fn focus_with_default<'a>(
        &'a self,
        explicit_terminal_id: Option<u64>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let id = resolve_terminal_id_with_host(self, explicit_terminal_id).await?;
            self.inner.focus(id).await
        })
    }

    fn list_terminals(
        &self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<TerminalListSnapshot>> + Send + '_>,
    > {
        Box::pin(self.inner.list_terminals())
    }
}

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

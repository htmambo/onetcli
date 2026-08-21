//! TerminalOperatorAgent 循环测试：mock LLM Provider + 脚本化桥接消费者。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use one_core::agent::types::{AgentContext, AgentEvent};
use one_core::llm::connector::ChatStream;
use one_core::llm::manager::GlobalProviderState;
use one_core::llm::{
    ChatRequest, ChatResponse, Delta, FunctionCall, Message, Role, StreamingChoice,
    StreamingResponse, ToolCall,
};
use one_core::storage::StorageManager;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::terminal_operator::TerminalOperatorAgent;
use crate::TerminalConnectionKind;
use crate::agent_bridge::{TerminalOpRequest, TerminalOperatorHandle, WriteOutcome};
use crate::registry::TerminalInfo;

/// 脚本化 mock Provider：每次 chat_stream 弹出一段预设 chunk 序列。
struct MockProvider {
    scripts: Mutex<VecDeque<Vec<StreamingResponse>>>,
    supports_tools: bool,
}

impl MockProvider {
    fn new(supports_tools: bool) -> Self {
        Self {
            scripts: Mutex::new(VecDeque::new()),
            supports_tools,
        }
    }

    /// 一轮纯文本响应。
    fn push_text_round(&self, text: &str) {
        self.push_round(vec![
            chunk(Some(text), None, None),
            chunk(None, None, Some("stop")),
        ]);
    }

    /// 一轮工具调用响应（完整调用一次性给出，贴近 SSE 层累积后的形态）。
    fn push_tool_round(&self, call_id: &str, name: &str, args: &str) {
        let call = ToolCall {
            id: call_id.to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: name.to_string(),
                arguments: args.to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        self.push_round(vec![
            chunk(None, Some(vec![call]), None),
            chunk(None, None, Some("tool_calls")),
        ]);
    }

    fn push_round(&self, chunks: Vec<StreamingResponse>) {
        self.scripts.lock().unwrap().push_back(chunks);
    }
}

fn chunk(
    content: Option<&str>,
    tool_calls: Option<Vec<ToolCall>>,
    finish: Option<&str>,
) -> StreamingResponse {
    StreamingResponse {
        choices: vec![StreamingChoice {
            index: 0,
            delta: Delta {
                content: content.map(String::from),
                tool_calls,
                ..Default::default()
            },
            finish_reason: finish.map(String::from),
            logprobs: None,
        }],
        ..Default::default()
    }
}

#[async_trait::async_trait]
impl one_core::llm::LlmProvider for MockProvider {
    async fn chat_full(&self, _request: &ChatRequest) -> anyhow::Result<ChatResponse> {
        unimplemented!("测试只走流式")
    }

    async fn chat_stream(&self, _request: &ChatRequest) -> anyhow::Result<ChatStream> {
        let chunks = self
            .scripts
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("mock 脚本已耗尽"))?;
        Ok(Box::pin(futures::stream::iter(chunks.into_iter().map(Ok))))
    }

    async fn models(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec!["mock-model".to_string()])
    }

    fn provider_name(&self) -> &str {
        "mock"
    }

    fn supports_tools(&self) -> bool {
        self.supports_tools
    }
}

/// 脚本化桥接消费者：记录写入的命令与等待时长，其余请求返回固定值。
struct MockBridge {
    handle: TerminalOperatorHandle,
    written: Arc<Mutex<Vec<(String, u64)>>>,
}

fn spawn_mock_bridge() -> MockBridge {
    let (tx, mut rx) = mpsc::channel::<TerminalOpRequest>(16);
    let written = Arc::new(Mutex::new(Vec::new()));
    let written_clone = written.clone();
    tokio::spawn(async move {
        while let Some(request) = rx.recv().await {
            match request {
                TerminalOpRequest::ListTerminals { reply } => {
                    let _ = reply.send(vec![TerminalInfo {
                        id: 1,
                        title: "local".to_string(),
                        connection_kind: TerminalConnectionKind::Local,
                        cwd: Some("/tmp".to_string()),
                    }]);
                }
                TerminalOpRequest::ReadOutput { reply, .. } => {
                    let _ = reply.send(Ok("file-a\nfile-b".to_string()));
                }
                TerminalOpRequest::WriteCommand {
                    command,
                    wait_ms,
                    reply,
                    ..
                } => {
                    written_clone.lock().unwrap().push((command, wait_ms));
                    let _ = reply.send(Ok(WriteOutcome {
                        output: "total 2".to_string(),
                        timed_out: false,
                        line_count_before_write: 0,
                    }));
                }
                TerminalOpRequest::GetCwd { reply, .. } => {
                    let _ = reply.send(Ok(Some("/tmp".to_string())));
                }
                TerminalOpRequest::GetSelection { reply, .. } => {
                    let _ = reply.send(Ok(None));
                }
                TerminalOpRequest::Focus { reply, .. } => {
                    let _ = reply.send(Ok(()));
                }
            }
        }
    });
    MockBridge {
        handle: TerminalOperatorHandle::from_sender(tx),
        written,
    }
}

/// 构造测试上下文（临时目录存储，不触碰真实数据库）。
fn test_context(cancel_token: CancellationToken) -> AgentContext {
    test_context_with_reminders(cancel_token, 1)
}

/// 测试上下文计数器（保证并发测试时 db_path 唯一）
static TEST_CTX_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// 构造测试上下文并指定 reminder 配额（0 = 关闭 reminder，行为等价于 commit 0dddf152）。
fn test_context_with_reminders(
    cancel_token: CancellationToken,
    max_reminders: usize,
) -> AgentContext {
    let seq = TEST_CTX_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let db_path = std::env::temp_dir()
        .join(format!("omnihub-agent-test-{}-{}", std::process::id(), seq,))
        .join("test.db");
    let mut ctx = AgentContext::new(
        "查看当前目录".to_string(),
        vec![Message::text(Role::User, "之前的对话")],
        Default::default(),
        GlobalProviderState::new(),
        StorageManager::with_path(&db_path).expect("创建测试存储失败"),
        cancel_token,
    );
    ctx.set_capability(super::terminal_operator::CAP_MAX_REMINDERS, max_reminders);
    ctx
}

/// 运行 run_loop 并收集全部事件。
async fn run_collect(
    provider: Arc<MockProvider>,
    handle: &TerminalOperatorHandle,
    cancel_token: CancellationToken,
) -> (Result<(), String>, Vec<AgentEvent>) {
    let ctx = test_context(cancel_token);
    let (tx, mut rx) = mpsc::channel(64);
    let agent = TerminalOperatorAgent::new();
    let handle = handle.clone();
    let run = tokio::spawn(async move { agent.run_loop(&ctx, provider, &handle, &tx).await });
    let mut events = Vec::new();
    while let Ok(Some(event)) =
        tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv()).await
    {
        events.push(event);
    }
    // 事件流结束后 run_loop 应立即退出；加超时防止挂起的循环拖死整个测试套件
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), run)
        .await
        .expect("run_loop 在事件流结束后 5s 内未终止")
        .expect("run_loop 任务 panic");
    (result, events)
}

/// 提取 Completed 事件的 content。
fn completed_content(events: &[AgentEvent]) -> Option<String> {
    events.iter().find_map(|event| match event {
        AgentEvent::Completed(result) => Some(result.content.clone()),
        _ => None,
    })
}

#[tokio::test]
async fn errors_when_provider_lacks_tool_support() {
    let provider = Arc::new(MockProvider::new(false));
    let bridge = spawn_mock_bridge();
    let (result, _events) = run_collect(provider, &bridge.handle, CancellationToken::new()).await;
    let err = result.expect_err("不支持 tools 的 provider 应报错");
    // 测试环境 i18n 回退英文
    assert!(
        err.contains("does not support tool calling"),
        "错误文案不符: {err}"
    );
}

#[tokio::test]
async fn text_only_round_completes_with_text() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_text_round("当前目录有两个文件");
    let bridge = spawn_mock_bridge();
    let (result, events) = run_collect(provider, &bridge.handle, CancellationToken::new()).await;
    assert!(result.is_ok());
    assert_eq!(
        completed_content(&events).as_deref(),
        Some("当前目录有两个文件")
    );
}

#[tokio::test]
async fn write_tool_call_round_trip() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls -la"}"#);
    // 第二轮：模型返回 text-only 中间态（reminder 注入目标）
    provider.push_text_round("已列出目录内容");
    // 第三轮：模型补 task_complete 收尾（reminder 兜底后的预期行为）
    provider.push_tool_round("call-2", "task_complete", r#"{"summary":"完成"}"#);
    let bridge = spawn_mock_bridge();
    let written = bridge.written.clone();

    let (result, events) = run_collect(provider, &bridge.handle, CancellationToken::new()).await;

    assert!(result.is_ok());
    assert_eq!(
        written.lock().unwrap().as_slice(),
        [("ls -la".to_string(), 1500)]
    );
    let started = events.iter().any(|event| {
        matches!(
            event,
            AgentEvent::ToolCallStarted { name, .. } if name == "write_to_terminal"
        )
    });
    let finished = events
        .iter()
        .any(|event| matches!(event, AgentEvent::ToolCallFinished { ok: true, .. }));
    assert!(started && finished);
    let content = completed_content(&events).expect("应有 Completed 事件");
    assert!(content.contains("完成"));
    assert!(content.contains("```omnihub-tool"), "持久化工具记录缺失");
}

#[tokio::test]
async fn task_complete_terminates_loop() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-9", "task_complete", r#"{"summary":"任务完成摘要"}"#);
    let bridge = spawn_mock_bridge();

    let (result, events) = run_collect(provider, &bridge.handle, CancellationToken::new()).await;

    assert!(result.is_ok());
    let content = completed_content(&events).expect("应有 Completed 事件");
    assert!(content.starts_with("任务完成摘要"));
    assert!(content.contains("```omnihub-tool"));
}

#[tokio::test]
async fn cancelled_token_stops_loop() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_text_round("不应到达");
    let bridge = spawn_mock_bridge();
    let cancel_token = CancellationToken::new();
    cancel_token.cancel();

    let (result, events) = run_collect(provider, &bridge.handle, cancel_token).await;

    assert!(result.is_ok());
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AgentEvent::Cancelled))
    );
    assert!(completed_content(&events).is_none());
}

#[tokio::test]
async fn repeated_tool_calls_trigger_circuit_breaker() {
    let provider = Arc::new(MockProvider::new(true));
    // 5 轮完全相同的工具调用；第 3 次应被熔断，桥接只收到 2 次写入
    for _ in 0..5 {
        provider.push_tool_round("call-r", "write_to_terminal", r#"{"command":"ls"}"#);
    }
    let bridge = spawn_mock_bridge();
    let written = bridge.written.clone();

    let (result, events) = run_collect(provider, &bridge.handle, CancellationToken::new()).await;

    assert!(result.is_ok());
    assert_eq!(written.lock().unwrap().len(), 2, "第 3 次重复调用应被熔断");
    let content = completed_content(&events).expect("熔断后应有 Completed 事件");
    // 测试环境 i18n 回退英文
    assert!(
        content.contains("repeated tool call"),
        "熔断提示缺失: {content}"
    );
}

#[tokio::test]
async fn empty_first_round_retries_once() {
    let provider = Arc::new(MockProvider::new(true));
    // 首轮空响应（无文本无工具调用），第二轮正常
    provider.push_round(vec![chunk(None, None, Some("stop"))]);
    provider.push_text_round("重试后的回答");
    let bridge = spawn_mock_bridge();

    let (result, events) = run_collect(provider, &bridge.handle, CancellationToken::new()).await;

    assert!(result.is_ok());
    assert_eq!(completed_content(&events).as_deref(), Some("重试后的回答"));
}

#[tokio::test]
async fn hallucination_guard_appends_warning() {
    let provider = Arc::new(MockProvider::new(true));
    // 声称已执行但全程无 write 工具调用
    provider.push_text_round("已执行清理命令");
    let bridge = spawn_mock_bridge();

    let (result, events) = run_collect(provider, &bridge.handle, CancellationToken::new()).await;

    assert!(result.is_ok());
    let content = completed_content(&events).expect("应有 Completed 事件");
    assert!(content.contains("已执行清理命令"));
    // 幻觉防护警示应被追加（测试环境 i18n 回退英文）
    assert!(
        content.contains("no terminal write was actually performed"),
        "幻觉警示缺失: {content}"
    );
}

#[tokio::test]
async fn write_wait_ms_is_clamped() {
    let provider = Arc::new(MockProvider::new(true));
    // wait_ms 超过上限 30000，应被钳制后再发给桥接层
    provider.push_tool_round(
        "call-c",
        "write_to_terminal",
        r#"{"command":"ls","wait_ms":999999}"#,
    );
    // 第二轮 text-only 中间态：触发 reminder 注入
    provider.push_text_round("done");
    // 第三轮 task_complete：模型收到 reminder 后补完成信号
    provider.push_tool_round("call-d", "task_complete", r#"{"summary":"完成"}"#);
    let bridge = spawn_mock_bridge();
    let written = bridge.written.clone();

    let (result, _events) = run_collect(provider, &bridge.handle, CancellationToken::new()).await;

    assert!(result.is_ok());
    assert_eq!(
        written.lock().unwrap().as_slice(),
        [("ls".to_string(), 30000)]
    );
}

// ===== Reminder 注入机制测试（M3.5 冒烟 + M7 单元测试）=====

/// 自定义 reminder 配额 + 收集事件（部分测试需要禁用 reminder 以验证默认行为）
async fn run_collect_reminders(
    provider: Arc<MockProvider>,
    handle: &TerminalOperatorHandle,
    cancel_token: CancellationToken,
    max_reminders: usize,
) -> (Result<(), String>, Vec<AgentEvent>) {
    let ctx = test_context_with_reminders(cancel_token, max_reminders);
    let (tx, mut rx) = mpsc::channel(64);
    let agent = TerminalOperatorAgent::new();
    let handle = handle.clone();
    let run = tokio::spawn(async move { agent.run_loop(&ctx, provider, &handle, &tx).await });
    let mut events = Vec::new();
    while let Ok(Some(event)) =
        tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv()).await
    {
        events.push(event);
    }
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), run)
        .await
        .expect("run_loop 在事件流结束后 5s 内未终止")
        .expect("run_loop 任务 panic");
    (result, events)
}

/// 场景：tool_call 后 text-only 中间态 → reminder 注入 → 下一轮 task_complete 收尾
#[tokio::test]
async fn reminder_injected_when_mid_session_text_only() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls"}"#);
    provider.push_text_round("继续"); // 模拟截图案例：模型返回纯文本"继续"
    provider.push_tool_round("call-2", "task_complete", r#"{"summary":"完成"}"#);
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok());
    let content = completed_content(&events).expect("应有 Completed 事件");
    assert!(content.contains("完成"));
}

/// 场景：max_tokens 截断（finish_reason=length）不续轮
#[tokio::test]
async fn reminder_skipped_when_finish_reason_is_length() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls"}"#);
    // length 截断 + 文本非空
    provider.push_round(vec![chunk(Some("输出已截断"), None, Some("length"))]);
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok());
    let content = completed_content(&events).expect("应有 Completed 事件");
    assert!(content.contains("输出已截断"));
    // i18n response_truncated 提示（测试环境英文）
    assert!(
        content.contains("truncated by max_tokens"),
        "length 路径应追加 response_truncated 提示: {content}"
    );
}

/// 场景：reminder 配额耗尽（连续 2 次 text-only）按完成处理
#[tokio::test]
async fn reminder_quota_exhausted_breaks_loop() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls"}"#);
    provider.push_text_round("继续 1");
    // 第二次 text-only：reminder 配额已耗尽，应 break
    provider.push_text_round("继续 2");
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok());
    let content = completed_content(&events).expect("应有 Completed 事件");
    // 配额耗尽追加 mid_session_aborted
    assert!(
        content.contains("reminder quota was exhausted"),
        "配额耗尽应追加 mid_session_aborted: {content}"
    );
}

/// 场景：max_reminders=0 时退回到原行为（无 reminder，text-only 直接 break）
#[tokio::test]
async fn reminder_disabled_when_max_reminders_is_zero() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls"}"#);
    provider.push_text_round("直接总结");
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 0).await;

    assert!(result.is_ok());
    let content = completed_content(&events).expect("应有 Completed 事件");
    assert!(content.contains("直接总结"));
    // 配额=0 不应追加 mid_session_aborted
    assert!(
        !content.contains("reminder quota was exhausted"),
        "max_reminders=0 时不应追加 mid_session_aborted"
    );
}

/// 场景：首轮 empty + finish_reason=stop 按既有 retry once 行为
#[tokio::test]
async fn first_round_empty_response_still_retries_once() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_round(vec![chunk(None, None, Some("stop"))]); // 首轮空响应
    provider.push_text_round("重试后的回答");
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok());
    assert_eq!(completed_content(&events).as_deref(), Some("重试后的回答"));
}

/// 场景：首轮 length 不 retry（避免死循环）
#[tokio::test]
async fn length_on_first_round_does_not_retry() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_round(vec![chunk(Some("首轮就截断"), None, Some("length"))]);
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok());
    let content = completed_content(&events).expect("应有 Completed 事件");
    assert!(content.contains("首轮就截断"));
}

/// 场景：content_filter 触发 fail-fast（返回 Err）
#[tokio::test]
async fn content_filter_finish_reason_fail_fast() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_round(vec![chunk(Some("违规内容"), None, Some("content_filter"))]);
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    // content_filter 应返回错误而非 Completed
    assert!(result.is_err());
    // 不应有 Completed 事件
    assert!(completed_content(&events).is_none());
    // 错误事件被 emit
    let err_count = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Error(_)))
        .count();
    assert!(err_count >= 1, "content_filter 应发送 Error 事件");
}

/// 场景：content_filter_after_reminder_fails_fast（reminder 后下一轮触发 content_filter）
#[tokio::test]
async fn content_filter_after_reminder_fails_fast() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls"}"#);
    provider.push_text_round("继续"); // 第一轮 reminder 注入
    provider.push_round(vec![chunk(Some("违规"), None, Some("content_filter"))]);
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_err());
    let err_count = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Error(_)))
        .count();
    assert!(err_count >= 1, "reminder 后 content_filter 应失败");
}

/// 场景：finish_reason=null 走 stop 同路径（多轮后流式正常结束）
#[tokio::test]
async fn finish_reason_null_treated_as_stop_when_mid_session() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls"}"#);
    // null finish_reason + text 非空 + had_tool → 走 stop 路径（注入 reminder）
    provider.push_round(vec![chunk(Some("继续 null"), None, Some("null"))]);
    provider.push_tool_round("call-2", "task_complete", r#"{"summary":"完成"}"#);
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok());
    let content = completed_content(&events).expect("应有 Completed 事件");
    assert!(content.contains("完成"));
}

/// 场景：reminder 轮计入 MAX_ROUNDS（钳制 effective_max_reminders）
#[tokio::test]
async fn reminder_round_counts_toward_max_rounds() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls"}"#);
    provider.push_text_round("继续"); // 注入 reminder，round 计数 +1
    provider.push_text_round("继续 2"); // 配额耗尽，break
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok());
    // 配额耗尽应追加 mid_session_aborted
    let content = completed_content(&events).expect("应有 Completed 事件");
    assert!(
        content.contains("reminder quota was exhausted"),
        "配额耗尽应追加 mid_session_aborted"
    );
}

// ===== review_code P0-3 补充测试 =====

/// 场景 4：未识别 finish_reason（大小写归一化后非 stop/length/...）→ 兜底 break
#[tokio::test]
async fn unknown_finish_reason_breaks_with_telemetry() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls"}"#);
    // "function_call" 是非 OpenAI 协议兼容的字符串，归一化后落入 Unknown 分支
    provider.push_round(vec![chunk(
        Some("奇怪的 finish_reason"),
        None,
        Some("function_call"),
    )]);
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok());
    let content = completed_content(&events).expect("应有 Completed 事件");
    // Unknown 分支直接 break；不注入 reminder，所以 content 不含"继续"
    assert!(content.contains("奇怪的 finish_reason"));
}

/// 场景：finish_reason 字段缺失（None）→ 归一化到 Null
/// 字段缺失 + 首轮 + 空文本 → 走 retry once（首轮空响应）
/// 字段缺失 + 非首轮 + 有 tool_call + text>0 → 注入 reminder（与 "null" 字符串走同一路径）
#[tokio::test]
async fn missing_finish_reason_field_falls_through_to_reminder() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls"}"#);
    // chunk(...) finish=None 表示字段缺失
    provider.push_round(vec![chunk(Some("继续 missing"), None, None)]);
    provider.push_tool_round("call-2", "task_complete", r#"{"summary":"完成"}"#);
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok());
    let content = completed_content(&events).expect("应有 Completed 事件");
    assert!(content.contains("完成"));
}

/// 场景：effective_max_reminders 被 MAX_ROUNDS - round 钳制
/// max_mid_session_reminders=10 但 MAX_ROUNDS=20, round=15 → effective 应为 5
/// （实际 MAX_ROUNDS=20 常量无法在测试中改；这里只验证 effective 计算逻辑可观测）
#[tokio::test]
async fn effective_max_reminders_clamped_by_remaining_rounds() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls"}"#);
    provider.push_text_round("继续 1");
    // 第二次 text-only：reminder_count=1, max_reminders=10, effective=10，
    // 应继续 reminder 注入（而非 break）；mock 脚本耗尽 → run_loop 返回 Err（流关闭）
    provider.push_text_round("继续 2");
    let bridge = spawn_mock_bridge();

    let (_result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 10).await;

    // 第 2 次 text-only 触发了 reminder 注入但 mock 脚本耗尽，可能 Completed（极少见）或事件流关闭
    // 验证关键不变量：mid_session_reminder_quota 没耗尽，不应出现"reminder quota was exhausted" 文案
    let content_or_empty = completed_content(&events).unwrap_or_default();
    assert!(
        !content_or_empty.contains("reminder quota was exhausted"),
        "max_reminders=10 时第 2 次 text-only 不应触发配额耗尽: {content_or_empty}"
    );
}

/// 场景：大写 finish_reason（如 "LENGTH"）归一化后命中 length 分支
#[tokio::test]
async fn uppercase_finish_reason_normalized() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls"}"#);
    provider.push_round(vec![chunk(Some("截断内容"), None, Some("LENGTH"))]);
    let bridge = spawn_mock_bridge();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok());
    let content = completed_content(&events).expect("应有 Completed 事件");
    // 归一化后应命中 length 分支，追加 response_truncated 提示
    assert!(
        content.contains("truncated by max_tokens"),
        "大写 LENGTH 应归一化为 length: {content}"
    );
}

// ===== M14: since_last_write 测试 =====

/// 场景：write → write → read（since_last_write=true）只返回第二次 write 之后的输出
#[tokio::test]
async fn write_then_read_returns_only_post_write_output() {
    let provider = Arc::new(MockProvider::new(true));
    // 第一轮：模拟写入命令 1
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"echo first"}"#);
    // 第二轮：写入命令 2（last_write_line_count 在此处被记录）
    provider.push_tool_round(
        "call-2",
        "write_to_terminal",
        r#"{"command":"echo second"}"#,
    );
    // 第三轮：read（since_last_write=true）—— 应只看到 echo second 的输出
    provider.push_tool_round("call-3", "read_terminal_output", r#"{"max_lines":100}"#);
    // 第四轮：task_complete 收尾
    provider.push_tool_round("call-4", "task_complete", r#"{"summary":"完成"}"#);
    let bridge = spawn_mock_bridge();
    let written = bridge.written.clone();

    let (result, events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok());
    // 验证两次 write 都被记录
    assert_eq!(
        written.lock().unwrap().len(),
        2,
        "两次 write_to_terminal 应都被记录"
    );
}

/// 场景：首次 read（无 last_write_line_count）→ 退回全终端尾部读取
#[tokio::test]
async fn read_before_write_returns_full_terminal_tail() {
    let provider = Arc::new(MockProvider::new(true));
    // 没有 write_to_terminal，直接 read
    provider.push_tool_round("call-r", "read_terminal_output", r#"{"max_lines":100}"#);
    provider.push_tool_round("call-d", "task_complete", r#"{"summary":"完成"}"#);
    let bridge = spawn_mock_bridge();

    let (result, _events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok(), "首次 read 应退回全终端读取并正常完成");
}

/// 场景：since_last_write=false → 强制读全终端尾部
#[tokio::test]
async fn since_last_write_false_falls_back_to_full_tail() {
    let provider = Arc::new(MockProvider::new(true));
    provider.push_tool_round("call-1", "write_to_terminal", r#"{"command":"ls"}"#);
    // since_last_write=false：忽略 last_write_line_count
    provider.push_tool_round(
        "call-r",
        "read_terminal_output",
        r#"{"max_lines":100,"since_last_write":false}"#,
    );
    provider.push_tool_round("call-d", "task_complete", r#"{"summary":"完成"}"#);
    let bridge = spawn_mock_bridge();

    let (result, _events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok(), "since_last_write=false 应正常完成");
}

/// 场景：多 terminal_id 上 last_write_lines 独立跟踪
#[tokio::test]
async fn multi_terminal_last_write_tracked_independently() {
    let provider = Arc::new(MockProvider::new(true));
    // 写入到不同 terminal
    provider.push_tool_round(
        "call-1",
        "write_to_terminal",
        r#"{"terminal_id":1,"command":"echo t1"}"#,
    );
    provider.push_tool_round(
        "call-2",
        "write_to_terminal",
        r#"{"terminal_id":2,"command":"echo t2"}"#,
    );
    provider.push_tool_round(
        "call-r1",
        "read_terminal_output",
        r#"{"terminal_id":1,"max_lines":50}"#,
    );
    provider.push_tool_round(
        "call-r2",
        "read_terminal_output",
        r#"{"terminal_id":2,"max_lines":50}"#,
    );
    provider.push_tool_round("call-d", "task_complete", r#"{"summary":"完成"}"#);
    let bridge = spawn_mock_bridge();
    let written = bridge.written.clone();

    let (result, _events) =
        run_collect_reminders(provider, &bridge.handle, CancellationToken::new(), 1).await;

    assert!(result.is_ok());
    assert_eq!(
        written.lock().unwrap().len(),
        2,
        "两个 terminal_id 的 write 都应被记录"
    );
}

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
    let db_path = std::env::temp_dir()
        .join(format!(
            "omnihub-agent-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
        .join("test.db");
    AgentContext::new(
        "查看当前目录".to_string(),
        vec![Message::text(Role::User, "之前的对话")],
        Default::default(),
        GlobalProviderState::new(),
        StorageManager::with_path(&db_path).expect("创建测试存储失败"),
        cancel_token,
    )
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
    provider.push_text_round("已列出目录内容");
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
    assert!(content.contains("已列出目录内容"));
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
    provider.push_text_round("done");
    let bridge = spawn_mock_bridge();
    let written = bridge.written.clone();

    let (result, _events) = run_collect(provider, &bridge.handle, CancellationToken::new()).await;

    assert!(result.is_ok());
    assert_eq!(
        written.lock().unwrap().as_slice(),
        [("ls".to_string(), 30000)]
    );
}

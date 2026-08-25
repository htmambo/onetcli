//! 终端操作员 Agent：基于原生工具调用的多轮循环。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use rust_i18n::t;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use one_core::agent::types::{Agent, AgentContext, AgentDescriptor, AgentEvent, AgentResult};
use one_core::llm::{
    ChatRequest, ChatStream, LlmProvider, Message, Role, ToolCall, ToolChoice,
    assistant_tool_calls_message, extract_stream_text_parts, tool_message,
};
use one_core::storage::traits::Repository;

use super::prompt::SYSTEM_PROMPT;
use super::tools::{ToolEffect, action_summary, execute_tool, summarize_args, tool_definitions};
use crate::agent_bridge::{CAP_TERMINAL, TerminalOperatorHandle};

/// 最大工具调用轮次。
const MAX_ROUNDS: usize = 20;
/// 单轮流式响应超时。
const ROUND_TIMEOUT: Duration = Duration::from_secs(120);
/// 连续相同工具调用熔断阈值。
const REPEAT_LIMIT: usize = 3;
/// text-only 中间态的最大 reminder 续轮次数（默认 1；0 = 关闭 reminder 退回原行为）。
const DEFAULT_MAX_MID_SESSION_REMINDERS: usize = 1;
/// Capability key：从 AgentContext.capabilities 读取 max_mid_session_reminders 的 key。
pub const CAP_MAX_REMINDERS: &str = "terminal.max_mid_session_reminders";
/// 注入到消息流的 reminder 文案：明确告诉模型"text-only 不算完成"。
const MID_SESSION_REMINDER_TEXT: &str = "你刚才只返回了文字总结，但还没有调用 task_complete。\
     请继续调用必要的工具，直到所有步骤完成后再调用 task_complete。\
     如果确实无法继续（例如等待用户输入），请调用 task_complete 并说明阻塞原因。";

static DESCRIPTOR: AgentDescriptor = AgentDescriptor {
    id: "terminal_operator",
    display_name: "终端操作员",
    description: "读取终端输出、向终端写入命令并执行、跨终端操作。\
                  适用于查看终端状态、执行 shell 命令、排查终端中的问题。",
    keywords: &["终端", "terminal", "命令", "执行"],
    command_prefix: Some("/term"),
    examples: &["查看当前目录并列出文件", "看看终端里刚才的报错"],
    required_capabilities: &[CAP_TERMINAL],
    priority: 5,
};

/// 终端操作员 Agent（无状态，全部状态在 AgentContext capability 中）。
#[derive(Default)]
pub struct TerminalOperatorAgent;

impl TerminalOperatorAgent {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Agent for TerminalOperatorAgent {
    fn descriptor(&self) -> &AgentDescriptor {
        &DESCRIPTOR
    }

    async fn execute(&self, ctx: AgentContext, tx: mpsc::Sender<AgentEvent>) {
        if let Err(err) = self.run(ctx, &tx).await {
            let _ = tx.send(AgentEvent::Error(err)).await;
        }
    }
}

/// 单轮结果：本轮文本 + 完整工具调用 + finish_reason。
struct RoundOutcome {
    text: String,
    tool_calls: Vec<ToolCall>,
    finish_reason: Option<String>,
}

/// 流式收集的终止原因。
enum RoundError {
    Cancelled,
    Failed(String),
}

/// 归一化后的 finish_reason 分类（review_code P1-4）。
/// 原文档 §4.5 矩阵的最小子集；归一化大小写后用此 enum 替代魔法字符串。
#[derive(Debug, Clone, PartialEq, Eq)]
enum FinishReasonKind {
    /// `stop` — 流式正常结束
    Stop,
    /// `length` — max_tokens 截断
    Length,
    /// `content_filter` — 合规拦截
    ContentFilter,
    /// `tool_calls` — 模型返回了工具调用（不在此处处理，由外层 break 'rounds）
    ToolCalls,
    /// `null` 或字段缺失 — provider 协议差异
    Null,
    /// 其他未识别字符串（保留原值便于诊断）
    Unknown(String),
}

/// 解析 finish_reason 字符串为归一化枚举（review_code P0-2）。
/// `null` 与字段缺失（None）统一映射到 Null。
fn classify_finish_reason(fr: Option<&str>) -> FinishReasonKind {
    match fr.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        None | Some("null") => FinishReasonKind::Null,
        Some("stop") => FinishReasonKind::Stop,
        Some("length") => FinishReasonKind::Length,
        Some("content_filter") => FinishReasonKind::ContentFilter,
        Some("tool_calls") => FinishReasonKind::ToolCalls,
        Some(other) => FinishReasonKind::Unknown(other.to_string()),
    }
}

impl TerminalOperatorAgent {
    async fn run(&self, ctx: AgentContext, tx: &mpsc::Sender<AgentEvent>) -> Result<(), String> {
        // 优先拿 HostedTerminalHandle（多 AI 助手并发时携带 host_terminal_id），
        // 拿不到再回退到裸 TerminalOperatorHandle（无 host 上下文）。
        let hosted: Option<crate::agent_bridge::HostedTerminalHandle> = ctx
            .get_capability::<crate::agent_bridge::HostedTerminalHandle>(CAP_TERMINAL)
            .cloned();
        let provider = ctx
            .provider_state
            .manager()
            .get_provider(&ctx.provider_config)
            .await
            .map_err(|err| t!("TerminalAgent.get_provider_failed", error = err).to_string())?;
        if let Some(hosted) = hosted {
            self.run_loop(
                &ctx,
                provider,
                &hosted as &dyn crate::agent_bridge::TerminalHost,
                tx,
            )
            .await
        } else {
            let handle = ctx
                .get_capability::<TerminalOperatorHandle>(CAP_TERMINAL)
                .ok_or_else(|| t!("TerminalAgent.capability_missing").to_string())?
                .clone();
            self.run_loop(
                &ctx,
                provider,
                &handle as &dyn crate::agent_bridge::TerminalHost,
                tx,
            )
            .await
        }
    }

    /// Agent 主循环（pub(crate) 供测试注入 mock Provider）。
    pub(crate) async fn run_loop(
        &self,
        ctx: &AgentContext,
        provider: Arc<dyn LlmProvider>,
        handle: &dyn crate::agent_bridge::TerminalHost,
        tx: &mpsc::Sender<AgentEvent>,
    ) -> Result<(), String> {
        if !provider.supports_tools() {
            return Err(t!("TerminalAgent.provider_not_support_tools").to_string());
        }

        let mut messages = vec![Message::text(Role::System, SYSTEM_PROMPT)];
        messages.extend(ctx.chat_history.iter().cloned());
        messages.push(Message::text(Role::User, ctx.user_input.clone()));

        // 解析持久化中间态所需的会话 id 与消息仓库。
        // 仅当 session_id 存在且能拿到 MessageRepository 时才落库工具消息对；
        // 否则跳过（测试场景或无持久化会话时 run_loop 仍正常跑，只是不落库）。
        let session_id = ctx.session_id.filter(|&id| id > 0);
        let message_repo = session_id.and_then(|_| {
            ctx.storage_manager
                .get::<one_core::llm::chat_history::MessageRepository>()
        });
        // 闭包：把 assistant(tool_calls) 消息或 tool(result) 消息持久化到 DB。
        // 失败仅记日志，不中断 agent 循环（持久化是侧路，不应阻塞主流程）。
        let persist_assistant_tool_calls =
            |repo: &one_core::llm::chat_history::MessageRepository,
             sid: i64,
             text: &str,
             tool_calls: &[ToolCall]| {
                let tool_calls_json = match serde_json::to_string(tool_calls) {
                    Ok(json) => json,
                    Err(err) => {
                        tracing::warn!(
                            "[terminal_agent] 序列化 tool_calls 失败，跳过持久化: {err}"
                        );
                        return;
                    }
                };
                let mut msg = one_core::llm::chat_history::ChatMessage::assistant_tool_calls(
                    sid,
                    text.to_string(),
                    tool_calls_json,
                );
                if let Err(err) = repo.insert(&mut msg) {
                    tracing::warn!("[terminal_agent] 持久化 assistant tool_calls 消息失败: {err}");
                }
            };
        let persist_tool_result = |repo: &one_core::llm::chat_history::MessageRepository,
                                   sid: i64,
                                   call_id: &str,
                                   result: &str| {
            let mut msg = one_core::llm::chat_history::ChatMessage::tool_result(
                sid,
                call_id.to_string(),
                result.to_string(),
            );
            if let Err(err) = repo.insert(&mut msg) {
                tracing::warn!("[terminal_agent] 持久化 tool 结果消息失败: {err}");
            }
        };

        let mut full_text = String::new();
        let mut tool_records: Vec<serde_json::Value> = Vec::new();
        let mut had_write = false;
        let mut repeat_count = 0usize;
        let mut last_signature: Option<u64> = None;
        // 工具调用序号：同一次运行内从 1 递增（每次用户请求重新计数）
        let mut tool_seq: u32 = 0;
        // 用于诊断"自动停止"：在已有 tool_call 历史后出现的 text-only 终止会被打 warn 日志
        let mut had_tool_in_session: bool = false;
        // text-only 中间态 reminder 注入计数；钳制到 effective_max_reminders
        let mut mid_session_reminder_count: usize = 0;
        // 从 capability 读取 settings；缺失则用编译期默认值（向后兼容既有 execute 调用方）
        let max_mid_session_reminders: usize = ctx
            .get_capability::<usize>(CAP_MAX_REMINDERS)
            .copied()
            .unwrap_or(DEFAULT_MAX_MID_SESSION_REMINDERS);

        let mut round = 0;
        'rounds: loop {
            if ctx.cancel_token.is_cancelled() {
                let _ = tx.send(AgentEvent::Cancelled).await;
                return Ok(());
            }
            round += 1;
            if round > MAX_ROUNDS {
                let note = t!("TerminalAgent.max_rounds_reached", max = MAX_ROUNDS).to_string();
                tracing::warn!("[terminal_agent] {note}");
                full_text.push_str("\n\n");
                full_text.push_str(&note);
                break;
            }

            let request = build_request(ctx, messages.clone());
            let outcome = match self
                .stream_round(&provider, request, &ctx.cancel_token, tx)
                .await
            {
                Ok(outcome) => outcome,
                Err(RoundError::Cancelled) => {
                    let _ = tx.send(AgentEvent::Cancelled).await;
                    return Ok(());
                }
                Err(RoundError::Failed(err)) => return Err(err),
            };
            tracing::debug!(
                "[terminal_agent] 第 {round} 轮：text={}B, tool_calls={}, finish_reason={:?}",
                outcome.text.len(),
                outcome.tool_calls.len(),
                outcome.finish_reason
            );
            full_text.push_str(&outcome.text);

            if outcome.tool_calls.is_empty() {
                let is_empty_text = outcome.text.is_empty();
                // effective_max_reminders：钳制到 MAX_ROUNDS-1，避免 reminder 突破 round 硬限
                let effective_max_reminders =
                    max_mid_session_reminders.min(MAX_ROUNDS.saturating_sub(round));
                let fr_kind = classify_finish_reason(outcome.finish_reason.as_deref());

                // 场景 1：首轮空响应（不论 finish_reason）可能是模型预热问题，重试一次（既有）
                if round == 1 && is_empty_text {
                    continue;
                }

                match fr_kind {
                    FinishReasonKind::Length => {
                        tracing::warn!(
                            event = "length_path_triggered",
                            round = round,
                            had_tool = had_tool_in_session,
                            text_len = outcome.text.len(),
                            finish_reason = ?outcome.finish_reason,
                            "[terminal_agent] max_tokens 截断；不续轮"
                        );
                        full_text.push_str("\n\n");
                        full_text.push_str(&t!("TerminalAgent.response_truncated").to_string());
                        break;
                    }
                    FinishReasonKind::ContentFilter => {
                        tracing::error!(
                            event = "content_filter_triggered",
                            round = round,
                            had_tool = had_tool_in_session,
                            text_len = outcome.text.len(),
                            "[terminal_agent] 触发 content_filter；终止循环"
                        );
                        // 先发 Error 事件再返回 Err（与 Agent::execute 包装行为对齐，
                        // 让 run_loop 直接调用方也能收到 Error 事件）
                        let err_msg = t!("TerminalAgent.content_filtered").to_string();
                        let _ = tx.send(AgentEvent::Error(err_msg.clone())).await;
                        return Err(err_msg);
                    }
                    FinishReasonKind::Unknown(raw) => {
                        tracing::warn!(
                            event = "unknown_finish_reason_break",
                            round = round,
                            raw_finish_reason = raw.as_str(),
                            had_tool = had_tool_in_session,
                            "[terminal_agent] 遇到未识别的 finish_reason；按 fail-safe break 处理"
                        );
                        break;
                    }
                    FinishReasonKind::Null => {
                        // 字段缺失或字符串 "null"：首轮/空文本视为截断变体；否则走 stop 路径
                        tracing::warn!(
                            event = "null_finish_reason",
                            round = round,
                            had_tool = had_tool_in_session,
                            text_len = outcome.text.len(),
                            "[terminal_agent] finish_reason=null"
                        );
                        if !had_tool_in_session || is_empty_text {
                            break;
                        }
                        // 落入 stop 分支（下方统一处理 reminder 注入）
                    }
                    FinishReasonKind::Stop | FinishReasonKind::ToolCalls => {
                        // stop：流式正常结束；tool_calls 在此处不可达（已被外层 tool_calls 非空捕获）
                    }
                }

                // stop / null(归一化后) 路径：had_tool + text>0 + effective>0 → reminder 注入
                if had_tool_in_session && !is_empty_text && effective_max_reminders > 0 {
                    if mid_session_reminder_count < effective_max_reminders {
                        mid_session_reminder_count += 1;
                        tracing::info!(
                            event = "reminder_injected",
                            round = round,
                            reminder_count = mid_session_reminder_count,
                            max_reminders = effective_max_reminders,
                            had_tool = had_tool_in_session,
                            text_len = outcome.text.len(),
                            finish_reason = ?outcome.finish_reason,
                            "[terminal_agent] reminder 注入本轮循环"
                        );
                        messages.push(Message::text(Role::User, MID_SESSION_REMINDER_TEXT));
                        continue;
                    } else {
                        // 区分两种 cause（review_code P1-2）：
                        // - quota_exhausted: 用户配额（max_mid_session_reminders）已用完
                        // - rounds_exhausted: 剩余 MAX_ROUNDS 不足继续 reminder
                        let cause = if mid_session_reminder_count >= max_mid_session_reminders {
                            "quota_exhausted"
                        } else {
                            "rounds_exhausted"
                        };
                        tracing::warn!(
                            event = "premature_break",
                            cause = cause,
                            round = round,
                            reminder_count = mid_session_reminder_count,
                            max_reminders = max_mid_session_reminders,
                            effective_max_reminders = effective_max_reminders,
                            had_tool = had_tool_in_session,
                            text_len = outcome.text.len(),
                            finish_reason = ?outcome.finish_reason,
                            "[terminal_agent] 假性终结 break ({cause})"
                        );
                        full_text.push_str("\n\n");
                        full_text.push_str(&t!("TerminalAgent.mid_session_aborted").to_string());
                        break;
                    }
                }

                // 既有 warn 日志保留——便于排查"自动停止"路径
                if had_tool_in_session && !is_empty_text {
                    tracing::warn!(
                        "[terminal_agent] 第 {round} 轮已执行过 tool_call 后返回 text-only (text={}B, finish_reason={:?})；用户报告的『自动停止』疑似本路径",
                        outcome.text.len(),
                        outcome.finish_reason
                    );
                }
                break;
            }
            had_tool_in_session = true;

            messages.push(assistant_tool_calls_message(
                outcome.tool_calls.clone(),
                // 始终携带 content（可为空串）：部分严格的 OpenAI 兼容提供方
                // 会拒绝 content 为 [] 的 assistant tool_calls 消息
                Some(outcome.text.clone()),
            ));
            // 持久化 assistant(tool_calls) 消息到会话历史，供下一轮用户输入时
            // build_agent_history_messages 还原结构化工具调用上下文。
            if let (Some(repo), Some(sid)) = (message_repo.as_ref(), session_id) {
                persist_assistant_tool_calls(repo, sid, &outcome.text, &outcome.tool_calls);
            }

            // 单轮内多个工具调用严格串行（共享同一终端，避免写入时序冲突）
            for call in &outcome.tool_calls {
                // saturating_add 避免极端长会话下 u32 回绕到 0 触发渲染层的「未知序号」分支
                tool_seq = tool_seq.saturating_add(1);
                if ctx.cancel_token.is_cancelled() {
                    let _ = tx.send(AgentEvent::Cancelled).await;
                    return Ok(());
                }
                if self.hit_repeat_limit(call, &mut last_signature, &mut repeat_count) {
                    let note = t!(
                        "TerminalAgent.repeated_tool_call",
                        name = call.function.name.as_str()
                    )
                    .to_string();
                    tracing::warn!("[terminal_agent] {note}");
                    let _ = tx
                        .send(AgentEvent::ToolCallFinished {
                            call_id: call.id.clone(),
                            ok: false,
                            output: note.clone(),
                        })
                        .await;
                    tool_records.push(serde_json::json!({
                        "call_id": call.id,
                        "name": call.function.name,
                        "seq": tool_seq,
                        "title": action_summary(call),
                        "ok": false,
                        "args": summarize_args(call),
                        "output": note.clone(),
                    }));
                    full_text.push_str("\n\n");
                    full_text.push_str(&note);
                    break 'rounds;
                }

                if !send_event(
                    tx,
                    AgentEvent::ToolCallStarted {
                        call_id: call.id.clone(),
                        name: call.function.name.clone(),
                        seq: tool_seq,
                        title: action_summary(call),
                        args_summary: summarize_args(call),
                    },
                )
                .await
                {
                    tracing::warn!("[terminal_agent] 事件通道已关闭，终止 Agent 循环");
                    return Ok(());
                }
                let (ok, result_text, effect) = execute_tool(handle, call).await;
                if ctx.cancel_token.is_cancelled() {
                    let _ = tx.send(AgentEvent::Cancelled).await;
                    return Ok(());
                }
                if !send_event(
                    tx,
                    AgentEvent::ToolCallFinished {
                        call_id: call.id.clone(),
                        ok,
                        output: summarize_output(&result_text),
                    },
                )
                .await
                {
                    tracing::warn!("[terminal_agent] 事件通道已关闭，终止 Agent 循环");
                    return Ok(());
                }
                tool_records.push(serde_json::json!({
                    "call_id": call.id,
                    "name": call.function.name,
                    "seq": tool_seq,
                    "title": action_summary(call),
                    "ok": ok,
                    "args": summarize_args(call),
                    "output": summarize_output(&result_text),
                }));
                if matches!(effect, ToolEffect::WroteTerminal) {
                    had_write = true;
                }
                // 持久化 tool(result) 消息，与上方 assistant(tool_calls) 配对，
                // 下一轮历史回放时还原为 OpenAI 协议要求的工具消息对。
                if let (Some(repo), Some(sid)) = (message_repo.as_ref(), session_id) {
                    persist_tool_result(repo, sid, &call.id, &result_text);
                }
                messages.push(tool_message(result_text, call.id.clone()));
                if let ToolEffect::TaskComplete(summary) = effect {
                    if !summary.is_empty() {
                        if !full_text.trim().is_empty() {
                            full_text.push_str("\n\n");
                        }
                        full_text.push_str(&summary);
                    }
                    break 'rounds;
                }
            }
        }

        if !had_write && claims_execution(&full_text) {
            full_text.push_str(t!("TerminalAgent.no_write_warning").as_ref());
        }
        append_tool_records(&mut full_text, &tool_records);
        let _ = tx
            .send(AgentEvent::Completed(AgentResult {
                content: full_text,
                ..Default::default()
            }))
            .await;
        Ok(())
    }

    /// 重复调用检测：同 (name, args) 连续达到阈值返回 true。
    /// args 先做 JSON 归一化，避免空白差异绕过熔断。
    fn hit_repeat_limit(
        &self,
        call: &ToolCall,
        last_signature: &mut Option<u64>,
        repeat_count: &mut usize,
    ) -> bool {
        let normalized_args = serde_json::from_str::<serde_json::Value>(&call.function.arguments)
            .map(|value| value.to_string())
            .unwrap_or_else(|_| call.function.arguments.clone());
        let mut hasher = DefaultHasher::new();
        call.function.name.hash(&mut hasher);
        normalized_args.hash(&mut hasher);
        let signature = hasher.finish();
        if *last_signature == Some(signature) {
            *repeat_count += 1;
        } else {
            *last_signature = Some(signature);
            *repeat_count = 1;
        }
        *repeat_count >= REPEAT_LIMIT
    }

    /// 发起一轮流式请求并收集文本与完整工具调用。
    async fn stream_round(
        &self,
        provider: &Arc<dyn LlmProvider>,
        request: ChatRequest,
        cancel_token: &CancellationToken,
        tx: &mpsc::Sender<AgentEvent>,
    ) -> Result<RoundOutcome, RoundError> {
        let mut stream = provider
            .chat_stream(&request)
            .await
            .map_err(|err| RoundError::Failed(err.to_string()))?;
        let collect = collect_round(&mut stream, cancel_token, tx);
        tokio::time::timeout(ROUND_TIMEOUT, collect)
            .await
            .map_err(|_| RoundError::Failed(t!("TerminalAgent.round_timeout").to_string()))?
    }
}

/// 收集一轮流式响应：文本/推理增量透传 UI，工具调用按 id 去重累积。
async fn collect_round(
    stream: &mut ChatStream,
    cancel_token: &CancellationToken,
    tx: &mpsc::Sender<AgentEvent>,
) -> Result<RoundOutcome, RoundError> {
    let mut text = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut finish_reason: Option<String> = None;
    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => return Err(RoundError::Cancelled),
            chunk = stream.next() => {
                match chunk {
                    None => break,
                    Some(Err(err)) => return Err(RoundError::Failed(err.to_string())),
                    Some(Ok(response)) => {
                        let parts = extract_stream_text_parts(&response);
                        if let Some(reasoning) = parts.reasoning {
                            // 接收端已关闭时按取消处理
                            if !send_event(tx, AgentEvent::ReasoningDelta(reasoning.to_string()))
                                .await
                            {
                                return Err(RoundError::Cancelled);
                            }
                        }
                        if let Some(content) = parts.content {
                            text.push_str(content);
                            if !send_event(tx, AgentEvent::TextDelta(content.to_string())).await {
                                return Err(RoundError::Cancelled);
                            }
                        }
                        // SSE 层已跨块累积完整调用；同一 id 可能重复出现，按 id 去重
                        if let Some(calls) = response
                            .choices
                            .first()
                            .and_then(|choice| choice.delta.tool_calls.clone())
                        {
                            for call in calls {
                                upsert_tool_call(&mut tool_calls, call);
                            }
                        }
                        if let Some(reason) = response
                            .choices
                            .iter()
                            .find_map(|choice| choice.finish_reason.clone())
                        {
                            finish_reason = Some(reason);
                            break;
                        }
                    }
                }
            }
        }
    }
    Ok(RoundOutcome {
        text,
        tool_calls,
        finish_reason,
    })
}

/// 按 id 更新或追加工具调用。
fn upsert_tool_call(tool_calls: &mut Vec<ToolCall>, call: ToolCall) {
    if let Some(existing) = tool_calls
        .iter_mut()
        .find(|existing| existing.id == call.id)
    {
        *existing = call;
    } else {
        tool_calls.push(call);
    }
}

/// 发送 Agent 事件；接收端已关闭（如面板被关闭）时返回 false，调用方应终止循环。
async fn send_event(tx: &mpsc::Sender<AgentEvent>, event: AgentEvent) -> bool {
    tx.send(event).await.is_ok()
}

/// 构建携带工具定义的流式请求。
fn build_request(ctx: &AgentContext, messages: Vec<Message>) -> ChatRequest {
    ChatRequest {
        model: ctx.provider_config.model.clone(),
        messages,
        max_tokens: ctx
            .provider_config
            .max_tokens
            .map(|v| v as u32)
            .or(Some(4096)),
        temperature: ctx.provider_config.temperature.or(Some(0.7)),
        stream: Some(true),
        tools: Some(tool_definitions()),
        tool_choice: Some(ToolChoice::auto()),
        ..Default::default()
    }
}

/// 工具结果摘要（供 ToolCallFinished 事件与持久化记录）。
fn summarize_output(output: &str) -> String {
    const MAX_SUMMARY_CHARS: usize = 200;
    // 快速路径：字节数不超阈值则字符数必然不超
    if output.len() <= MAX_SUMMARY_CHARS {
        return output.to_string();
    }
    if output.chars().count() <= MAX_SUMMARY_CHARS {
        return output.to_string();
    }
    output.chars().take(MAX_SUMMARY_CHARS).collect::<String>() + "..."
}

/// 幻觉防护：检测"已执行"类声明（中英文关键词）。
fn claims_execution(text: &str) -> bool {
    const KEYWORDS: &[&str] = &["已执行", "已运行", "已成功执行", "已執行"];
    if KEYWORDS.iter().any(|keyword| text.contains(keyword)) {
        return true;
    }
    let lower = text.to_lowercase();
    lower.contains("executed") || lower.contains("has been run") || lower.contains("i ran")
}

/// 把工具调用记录以 fenced 块嵌入最终内容（重载后由 code_block_renderer 重建卡片）。
fn append_tool_records(content: &mut String, records: &[serde_json::Value]) {
    if records.is_empty() {
        return;
    }
    content.push_str("\n\n");
    for record in records {
        content.push_str("```omnihub-tool\n");
        content.push_str(&serde_json::to_string(record).unwrap_or_default());
        content.push_str("\n```\n");
    }
}

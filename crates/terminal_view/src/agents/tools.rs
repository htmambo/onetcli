//! 终端操作员工具集：JSON Schema 定义与工具执行。

use one_core::llm::{Tool, ToolCall};
use rust_i18n::t;

use crate::agent_bridge::{TerminalOperatorHandle, WriteOutcome};

/// 工具输出截断阈值（按字节计，约 2000 token）。
const MAX_TOOL_OUTPUT_BYTES: usize = 8000;

/// 兼容整数与 JSON 浮点数（如 `{"max_lines": 50.0}`）。LLM 经常以浮点形式下发数值，
/// 直接 `as_u64()` 会静默回退为默认值，导致语义被模型意外改写。
fn as_u64_lossy(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value.as_f64().and_then(|f| {
            if f.is_finite() && f >= 0.0 {
                Some(f as u64)
            } else {
                None
            }
        })
    })
}
/// read_terminal_output 默认行数（修改时同步 schema 描述）。
const DEFAULT_READ_LINES: u64 = 200;
/// read_terminal_output 行数上限。
const MAX_READ_LINES: u64 = 2000;
/// write_to_terminal 默认等待毫秒数（修改时同步 schema 描述）。
const DEFAULT_WAIT_MS: u64 = 1500;
/// write_to_terminal 等待毫秒数上限。
const MAX_WAIT_MS: u64 = 30000;

/// 工具执行的副作用标记（供 Agent 循环判定终止与幻觉防护）。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ToolEffect {
    None,
    /// 实际写入了终端（用于幻觉防护）。
    WroteTerminal,
    /// 模型宣告任务完成，携带汇报摘要。
    TaskComplete(String),
}

/// 全部工具定义（供 ChatRequest.tools）。
pub(crate) fn tool_definitions() -> Vec<Tool> {
    let terminal_id = |required: bool| {
        let desc = if required {
            "终端 id（取自 get_terminal_list）"
        } else {
            "终端 id（取自 get_terminal_list），缺省为终端列表第一个"
        };
        serde_json::json!({ "type": "integer", "minimum": 0, "description": desc })
    };
    vec![
        tool(
            "task_complete",
            "任务完成时调用，携带结果摘要",
            serde_json::json!({
                "type": "object",
                "properties": { "summary": { "type": "string", "maxLength": 2000, "description": "任务结果摘要" } },
                "required": ["summary"],
                "additionalProperties": false
            }),
        ),
        tool(
            "read_terminal_output",
            "读取终端输出，返回最近的尾部内容（从底部向上最多 max_lines 行）",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "terminal_id": terminal_id(false),
                    "max_lines": { "type": "integer", "minimum": 1, "maximum": 2000, "description": "最大行数，默认 200，上限 2000" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "write_to_terminal",
            "向终端写入命令并自动追加回车执行，等待完成后返回尾部输出",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "maxLength": 4096, "description": "要执行的命令（单行，不要包含换行符，执行时自动追加回车）" },
                    "terminal_id": terminal_id(false),
                    "wait_ms": { "type": "integer", "minimum": 0, "maximum": 30000, "description": "等待毫秒数，默认 1500，上限 30000；交互式命令应给较大值" }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        ),
        tool(
            "get_terminal_list",
            "枚举当前打开的终端，返回 JSON 数组（字段：id/title/kind/cwd）",
            serde_json::json!({
                "type": "object", "properties": {}, "additionalProperties": false
            }),
        ),
        tool(
            "get_terminal_cwd",
            "获取终端当前工作目录",
            serde_json::json!({
                "type": "object",
                "properties": { "terminal_id": terminal_id(false) },
                "additionalProperties": false
            }),
        ),
        tool(
            "get_terminal_selection",
            "获取终端当前选区文本",
            serde_json::json!({
                "type": "object",
                "properties": { "terminal_id": terminal_id(false) },
                "additionalProperties": false
            }),
        ),
        tool(
            "focus_terminal",
            "聚焦指定终端窗口",
            serde_json::json!({
                "type": "object",
                "properties": { "terminal_id": terminal_id(true) },
                "required": ["terminal_id"],
                "additionalProperties": false
            }),
        ),
    ]
}

fn tool(name: &str, description: &str, parameters: serde_json::Value) -> Tool {
    Tool::function(name, Some(description.to_string()), parameters)
}

/// 执行单个工具调用，返回 (是否成功, 回灌模型的结果文本, 副作用)。
pub(crate) async fn execute_tool(
    handle: &TerminalOperatorHandle,
    call: &ToolCall,
) -> (bool, String, ToolEffect) {
    let args: serde_json::Value = match serde_json::from_str(&call.function.arguments) {
        Ok(args) => args,
        Err(err) => {
            return (
                false,
                format!("参数 JSON 解析失败，请修正格式后重试: {err}"),
                ToolEffect::None,
            );
        }
    };
    match call.function.name.as_str() {
        "task_complete" => {
            let summary = args["summary"].as_str().unwrap_or("任务已完成").to_string();
            (true, "ok".to_string(), ToolEffect::TaskComplete(summary))
        }
        "read_terminal_output" => match resolve_terminal(handle, &args).await {
            Ok(id) => {
                let max_lines = as_u64_lossy(&args["max_lines"])
                    .unwrap_or(DEFAULT_READ_LINES)
                    .clamp(1, MAX_READ_LINES);
                match handle.read_output(id, max_lines as usize).await {
                    Ok(output) => (true, truncate_output(output), ToolEffect::None),
                    Err(err) => (false, format!("读取终端输出失败: {err}"), ToolEffect::None),
                }
            }
            Err(err) => (false, err, ToolEffect::None),
        },
        "write_to_terminal" => {
            let Some(command) = args["command"].as_str().map(str::to_string) else {
                return (false, "缺少 command 参数".to_string(), ToolEffect::None);
            };
            match resolve_terminal(handle, &args).await {
                Ok(id) => {
                    let wait_ms = as_u64_lossy(&args["wait_ms"])
                        .unwrap_or(DEFAULT_WAIT_MS)
                        .min(MAX_WAIT_MS);
                    match handle.write_command(id, command, wait_ms).await {
                        Ok(outcome) => (
                            true,
                            format_write_outcome(outcome),
                            ToolEffect::WroteTerminal,
                        ),
                        Err(err) => (false, format!("命令写入失败: {err}"), ToolEffect::None),
                    }
                }
                Err(err) => (false, err, ToolEffect::None),
            }
        }
        "get_terminal_list" => match handle.list_terminals().await {
            Ok(terminals) => {
                let items: Vec<serde_json::Value> = terminals
                    .iter()
                    .map(|info| {
                        serde_json::json!({
                            "id": info.id,
                            "title": info.title,
                            "kind": format!("{:?}", info.connection_kind),
                            "cwd": info.cwd,
                        })
                    })
                    .collect();
                (
                    true,
                    serde_json::to_string_pretty(&items).unwrap_or_default(),
                    ToolEffect::None,
                )
            }
            Err(err) => (false, format!("枚举终端失败: {err}"), ToolEffect::None),
        },
        "get_terminal_cwd" => match resolve_terminal(handle, &args).await {
            Ok(id) => match handle.get_cwd(id).await {
                Ok(cwd) => (
                    true,
                    cwd.unwrap_or_else(|| "(未知)".to_string()),
                    ToolEffect::None,
                ),
                Err(err) => (false, format!("获取工作目录失败: {err}"), ToolEffect::None),
            },
            Err(err) => (false, err, ToolEffect::None),
        },
        "get_terminal_selection" => match resolve_terminal(handle, &args).await {
            Ok(id) => match handle.get_selection(id).await {
                Ok(text) => (
                    true,
                    truncate_output(text.unwrap_or_else(|| "(无选区)".to_string())),
                    ToolEffect::None,
                ),
                Err(err) => (false, format!("获取选区失败: {err}"), ToolEffect::None),
            },
            Err(err) => (false, err, ToolEffect::None),
        },
        "focus_terminal" => match resolve_terminal(handle, &args).await {
            Ok(id) => match handle.focus(id).await {
                Ok(()) => (true, "ok".to_string(), ToolEffect::None),
                Err(err) => (false, format!("聚焦终端失败: {err}"), ToolEffect::None),
            },
            Err(err) => (false, err, ToolEffect::None),
        },
        other => (false, format!("未知工具: {other}"), ToolEffect::None),
    }
}

/// 解析 terminal_id 参数；缺省（未传或 null）时取终端列表第一个，非法值报错。
async fn resolve_terminal(
    handle: &TerminalOperatorHandle,
    args: &serde_json::Value,
) -> Result<u64, String> {
    match args.get("terminal_id") {
        Some(value) if !value.is_null() => value
            .as_u64()
            .ok_or_else(|| format!("terminal_id 必须是非负整数，收到: {value}")),
        _ => {
            let terminals = handle
                .list_terminals()
                .await
                .map_err(|err| format!("枚举终端失败: {err}"))?;
            terminals
                .first()
                .map(|info| info.id)
                .ok_or_else(|| "当前没有可用的终端".to_string())
        }
    }
}

/// 截断超长工具输出并附加标记（防终端日志撑爆上下文）。
fn truncate_output(mut output: String) -> String {
    if output.len() <= MAX_TOOL_OUTPUT_BYTES {
        return output;
    }
    let mut end = MAX_TOOL_OUTPUT_BYTES;
    while !output.is_char_boundary(end) {
        end -= 1;
    }
    output.truncate(end);
    output.push_str("...\n[输出过长已截断]");
    output
}

/// 组装 write_to_terminal 的结果文本。
fn format_write_outcome(outcome: WriteOutcome) -> String {
    let output = truncate_output(outcome.output);
    if outcome.timed_out {
        format!(
            "{output}\n[等待超时：命令可能仍在运行或已进入交互模式（top/vim/sudo 密码等），请先 read_terminal_output 观察再决定下一步]"
        )
    } else {
        output
    }
}

/// 工具调用参数的短摘要（供 UI 卡片展示）。
pub(crate) fn summarize_args(call: &ToolCall) -> String {
    const MAX_SUMMARY_LEN: usize = 200;
    let args: serde_json::Value = match serde_json::from_str(&call.function.arguments) {
        Ok(args) => args,
        Err(err) => {
            tracing::warn!(
                tool = %call.function.name,
                call_id = %call.id,
                error = %err,
                "工具参数 JSON 解析失败，摘要回退为空对象"
            );
            serde_json::json!({})
        }
    };
    let summary = match call.function.name.as_str() {
        "write_to_terminal" => args["command"].as_str().unwrap_or("").to_string(),
        "task_complete" => args["summary"].as_str().unwrap_or("").to_string(),
        "read_terminal_output" => {
            let target = terminal_target_label(&args);
            let lines = as_u64_lossy(&args["max_lines"])
                .unwrap_or(DEFAULT_READ_LINES)
                .clamp(1, MAX_READ_LINES);
            t!(
                "TerminalAgent.args_read_output",
                id = target.as_str(),
                lines = lines
            )
            .to_string()
        }
        "get_terminal_cwd" | "get_terminal_selection" | "focus_terminal" => {
            let target = terminal_target_label(&args);
            t!("TerminalAgent.args_terminal_with_id", id = target.as_str()).to_string()
        }
        "get_terminal_list" => t!("TerminalAgent.args_terminal_list").to_string(),
        _ => call.function.arguments.clone(),
    };
    // 单次遍历：char_indices().nth(MAX_SUMMARY_LEN) 在字符数 > 限制时返回 Some，(idx, _) 的 idx 必为 UTF-8 字符边界
    match summary.char_indices().nth(MAX_SUMMARY_LEN) {
        Some((end, _)) => {
            let mut truncated = String::with_capacity(end + 3);
            truncated.push_str(&summary[..end]);
            truncated.push_str("...");
            truncated
        }
        None => summary,
    }
}

/// 终端目标的可读标签（#id 或"默认终端"）。
fn terminal_target_label(args: &serde_json::Value) -> String {
    match args.get("terminal_id").and_then(|value| value.as_u64()) {
        Some(id) => format!("#{id}"),
        None => t!("TerminalAgent.args_default_terminal").to_string(),
    }
}

/// 用作折叠头部的插值字符串净化：去除控制字符与换行，将连续空白折叠为单空格，防止含 \n/\r 的命令或终端 id 破坏单行布局。
fn sanitize_for_header(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut prev_space = false;
    for ch in value.chars() {
        if ch.is_control() {
            continue;
        }
        let is_space = ch.is_whitespace();
        if is_space {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    // 原地 trim，避免 out.trim().to_string() 产生的额外堆分配
    let trimmed_start = out.len() - out.trim_start().len();
    if trimmed_start > 0 {
        out.drain(..trimmed_start);
    }
    let new_end = out.trim_end().len();
    out.truncate(new_end);
    out
}

/// 折叠态头部的动作摘要（如「执行命令，ls -la」），与 summarize_args 的详细摘要是互补关系。
pub(crate) fn action_summary(call: &ToolCall) -> String {
    let args: serde_json::Value = match serde_json::from_str(&call.function.arguments) {
        Ok(args) => args,
        Err(err) => {
            tracing::warn!(
                tool = %call.function.name,
                call_id = %call.id,
                error = %err,
                "工具参数 JSON 解析失败，动作摘要回退到工具名"
            );
            return call.function.name.clone();
        }
    };
    let summary = match call.function.name.as_str() {
        "write_to_terminal" => t!(
            "TerminalAgent.action_write_command",
            command = sanitize_for_header(args["command"].as_str().unwrap_or("")).as_str()
        )
        .to_string(),
        "task_complete" => t!("TerminalAgent.action_task_complete").to_string(),
        "read_terminal_output" => {
            let target = terminal_target_label(&args);
            // 摘要也走 clamp，防止 LLM 下发 1e18 行之类极端值撑爆 UI
            let lines = as_u64_lossy(&args["max_lines"])
                .unwrap_or(DEFAULT_READ_LINES)
                .clamp(1, MAX_READ_LINES);
            t!(
                "TerminalAgent.action_read_output",
                id = target.as_str(),
                lines = lines
            )
            .to_string()
        }
        "get_terminal_cwd" => {
            let target = terminal_target_label(&args);
            t!("TerminalAgent.action_get_cwd", id = target.as_str()).to_string()
        }
        "get_terminal_selection" => {
            let target = terminal_target_label(&args);
            t!("TerminalAgent.action_get_selection", id = target.as_str()).to_string()
        }
        "focus_terminal" => {
            let target = terminal_target_label(&args);
            t!("TerminalAgent.action_focus_terminal", id = target.as_str()).to_string()
        }
        "get_terminal_list" => t!("TerminalAgent.action_terminal_list").to_string(),
        _ => call.function.name.clone(),
    };
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definitions_cover_operator_surface() {
        let tools = tool_definitions();
        let names: Vec<&str> = tools.iter().map(|t| t.function.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "task_complete",
                "read_terminal_output",
                "write_to_terminal",
                "get_terminal_list",
                "get_terminal_cwd",
                "get_terminal_selection",
                "focus_terminal",
            ]
        );
        // 每个工具必须能通过 JSON 序列化往返，且 schema 收敛（禁止额外属性）
        for tool in &tools {
            let payload = serde_json::to_value(tool).expect("工具定义序列化失败");
            assert_eq!(payload["type"], "function");
            assert_eq!(
                payload["function"]["parameters"]["additionalProperties"],
                serde_json::json!(false),
                "工具 {} 缺少 additionalProperties: false",
                tool.function.name
            );
        }
    }

    #[test]
    fn truncate_output_appends_marker() {
        let short = "abc".to_string();
        assert_eq!(truncate_output(short.clone()), short);
        let long = "x".repeat(MAX_TOOL_OUTPUT_BYTES + 100);
        let result = truncate_output(long);
        // 截断后长度不超过阈值 + 标记文本（标记约 27 字节）
        assert!(result.len() <= MAX_TOOL_OUTPUT_BYTES + 30);
        assert!(result.ends_with("[输出过长已截断]"));
    }

    #[test]
    fn truncate_output_respects_char_boundary() {
        // 截断点落在 CJK 字符中间时不得 panic，且不得切碎字符
        let prefix = "a".repeat(MAX_TOOL_OUTPUT_BYTES - 1);
        let long = format!("{prefix}中yyy");
        let result = truncate_output(long);
        assert!(result.ends_with("[输出过长已截断]"));
        assert!(result.starts_with(&prefix));
        assert!(!result[..MAX_TOOL_OUTPUT_BYTES].contains('中'));
    }

    #[test]
    fn summarize_args_extracts_command() {
        let call = ToolCall {
            id: "c1".to_string(),
            call_type: "function".to_string(),
            function: one_core::llm::FunctionCall {
                name: "write_to_terminal".to_string(),
                arguments: "{\"command\":\"ls -la\"}".to_string(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(summarize_args(&call), "ls -la");
    }

    fn make_call(name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: "c1".to_string(),
            call_type: "function".to_string(),
            function: one_core::llm::FunctionCall {
                name: name.to_string(),
                arguments: arguments.to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn summarize_args_read_output_is_human_readable() {
        let summary = summarize_args(&make_call(
            "read_terminal_output",
            "{\"terminal_id\":3,\"max_lines\":50}",
        ));
        assert!(summary.contains("#3"));
        assert!(summary.contains("50"));
        assert!(!summary.contains('{'));
    }

    #[test]
    fn summarize_args_terminal_tools_have_no_raw_json() {
        for (name, arguments) in [
            ("get_terminal_list", "{}"),
            ("focus_terminal", "{\"terminal_id\":2}"),
            ("get_terminal_cwd", "{}"),
            ("get_terminal_selection", "{}"),
        ] {
            let summary = summarize_args(&make_call(name, arguments));
            assert!(
                !summary.contains('{'),
                "{name} 的摘要不应包含原始 JSON: {summary}"
            );
        }
    }

    #[test]
    fn action_summary_is_localized_action_label() {
        // 动作摘要为「动词短语 + 参数」，折叠态头部可直接展示
        let write = action_summary(&make_call("write_to_terminal", "{\"command\":\"ls -la\"}"));
        assert!(write.contains("ls -la"), "write 摘要应包含命令: {write}");
        assert!(!write.contains('{'), "write 摘要不应包含原始 JSON: {write}");

        let list = action_summary(&make_call("get_terminal_list", "{}"));
        assert!(!list.contains('{') && !list.contains("get_terminal_list"));

        let task = action_summary(&make_call("task_complete", "{\"summary\":\"done\"}"));
        assert!(!task.contains('{') && !task.contains("done"));

        let read = action_summary(&make_call(
            "read_terminal_output",
            "{\"terminal_id\":3,\"max_lines\":50}",
        ));
        assert!(read.contains("#3") && read.contains("50") && !read.contains('{'));
    }

    #[test]
    fn action_summary_falls_back_to_tool_name() {
        let unknown = action_summary(&make_call("unknown_tool", "{}"));
        assert_eq!(unknown, "unknown_tool");
    }

    #[test]
    fn action_summary_sanitizes_control_chars_and_newlines() {
        // 命令含换行 / 回车 / 制表符时，折叠头必须塌缩为单行单空格，不破坏布局
        let s = action_summary(&make_call(
            "write_to_terminal",
            "{\"command\":\"echo \\nfoo\\t\\rbar\\nbaz\"}",
        ));
        assert!(!s.contains('\n'), "不应留下换行: {s:?}");
        assert!(!s.contains('\r'), "不应留下回车: {s:?}");
        assert!(!s.contains('\t'), "不应留下制表符: {s:?}");
        assert!(s.contains("foo"));
        assert!(s.contains("bar"));
        assert!(s.contains("baz"));
    }

    #[test]
    fn sanitize_for_header_collapses_whitespace_runs() {
        assert_eq!(sanitize_for_header("hello  world"), "hello world");
        assert_eq!(sanitize_for_header("  a\t b  "), "a b");
        assert_eq!(sanitize_for_header(""), "");
        assert!(!sanitize_for_header("\x00str\x07").contains('\x00'));
    }

    #[test]
    fn action_summary_accepts_float_json_numbers() {
        // LLM 经常把 max_lines 输出为 `50.0`（JSON 浮点），应被正确解析而非回退到默认值
        let s = action_summary(&make_call(
            "read_terminal_output",
            "{\"terminal_id\":3,\"max_lines\":50.0}",
        ));
        assert!(s.contains("50"), "浮点 50.0 应被识别为 50 行: {s:?}");
    }

    #[test]
    fn sanitize_for_header_trims_whitespace_correctly() {
        // 验证原地 trim（避免 out.trim().to_string() 的额外分配）
        let s = sanitize_for_header("  hello  ");
        assert_eq!(s, "hello");
        let s = sanitize_for_header("\n\nfoo\n\n");
        assert_eq!(s, "foo");
    }
}

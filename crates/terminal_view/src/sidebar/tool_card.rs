//! omnihub-tool 代码块渲染器。
//!
//! Agent 模式的工具调用记录以 fenced `omnihub-tool` JSON 块持久化在助手消息中；
//! 会话重载后由本渲染器把代码块重建为只读工具卡片（与 ChatDB 图表同一往返模式）。
//! 头部与实时卡片同格式：`[序号]:动作摘要`；展开后显示工具名/参数/输出详情。

use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Styled, Window, div};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::text::{CodeBlock, CodeBlockRenderOptions};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, Size, h_flex, v_flex};
use rust_i18n::t;
use serde::Deserialize;

/// 持久化的工具调用记录（与 agents::terminal_operator 的写入格式对应）。
#[derive(Debug, Deserialize)]
struct ToolRecord {
    call_id: String,
    name: String,
    /// 序号（同一次 Agent 运行内从 1 递增；0 表示旧记录）
    #[serde(default)]
    seq: u32,
    /// 折叠态头部展示的动作摘要（旧记录为空，退化为工具名）
    #[serde(default)]
    title: String,
    #[serde(default)]
    ok: bool,
    /// 请求信息摘要（如命令），供展开详情展示
    #[serde(default)]
    args: String,
    #[serde(default)]
    output: String,
}

/// `TextView::markdown` 的 code_block_renderer：
/// `omnihub-tool` 块渲染为只读卡片，其他块回落到默认渲染。
/// 头部截断上限（与 ai_chat::rendering 同值，保持 live/reload 卡片头部宽度一致）。
const TOOL_CARD_HEADER_MAX_CHARS: usize = 80;

/// 按字符数截断，保留字符完整性（适合 CJK 等多字节文本）。
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let end = text
        .char_indices()
        .nth(max_chars)
        .map_or(text.len(), |(idx, _)| idx);
    let mut end = end;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

pub fn render_omnihub_tool_block(
    code_block: &CodeBlock,
    _options: CodeBlockRenderOptions,
    default_element: AnyElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let is_tool_block = code_block
        .lang()
        .as_ref()
        .is_some_and(|lang| lang.as_ref().eq_ignore_ascii_case("omnihub-tool"));
    if !is_tool_block {
        return default_element;
    }

    match serde_json::from_str::<ToolRecord>(code_block.code().as_ref()) {
        Ok(record) => render_record_card(&record, window, cx),
        Err(_) => default_element,
    }
}

/// 渲染单条工具记录的只读卡片（可折叠：状态图标 + 序号/动作摘要；展开显示详情）。
fn render_record_card(record: &ToolRecord, window: &mut Window, cx: &mut App) -> AnyElement {
    let (status_icon, status_color) = if record.ok {
        (IconName::Check, cx.theme().success)
    } else {
        (IconName::TriangleAlert, cx.theme().danger)
    };
    // 头部与实时卡片同格式：[序号]:动作摘要；旧记录（无 title/seq）退化为工具名
    let header_text = if record.title.is_empty() {
        record.name.clone()
    } else if record.seq > 0 {
        format!("[{}]:{}", record.seq, record.title)
    } else {
        record.title.clone()
    };
    // 与实时卡片统一截断（TOOL_CARD_HEADER_MAX_CHARS=80，字符级安全）
    let header_summary = truncate_chars(&header_text, TOOL_CARD_HEADER_MAX_CHARS);
    // 展开详情：工具名 + 参数摘要 + 输出（失败时输出即错误信息），标签与实时卡片同源
    let mut detail = format!("**{}** {}", t!("AiChat.tool_call_detail_tool"), record.name);
    if !record.args.is_empty() {
        detail.push_str(&format!(
            "\n**{}** {}",
            t!("AiChat.tool_call_detail_args"),
            record.args
        ));
    }
    if !record.output.is_empty() {
        detail.push_str(&format!(
            "\n**{}** {}",
            t!("AiChat.tool_call_detail_output"),
            record.output
        ));
    }

    let state_id = SharedString::from(format!("tool-record-expanded-{}", record.call_id));
    let expanded_state = window.use_keyed_state(state_id, cx, |_, _| false);
    let is_expanded = *expanded_state.read(cx);
    let record_call_id = record.call_id.clone();

    v_flex()
        .w_full()
        .gap_1()
        .pl_2()
        .border_l_2()
        .border_color(status_color.opacity(0.7))
        .child(
            h_flex()
                .w_full()
                .items_center()
                .gap_1()
                .child(
                    Button::new(SharedString::from(format!(
                        "tool-record-toggle-{}",
                        record_call_id
                    )))
                    .ghost()
                    .xsmall()
                    .icon(if is_expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .tooltip(if is_expanded {
                        t!("AiChat.tool_call_collapse").to_string()
                    } else {
                        t!("AiChat.tool_call_expand").to_string()
                    })
                    .on_click(move |_, _, cx| {
                        expanded_state.update(cx, |expanded, cx| {
                            *expanded = !*expanded;
                            cx.notify();
                        });
                    }),
                )
                .child(
                    Icon::new(status_icon)
                        .with_size(Size::Small)
                        .text_color(status_color),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(header_summary),
                ),
        )
        .when(is_expanded && !detail.is_empty(), |this| {
            this.child(
                div()
                    .pl_6()
                    .pr_2()
                    .pb_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(detail),
            )
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_record_with_all_fields() {
        let record: ToolRecord = serde_json::from_str(
            r#"{"call_id":"c1","name":"write_terminal_input","seq":2,"title":"执行命令，ls -la","ok":true,"args":"ls -la","output":"done"}"#,
        )
        .expect("合法 JSON 应解析成功");
        assert_eq!(record.call_id, "c1");
        assert_eq!(record.name, "write_terminal_input");
        assert_eq!(record.seq, 2);
        assert_eq!(record.title, "执行命令，ls -la");
        assert!(record.ok);
        assert_eq!(record.args, "ls -la");
        assert_eq!(record.output, "done");
    }

    #[test]
    fn parse_record_defaults_seq_title_and_output() {
        let record: ToolRecord =
            serde_json::from_str(r#"{"call_id":"c2","name":"read_terminal_output"}"#)
                .expect("缺省字段应使用默认值");
        assert_eq!(record.seq, 0);
        assert!(record.title.is_empty());
        assert!(!record.ok);
        assert!(record.args.is_empty());
        assert!(record.output.is_empty());
    }
}

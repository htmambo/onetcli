//! Thinking 块解析与展示。
//!
//! **背景**: 部分 LLM provider 在 streaming 响应中把 `... ` 字面量写入 `content` 字段
//! （而非 `reasoning` 字段），导致 markdown 渲染时遇到"unsupported inline html tag"警告。
//! 此模块在 UI 渲染前**前置解析**：从 content 文本中提取 `` 块渲染为可折叠
//! ThinkingPanel，剩余部分按 markdown 正常渲染。
//!
//! **不持久化**: reasoning 仅在当前对话 (in-memory) 展示，不写入 chat_messages 表。

use gpui::prelude::FluentBuilder;
use gpui::{App, Component, IntoElement, ParentElement, SharedString, Styled, Window};
use gpui_component::{
    ActiveTheme, IconName, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    text::TextView,
    v_flex,
};
use rust_i18n::t;

/// 解析后的消息内容。
#[derive(Debug, Default, Clone)]
pub struct ParsedMessageContent {
    /// 合并后的所有 `` 块内容（trim 后非空）；None = 无 `` 块
    pub thinking: Option<String>,
    /// 剥离 `` 块后的剩余 markdown 内容
    pub body: String,
}

const THINKING_OPEN: &str = "<think>";
const THINKING_CLOSE: &str = "</think>";
const THINKING_OPEN_LEN: usize = THINKING_OPEN.len();
const THINKING_CLOSE_LEN: usize = THINKING_CLOSE.len();

/// 从 content 文本中提取 `` 块（DOTALL 模式，跨行）。
///
/// **行为**:
/// - 多块合并（按出现顺序用 `\n\n` join）
/// - 块前后空白修剪
/// - 块 trim 后为空 → 视为无 thinking
/// - 块外内容（剩余 body）保留原顺序
/// - 未配对的 `` 视为普通文本
pub fn split_thinking_blocks(content: &str) -> ParsedMessageContent {
    let mut thinking_parts: Vec<String> = Vec::new();
    let mut body = String::new();
    let mut remaining = content;

    while let Some(open_idx) = remaining.find(THINKING_OPEN) {
        // 保留 `` 之前的内容到 body
        body.push_str(&remaining[..open_idx]);

        let after_open = &remaining[open_idx + THINKING_OPEN_LEN..];
        if let Some(close_idx) = after_open.find(THINKING_CLOSE) {
            let inner = &after_open[..close_idx];
            let trimmed = inner.trim();
            if !trimmed.is_empty() {
                thinking_parts.push(trimmed.to_string());
            }
            remaining = &after_open[close_idx + THINKING_CLOSE_LEN..];
        } else {
            // 没有匹配的 ``：把 `` 视为普通文本追加到 body
            body.push_str(THINKING_OPEN);
            remaining = after_open;
        }
    }
    body.push_str(remaining);

    ParsedMessageContent {
        thinking: if thinking_parts.is_empty() {
            None
        } else {
            Some(thinking_parts.join("\n\n"))
        },
        body,
    }
}

/// ThinkingPanel：可折叠的思考过程展示组件。
///
/// **折叠态**:
/// - 高度限制 60px (≈ 2 行)
/// - 内部用 flex column + justify-end 让最新内容贴底
/// - 用户无需展开也能看到最新思考
///
/// **展开态**:
/// - 自适应高度
///
/// **按钮**: 折叠态显示 `▶`，展开态显示 `▼`。
///
/// **折叠态**:
/// - 高度限制 60px (≈ 2 行)
/// - 内部用 flex column + justify-end 让最新内容贴底
/// - 用户无需展开也能看到最新思考
///
/// **展开态**:
/// - 自适应高度
///
/// **按钮**: 折叠态显示 `▶`，展开态显示 `▼`。
pub struct ThinkingPanel {
    content: SharedString,
    /// 渲染时由 keyed_state 注入的状态键，避免多实例冲突
    state_key: SharedString,
}

impl ThinkingPanel {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: SharedString::from(content.into()),
            state_key: SharedString::from(format!("thinking-{}", uuid_like())),
        }
    }

    /// 自定义 state_key（同一消息多次渲染时复用同一个状态）
    pub fn with_state_key(mut self, key: impl Into<SharedString>) -> Self {
        self.state_key = key.into();
        self
    }
}

/// 简单唯一 ID（避免引入 uuid crate）
fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", nanos)
}

impl gpui::RenderOnce for ThinkingPanel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let border_color = theme.border.opacity(0.7);
        let muted_fg = theme.muted_foreground;
        let bg = theme.background;

        let expanded_state = window.use_keyed_state(self.state_key.clone(), cx, |_, _| false);
        let is_expanded = *expanded_state.read(cx);

        let header = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_1()
            .px_2()
            .py_1()
            .child(
                gpui::div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(muted_fg)
                    .child(t!("AiChat.thinking_title").to_string()),
            )
            .child({
                let state = expanded_state.clone();
                Button::new(self.state_key.clone())
                    .ghost()
                    .xsmall()
                    .icon(if is_expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .tooltip(if is_expanded {
                        t!("AiChat.thinking_collapse").to_string()
                    } else {
                        t!("AiChat.thinking_expand").to_string()
                    })
                    .on_click(move |_, _, cx| {
                        state.update(cx, |v, cx| {
                            *v = !*v;
                            cx.notify();
                        });
                    })
            });

        // 折叠态：固定 60px + flex column + justify_end（最新贴底）
        // 展开态：自适应高度
        let body = if is_expanded {
            gpui::div()
                .w_full()
                .px_2()
                .pb_2()
                .text_xs()
                .text_color(muted_fg)
                .child(
                    TextView::markdown("thinking-md-expanded", self.content.clone())
                        .text_xs()
                        .selectable(true),
                )
        } else {
            gpui::div()
                .w_full()
                .max_h(gpui::px(60.0))
                .overflow_hidden()
                .px_2()
                .pb_2()
                .flex()
                .flex_col()
                .justify_end()
                .text_xs()
                .text_color(muted_fg)
                .child(
                    TextView::markdown("thinking-md-collapsed", self.content.clone())
                        .text_xs()
                        .selectable(true),
                )
        };

        v_flex()
            .w_full()
            .gap_1()
            .pl_2()
            .border_l_2()
            .border_color(border_color)
            .bg(bg)
            .rounded_md()
            .child(header)
            .child(body)
    }
}

impl IntoElement for ThinkingPanel {
    type Element = Component<Self>;

    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_single_block() {
        let parsed = split_thinking_blocks("<think>reasoning</think>body");
        assert_eq!(parsed.thinking.as_deref(), Some("reasoning"));
        assert_eq!(parsed.body, "body");
    }

    #[test]
    fn split_multiple_blocks_merged() {
        let parsed = split_thinking_blocks("<think>first</think>between<think>second</think>after");
        assert_eq!(parsed.thinking.as_deref(), Some("first\n\nsecond"));
        assert_eq!(parsed.body, "betweenafter");
    }

    #[test]
    fn split_multiline_thinking() {
        let parsed = split_thinking_blocks("<think>line1\nline2\nline3</think>body");
        assert_eq!(parsed.thinking.as_deref(), Some("line1\nline2\nline3"));
        assert_eq!(parsed.body, "body");
    }

    #[test]
    fn split_no_block_returns_body_only() {
        let parsed = split_thinking_blocks("just plain markdown");
        assert!(parsed.thinking.is_none());
        assert_eq!(parsed.body, "just plain markdown");
    }

    #[test]
    fn split_empty_block_returns_none() {
        let parsed = split_thinking_blocks("<think></think>body");
        assert!(parsed.thinking.is_none());
        assert_eq!(parsed.body, "body");
    }

    #[test]
    fn split_whitespace_only_block_returns_none() {
        let parsed = split_thinking_blocks("<think>   \n </think>body");
        assert!(parsed.thinking.is_none());
        assert_eq!(parsed.body, "body");
    }

    #[test]
    fn split_unclosed_block_kept_as_text() {
        let parsed = split_thinking_blocks("<think>unclosed body");
        assert!(parsed.thinking.is_none());
        assert_eq!(parsed.body, "<think>unclosed body");
    }

    #[test]
    fn split_trims_whitespace() {
        let parsed = split_thinking_blocks("<think>  \n  reasoning  \n </think>body");
        assert_eq!(parsed.thinking.as_deref(), Some("reasoning"));
        assert_eq!(parsed.body, "body");
    }

    #[test]
    fn split_block_before_and_after() {
        let parsed = split_thinking_blocks("pre<think>t</think>post");
        assert_eq!(parsed.thinking.as_deref(), Some("t"));
        assert_eq!(parsed.body, "prepost");
    }

    #[test]
    fn split_realistic_ai_response() {
        let content = "<think>The user wants to check the schema.\nLet me query.\n\nSELECT * FROM users</think>I've queried the users table.";
        let parsed = split_thinking_blocks(content);
        assert!(parsed.thinking.is_some());
        assert!(
            parsed
                .thinking
                .as_ref()
                .unwrap()
                .contains("SELECT * FROM users")
        );
        assert_eq!(parsed.body, "I've queried the users table.");
    }
}

//! ChatMessageRenderer - 共享消息渲染工具
//!
//! 提供通用的消息渲染函数，可被不同的面板复用。
//! SQL 面板可以在此基础上覆盖特定渲染（如 SQL 代码块）。

use crate::ai_chat::panel::CodeBlockActionRegistry;
use crate::ai_chat::reasoning;
use crate::ai_chat::thinking::{split_thinking_blocks, ThinkingPanel};
use crate::ai_chat::types::{
    ChatMessageUIGeneric, ChatRole, MessageExtension, MessageVariant, ToolCallStatus,
};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, Window,
    div,
};
use gpui_component::button::Button;
use gpui_component::clipboard::Clipboard;
use gpui_component::text::CodeBlockRenderer;
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, Size, button::ButtonVariants, h_flex, text::TextView,
    v_flex,
};
use rust_i18n::t;
use std::sync::Arc;

/// 共享消息渲染器
pub struct ChatMessageRenderer;

impl ChatMessageRenderer {
    /// 渲染用户消息
    pub fn render_user_message<E: MessageExtension>(
        msg: &ChatMessageUIGeneric<E>,
        cx: &App,
    ) -> AnyElement {
        div()
            .w_full()
            .px_3()
            .py_2()
            .bg(cx.theme().accent)
            .text_color(cx.theme().accent_foreground)
            .rounded_lg()
            .child(
                TextView::markdown(
                    SharedString::from(format!("user-msg-{}", msg.id)),
                    msg.content.clone(),
                )
                .selectable(true),
            )
            .into_any_element()
    }

    /// 渲染系统消息
    pub fn render_system_message<E: MessageExtension>(
        msg: &ChatMessageUIGeneric<E>,
        cx: &App,
    ) -> AnyElement {
        h_flex()
            .w_full()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(msg.content.clone()),
            )
            .into_any_element()
    }

    /// 渲染状态消息
    pub fn render_status_message(id: &str, title: &str, is_done: bool, cx: &App) -> AnyElement {
        let icon = if is_done {
            IconName::Check
        } else {
            IconName::Loader
        };

        h_flex()
            .id(SharedString::from(id.to_string()))
            .w_full()
            .items_center()
            .gap_2()
            .py_1()
            .child(
                Icon::new(icon)
                    .with_size(Size::Small)
                    .text_color(if is_done {
                        cx.theme().success
                    } else {
                        cx.theme().muted_foreground
                    }),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(title.to_string()),
            )
            .into_any_element()
    }

    /// 渲染 "思考中..." 占位符
    pub fn render_thinking(cx: &App) -> AnyElement {
        div()
            .w_full()
            .py_2()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("AiChat.thinking").to_string()),
            )
            .into_any_element()
    }

    /// 渲染可折叠的思考内容
    pub fn render_reasoning_block<E: MessageExtension>(
        msg: &ChatMessageUIGeneric<E>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        reasoning::render_reasoning_block(msg, window, cx)
    }

    /// 渲染助手文本消息（带代码块操作按钮）
    pub fn render_assistant_text<E: MessageExtension>(
        msg: &ChatMessageUIGeneric<E>,
        code_block_actions: &CodeBlockActionRegistry,
        code_block_renderer: Option<Arc<CodeBlockRenderer>>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        if msg.is_streaming && msg.content.is_empty() && msg.reasoning_content.is_empty() {
            return Self::render_thinking(cx);
        }

        v_flex()
            .w_full()
            .gap_2()
            .when(!msg.reasoning_content.is_empty(), |this| {
                this.child(Self::render_reasoning_block(msg, window, cx))
            })
            .when(!msg.content.is_empty(), |this| {
                this.child(Self::render_assistant_content(
                    msg,
                    code_block_actions,
                    code_block_renderer,
                ))
            })
            .into_any_element()
    }

    /// 渲染工具调用卡片（可折叠：图标 + 工具名 + 状态 + 输出）
    pub fn render_tool_call_message<E: MessageExtension>(
        msg: &ChatMessageUIGeneric<E>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let MessageVariant::ToolCall {
            call_id,
            name,
            seq,
            title,
            status,
            args_summary,
            output,
        } = &msg.variant
        else {
            return div().into_any_element();
        };

        let state_id = SharedString::from(format!("tool-call-expanded-{}", msg.id));
        let expanded_state = window.use_keyed_state(state_id, cx, |_, _| msg.is_expanded);
        let is_expanded = *expanded_state.read(cx);

        let (status_icon, status_color) = match status {
            ToolCallStatus::Running => (IconName::Loader, cx.theme().muted_foreground),
            ToolCallStatus::Success => (IconName::Check, cx.theme().success),
            ToolCallStatus::Failed => (IconName::TriangleAlert, cx.theme().danger),
        };
        // 折叠态只展示序号 + 动作摘要（如「[2]:执行命令，ls -la」）；seq=0 为旧记录，退化为纯摘要
        let header_text = if *seq > 0 {
            format!("[{seq}]:{title}")
        } else {
            title.clone()
        };
        let header_summary = truncate_chars(&header_text, TOOL_CARD_HEADER_MAX_CHARS);
        // 展开态显示完整详情：工具名 + 参数摘要 + 输出（失败时为错误输出）
        let detail = Self::tool_call_detail(name, args_summary, output);
        let call_id = call_id.clone();

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
                        Button::new(SharedString::from(format!("tool-call-toggle-{}", msg.id)))
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
                        .child(
                            TextView::markdown(
                                SharedString::from(format!("tool-call-msg-{}", call_id)),
                                detail,
                            )
                            .text_xs()
                            .selectable(true),
                        ),
                )
            })
            .into_any_element()
    }

    /// 组装工具卡片展开态详情：工具名 + 参数摘要 + 输出（失败时输出即错误信息）。
    fn tool_call_detail(name: &str, args_summary: &str, output: &str) -> String {
        let mut detail = format!("**{}** {}", t!("AiChat.tool_call_detail_tool"), name);
        if !args_summary.is_empty() {
            detail.push_str(&format!(
                "\n**{}** {}",
                t!("AiChat.tool_call_detail_args"),
                args_summary
            ));
        }
        if !output.is_empty() {
            detail.push_str(&format!(
                "\n**{}** {}",
                t!("AiChat.tool_call_detail_output"),
                output
            ));
        }
        detail
    }

    fn render_assistant_content<E: MessageExtension>(
        msg: &ChatMessageUIGeneric<E>,
        code_block_actions: &CodeBlockActionRegistry,
        code_block_renderer: Option<Arc<CodeBlockRenderer>>,
    ) -> AnyElement {
        let view_id = SharedString::from(format!("ai-msg-{}", msg.id));
        // 从 msg.content 提取 `` 块（兼容 provider 把 `` 写入 content 字段的场景）
        let parsed = split_thinking_blocks(&msg.content);
        let body_content = parsed.body.clone();
        let thinking_content = parsed.thinking.clone();

        let container = v_flex().w_full().gap_1();
        let container = if let Some(thinking) = thinking_content {
            container.child(
                ThinkingPanel::new(thinking)
                    .with_state_key(SharedString::from(format!("{}-thinking", view_id))),
            )
        } else {
            container
        };

        if code_block_actions.is_empty() {
            // 无代码块操作，简单渲染
            container
                .child(
                    TextView::markdown(view_id, body_content)
                        .p_3()
                        .selectable(true)
                        .when_some(code_block_renderer, |view, renderer| {
                            view.code_block_renderer(
                                move |code_block, options, element, window, cx| {
                                    renderer(code_block, options, element, window, cx)
                                },
                            )
                        }),
                )
                .into_any_element()
        } else {
            // 有代码块操作，使用 code_block_actions
            let registry = code_block_actions.clone();
            container
                .child(
                    TextView::markdown(view_id, body_content)
                        .code_block_actions(move |code_block, _window, _cx| {
                            let code = code_block.code();
                            let lang = code_block.lang();
                            let lang_str = lang.as_ref().map(|s| s.as_ref());
                            let matched_actions = registry.get_actions_for_lang(lang_str);

                            let mut row = h_flex()
                                .gap_1()
                                .child(Clipboard::new("copy").value(code.clone()));

                            for (idx, action) in matched_actions.iter().enumerate() {
                                let btn_id = SharedString::from(format!("{}-{}", action.id, idx));
                                let callback = action.callback.clone();
                                let icon = action.icon.clone();
                                let label = action.label.clone();
                                let code = code.to_string();
                                let lang = lang.as_ref().map(|s| s.to_string());
                                let mut btn =
                                    Button::new(btn_id).icon(icon).ghost().xsmall().on_click({
                                        let code = code.clone();
                                        let lang = lang.clone();
                                        move |_, window, cx| {
                                            callback(code.clone(), lang.clone(), window, cx);
                                        }
                                    });

                                if let Some(lbl) = label {
                                    btn = btn.tooltip(lbl);
                                }

                                row = row.child(btn);
                            }

                            row
                        })
                        .when_some(code_block_renderer, |view, renderer| {
                            view.code_block_renderer(
                                move |code_block, options, element, window, cx| {
                                    renderer(code_block, options, element, window, cx)
                                },
                            )
                        })
                        .p_3()
                        .selectable(true),
                )
                .into_any_element()
        }
    }

    /// 渲染单条消息（通用路由）
    pub fn render_message<E: MessageExtension>(
        msg: &ChatMessageUIGeneric<E>,
        code_block_actions: &CodeBlockActionRegistry,
        code_block_renderer: Option<Arc<CodeBlockRenderer>>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        match msg.role {
            ChatRole::User => Self::render_user_message(msg, cx),
            ChatRole::Assistant => match &msg.variant {
                MessageVariant::Status { title, is_done } => {
                    Self::render_status_message(&msg.id, title, *is_done, cx)
                }
                MessageVariant::Text => Self::render_assistant_text(
                    msg,
                    code_block_actions,
                    code_block_renderer,
                    window,
                    cx,
                ),
                MessageVariant::ToolCall { .. } => Self::render_tool_call_message(msg, window, cx),
                MessageVariant::SqlResult => {
                    // SqlResult 需要特殊渲染，默认只显示占位符
                    div()
                        .w_full()
                        .py_2()
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(t!("AiChat.sql_result").to_string()),
                        )
                        .into_any_element()
                }
            },
            ChatRole::System => Self::render_system_message(msg, cx),
        }
    }
}

/// 工具卡片 header 摘要的最大字符数
const TOOL_CARD_HEADER_MAX_CHARS: usize = 80;

/// 截断为最多 max_chars 个字符（超出时追加省略标记）
fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

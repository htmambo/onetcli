//! ChatMessageRenderer - 共享消息渲染工具
//!
//! 提供通用的消息渲染函数，可被不同的面板复用。
//! SQL 面板可以在此基础上覆盖特定渲染（如 SQL 代码块）。

use crate::ai_chat::panel::CodeBlockActionRegistry;
use crate::ai_chat::reasoning;
use crate::ai_chat::thinking::{ThinkingPanel, split_thinking_blocks};
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
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

/// 解析结果缓存的最大条目数，超出后淘汰最久未使用的条目
const PARSED_CONTENT_CACHE_CAPACITY: usize = 64;

/// 内容摘要：长度 + 128 位哈希
///
/// 使用 128 位摘要而非 64 位，是为了把碰撞概率压到可忽略的量级——
/// 哈希碰撞会让不同内容共享同一条缓存记录，从而**静默**渲染出错误正文。
type ContentDigest = (usize, u128);

/// 消息内容解析结果的缓存条目
///
/// 与 `split_thinking_blocks` 的返回结果一一对应，唯一的差别是正文以
/// `SharedString` 保存：命中缓存时克隆只是引用计数递增，正文不会被再次拷贝。
struct ParsedContentCacheEntry {
    /// 剥离思考块后的正文
    body: SharedString,
    /// 合并后的思考内容
    ///
    /// 保持 `String` 而非 `SharedString`：`ThinkingPanel::new` 的入参是
    /// `impl Into<String>`，最终必然要产出一份拥有所有权的 `String`，
    /// 缓存里存 `String` 可以少一次 `Arc` → `String` → `Arc` 的往返转换。
    thinking: Option<String>,
    /// 最近一次访问序号，用于 LRU 淘汰
    last_used: u64,
}

/// 按内容摘要缓存解析结果的容器
#[derive(Default)]
struct ParsedContentCache {
    entries: HashMap<ContentDigest, ParsedContentCacheEntry>,
    /// 单调递增的访问序号
    clock: u64,
}

impl ParsedContentCache {
    /// 取出缓存条目并刷新其访问序号
    fn get(&mut self, digest: ContentDigest) -> Option<(SharedString, Option<String>)> {
        // 先确认命中，再推进逻辑时钟——未命中不应消耗时钟
        let entry = self.entries.get_mut(&digest)?;
        let clock = self.clock.wrapping_add(1);
        self.clock = clock;
        entry.last_used = clock;
        Some((entry.body.clone(), entry.thinking.clone()))
    }

    /// 写入缓存条目，容量超限时淘汰最久未使用的条目
    fn insert(&mut self, digest: ContentDigest, body: SharedString, thinking: Option<String>) {
        let clock = self.clock.wrapping_add(1);
        self.clock = clock;

        // 已存在同一条摘要时直接覆盖，避免误淘汰其它仍然有效的条目
        if let Some(entry) = self.entries.get_mut(&digest) {
            entry.body = body;
            entry.thinking = thinking;
            entry.last_used = clock;
            return;
        }

        if self.entries.len() >= PARSED_CONTENT_CACHE_CAPACITY {
            // 先取出最久未使用的键（复制出来，避免迭代器借用与后续 remove 冲突）
            // 容量固定为 64，O(n) 扫描的开销可以忽略
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| *key);
            if let Some(oldest) = oldest {
                self.entries.remove(&oldest);
            }
        }

        self.entries.insert(
            digest,
            ParsedContentCacheEntry {
                body,
                thinking,
                last_used: clock,
            },
        );
    }
}

thread_local! {
    /// 按内容摘要缓存 `` 块的解析结果
    ///
    /// 放在 `thread_local` 中：UI 渲染集中在主线程，无需加锁避免争用；
    /// 不同线程各自持有独立缓存，互不影响（含测试线程之间的隔离）。
    ///
    /// 注意：`split_thinking_blocks` 内部不得再回调本缓存，否则 `RefCell`
    /// 会在同一线程上被重复借用而 panic。
    static PARSED_CONTENT_CACHE: RefCell<ParsedContentCache> =
        RefCell::new(ParsedContentCache::default());
}

/// 计算内容摘要：长度 + FNV-1a 128
///
/// 摘要只占固定大小，不会像完整内容那样在缓存中长期驻留大字符串。
///
/// 采用 128 位摘要而非 64 位，是为了把碰撞概率压到可忽略的量级——
/// 哈希碰撞会让不同内容共享同一条缓存记录，从而**静默**渲染出错误正文。
/// 单趟扫描即可算出全部 128 位，比"两个 64 位哈希"少一次全文遍历。
fn content_digest(content: &str) -> ContentDigest {
    (content.len(), fnv1a_128(content.as_bytes()))
}

/// FNV-1a 128 位哈希
///
/// 单趟扫描、无额外依赖；输出 128 位，用作缓存键的碰撞概率可忽略。
fn fnv1a_128(bytes: &[u8]) -> u128 {
    const FNV_OFFSET_BASIS: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const FNV_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in bytes {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// 按内容摘要获取 `` 块的解析结果
///
/// 命中缓存时直接复用已解析的正文与思考内容，未命中时才做一次全文扫描，
/// 避免 AI 流式输出期间每帧都为每条消息重新扫描并分配字符串。
///
/// **命中率的来源**：消息列表每帧会整体重绘（历史消息内容不变），
/// 因此绝大多数消息每帧都命中缓存；只有正在流式输出的那一条内容在变，
/// 每帧必然未命中——这部分开销无法避免，因为内容确实变了。
///
/// 摘要在此处内部计算，调用方无需关心摘要与内容的对应关系。
fn cached_split_thinking_blocks(content: &str) -> (SharedString, Option<String>) {
    let digest = content_digest(content);
    PARSED_CONTENT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();

        // 命中：直接复用，不再重新扫描与分配
        if let Some(hit) = cache.get(digest) {
            return hit;
        }

        // 未命中：重新解析并写入缓存
        let parsed = split_thinking_blocks(content);
        let body = SharedString::from(parsed.body);
        let thinking = parsed.thinking;

        // 缓存里存一份副本，自身这份留给本次渲染使用
        cache.insert(digest, body.clone(), thinking.clone());

        (body, thinking)
    })
}

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
        // 解析结果按内容摘要缓存，流式输出期间可避免每帧重复全文扫描与分配
        let (body_content, thinking_content) = cached_split_thinking_blocks(&msg.content);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_split_returns_same_result_as_direct_split() {
        let content = "<think>reasoning</think>body";
        let (body, thinking) = cached_split_thinking_blocks(content);
        assert_eq!(body.as_str(), "body");
        assert_eq!(thinking.as_deref(), Some("reasoning"));
    }

    #[test]
    fn cache_hit_reuses_parsed_result() {
        let content = "<think>a</think>b";
        let first = cached_split_thinking_blocks(content);
        let second = cached_split_thinking_blocks(content);
        assert_eq!(first.0.as_str(), second.0.as_str());
        assert_eq!(first.1.as_deref(), second.1.as_deref());
    }

    #[test]
    fn different_content_yields_different_digest() {
        assert_ne!(content_digest("abc"), content_digest("abd"));
        assert_ne!(content_digest("abc"), content_digest("abcd"));
    }

    #[test]
    fn changed_content_is_not_served_from_cache() {
        // 同一份摘要契约下，内容变化必须得到不同的解析结果
        let (body_a, _) = cached_split_thinking_blocks("<think>a</think>body-a");
        let (body_b, _) = cached_split_thinking_blocks("<think>b</think>body-b");
        assert_eq!(body_a.as_str(), "body-a");
        assert_eq!(body_b.as_str(), "body-b");
    }

    #[test]
    fn cache_evicts_when_capacity_exceeded() {
        // 写入超过容量的条目后，最旧的条目应被淘汰
        for i in 0..(PARSED_CONTENT_CACHE_CAPACITY + 10) {
            let content = format!("content-{}", i);
            let _ = cached_split_thinking_blocks(&content);
        }
        PARSED_CONTENT_CACHE.with(|cache| {
            let cache = cache.borrow();
            assert!(cache.entries.len() <= PARSED_CONTENT_CACHE_CAPACITY);
        });
    }
}

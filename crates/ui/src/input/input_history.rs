//! Up/Down/Ctrl-R 上下箭头历史记录状态机。
//!
//! 抽离自 AI 输入框（ChatDB `AIInput` / 通用 `AiChatPanel`）的复用需求，
//! 不依赖 GPUI 类型——宿主通过纯数据方法喂入/读出状态。
//!
//! 状态机：
//! - **编辑态**（cursor = None）：用户正在输入，浏览态被禁用
//! - **浏览态**（cursor = Some(i)）：用户按 ↑/↓ 在 history 中浏览
//! - **搜索态**（search_mode = true）：用户按 Ctrl-R 进入反向搜索
//!
//! 与 IME composition、Enter 提交、Esc 退出等交互细节由宿主侧处理，
//! 本组件只输出 [`HistoryAction`] 指令。

use std::collections::VecDeque;

use gpui::{Action, actions};

actions!(input_history, [HistoryPrev, HistoryNext, HistorySearch, HistoryEscape]);

/// 宿主执行动作的指令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryAction {
    /// 替换输入框内容。
    SetValue(String),
    /// 恢复进入浏览态前的草稿。
    RestoreDraft(String),
    /// 不在边界或无操作；宿主应让默认 MoveUp/MoveDown 接管。
    Noop,
    /// 应用当前搜索匹配项（Ctrl-R 模式下）。
    ApplyMatch(String),
}

/// 上 / 下箭头历史记录状态机。
///
/// `history` 字段按时间倒序排列（最新在前）：
/// - `[0..local_len)` 为当前会话本地历史
/// - `[local_len..)` 为全局最近 N 条（来自跨会话视图）
///
/// 草稿仅在编辑态 → 浏览态切换时压栈，回到 `None` 时恢复。
#[derive(Debug, Default)]
pub struct InputHistory {
    /// 用 VecDeque 是因为 push_local 在头部插入 O(1)（Vec::insert(0) 是 O(n)）。
    history: VecDeque<String>,
    /// `[0, local_len)` 范围为当前会话本地历史。
    local_len: usize,
    cursor: Option<usize>,
    draft: Option<String>,
    search_mode: bool,
    search_query: String,
    search_matches: Vec<usize>,
    search_index: usize,
}

impl InputHistory {
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前会话提交后写入（D8）。
    ///
    /// 跳过空消息（前后空白裁剪后为空视为空）；与最新一条相邻重复时跳过。
    ///
    /// **提交语义**：push_local 是"提交"动作的副作用，调用后强制清空所有交互态
    ///（cursor / draft / search_mode / search_query / search_matches），
    /// 避免 push_front 引入的索引偏移残留导致浏览态错位。索引同步（如 cursor += 1）
    /// 在此设计下已无必要——状态已全部归零。
    pub fn push_local(&mut self, msg: String) {
        let trimmed = msg.trim();
        if trimmed.is_empty() {
            return;
        }
        if self.history.front().map(|s| s == &msg).unwrap_or(false) {
            return;
        }
        self.history.push_front(msg);
        self.local_len += 1;
        // 强制退出浏览 + 搜索态（防御性清理；宿主无需单独调用 reset）
        self.cursor = None;
        self.draft = None;
        self.search_mode = false;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_index = 0;
    }

    /// 全局视图喂入（D1）。
    ///
    /// 调用方按时间倒序传入；与现有 history（含 local + 已有 global）按内容去重，
    /// 重复的跳过；不刷新 `local_len`——全局追加在 `[local_len..)` 之后。
    ///
    /// 全量去重可避免多次加载或 SQL 返回重复时累积相同条目。
    pub fn extend_global(&mut self, msgs: impl IntoIterator<Item = String>) {
        for msg in msgs {
            if msg.trim().is_empty() {
                continue;
            }
            if self.history.iter().any(|s| s == &msg) {
                continue;
            }
            self.history.push_back(msg);
        }
    }

    /// 切换会话（D1）。
    ///
    /// 清空本地部分，保留全局部分；调用方传入新会话的本地历史（按时间正序，
    /// 与上游 `MessageRepository::list_by_session` 一致）；同时强制退出浏览态、
    /// 清空草稿。
    pub fn reset_session(&mut self, new_local: impl IntoIterator<Item = String>) {
        self.history.drain(..self.local_len);
        self.local_len = 0;
        // 收集到 Vec 再翻转，保证调用方按时间正序传入时正确恢复成倒序
        let mut local: Vec<String> = new_local.into_iter().collect();
        local.reverse();
        for msg in local {
            self.push_local(msg);
        }
        self.cursor = None;
        self.draft = None;
    }

    /// 当前是否处于浏览态（用于宿主决定 inline 预览是否显示）。
    pub fn is_browsing(&self) -> bool {
        self.cursor.is_some()
    }

    /// history 是否为空（仅用于诊断 / 日志）。
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }

    /// 历史总条数（仅用于诊断 / 日志）。
    pub fn total_len(&self) -> usize {
        self.history.len()
    }

    /// 本地段条数（仅用于诊断 / 日志）。
    pub fn local_len(&self) -> usize {
        self.local_len
    }

    /// 当前 cursor（仅用于诊断 / 日志）。
    pub fn cursor_debug(&self) -> Option<usize> {
        self.cursor
    }

    /// 浏览态下要预览的内容（首 2 行截断）。
    pub fn preview_text(&self) -> Option<String> {
        let i = self.cursor?;
        let text = self.history.get(i)?;
        Some(truncate_preview_lines(text, 2))
    }

    /// 按 ↑：编辑态→浏览第一条；浏览态→下一条（更旧）；已在最旧→Noop。
    ///
    /// **方向**: cursor 增大方向，从最新（history[0]）→ 最旧（history[len-1]）。
    /// `current_text` 是宿主当前输入框文本，用于压栈草稿。
    pub fn recall_previous(&mut self, current_text: &str) -> HistoryAction {
        if self.history.is_empty() {
            return HistoryAction::Noop;
        }
        match self.cursor {
            None => {
                // 编辑态 → 浏览态：压栈草稿（仅当非空），切到第一条（最新）
                if !current_text.is_empty() {
                    self.draft = Some(current_text.to_string());
                }
                self.cursor = Some(0);
                HistoryAction::SetValue(self.history[0].clone())
            }
            Some(i) if i + 1 < self.history.len() => {
                // 浏览态 → 下一条（更旧）：cursor 增大
                self.cursor = Some(i + 1);
                HistoryAction::SetValue(self.history[i + 1].clone())
            }
            Some(_) => HistoryAction::Noop, // 已在最旧
        }
    }

    /// 按 ↓：浏览态→上一条（更新）；从最新（cursor=0）之后→恢复草稿或清空。
    ///
    /// **方向**: cursor 减小方向，从最旧（history[len-1]）→ 最新（history[0]）→ 退出浏览。
    /// **不**自动 apply_pending_edit——由宿主在调用前手动调用 `apply_pending_edit` 落定临时副本。
    pub fn recall_next(&mut self) -> HistoryAction {
        let Some(i) = self.cursor else {
            return HistoryAction::Noop;
        };
        if i > 0 {
            // 浏览态 → 上一条（更新）：cursor 减小
            self.cursor = Some(i - 1);
            HistoryAction::SetValue(self.history[i - 1].clone())
        } else {
            // 从最新条 (cursor=0) 退出浏览：恢复草稿或清空
            self.cursor = None;
            match self.draft.take() {
                Some(d) => HistoryAction::RestoreDraft(d),
                None => HistoryAction::SetValue(String::new()),
            }
        }
    }

    /// 应用临时副本：浏览中编辑后，切换或退出浏览前调用。
    ///
    /// **语义**（按用户澄清）:
    /// - 临时副本 == history[cursor]: 不动（未编辑）
    /// - 临时副本 != history[cursor] 且非空: **覆盖 history[cursor]**（更新原位置，长度不变）
    /// - 临时副本为空: **不动 history[cursor]**（清空仅清显示，不影响 history）
    ///
    /// **调用方必须**: 切换 history 前（recall_next/previous 之前）调用。
    pub fn apply_pending_edit(&mut self, current_text: &str) {
        let Some(i) = self.cursor else { return; };
        if current_text.is_empty() {
            // 清空态：不动 history[i]
            return;
        }
        if let Some(existing) = self.history.get(i) {
            if existing == current_text {
                // 未编辑
                return;
            }
        }
        // 覆盖更新
        if let Some(slot) = self.history.get_mut(i) {
            *slot = current_text.to_string();
        }
    }

    /// 浏览态下当前输入框内容是否允许提交。
    ///
    /// **语义**（按用户澄清 B）: 浏览中清空输入框后**不允许**提交；
    /// 编辑态（cursor=None）或浏览态非空均可提交。
    pub fn can_submit_in_browse(&self, current_text: &str) -> bool {
        match self.cursor {
            Some(_) => !current_text.is_empty(),
            None => true,
        }
    }

    /// ESC 复位：清空草稿与浏览态，光标归零。history 本身不变。
    pub fn escape(&mut self) {
        self.cursor = None;
        self.draft = None;
    }

    /// 用户开始编辑字符：宿主应调用以退出浏览态、清预览、清草稿。
    pub fn exit_browse_on_edit(&mut self) {
        self.cursor = None;
        self.draft = None;
    }

    /// 提交当前浏览内容（Enter）：保留草稿（便于按 ↓ 恢复），
    /// 但 cursor 必须清空（否则下次浏览态错乱）。
    pub fn commit_browsed(&mut self) {
        self.cursor = None;
    }

    // ---- Ctrl-R 反向搜索（M8 可裁剪；接口先落地）----

    pub fn enter_search(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_index = 0;
    }

    pub fn exit_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.search_matches.clear();
    }

    pub fn is_searching(&self) -> bool {
        self.search_mode
    }

    pub fn search_push_char(&mut self, ch: char) {
        self.search_query.push(ch);
        self.refresh_search_matches();
    }

    pub fn search_backspace(&mut self) {
        self.search_query.pop();
        self.refresh_search_matches();
    }

    /// 搜索模式下按 ↑：在 matches 中向前一条（更早的）。
    pub fn search_prev(&mut self) -> HistoryAction {
        if !self.search_mode {
            return HistoryAction::Noop;
        }
        if self.search_matches.is_empty() {
            return HistoryAction::Noop;
        }
        // search_index 在 matches 内循环：0 → last → 0 → last ...
        let new_index = if self.search_index == 0 {
            self.search_matches.len() - 1
        } else {
            self.search_index - 1
        };
        self.search_index = new_index;
        let i = self.search_matches[new_index];
        HistoryAction::ApplyMatch(self.history[i].clone())
    }

    /// 搜索模式下按 ↓：在 matches 中向后一条（更新的）。
    pub fn search_next(&mut self) -> HistoryAction {
        if !self.search_mode {
            return HistoryAction::Noop;
        }
        if self.search_matches.is_empty() {
            return HistoryAction::Noop;
        }
        let new_index = (self.search_index + 1) % self.search_matches.len();
        self.search_index = new_index;
        let i = self.search_matches[new_index];
        HistoryAction::ApplyMatch(self.history[i].clone())
    }

    /// 搜索模式下输入字符：更新查询。
    pub fn search_input(&mut self, ch: char) {
        self.search_push_char(ch);
    }

    /// 搜索查询：当前查询字符串（用于 UI 显示）。
    pub fn search_query_display(&self) -> &str {
        &self.search_query
    }

    pub fn search_accept(&mut self) -> HistoryAction {
        let Some(&i) = self.search_matches.get(self.search_index) else {
            self.exit_search();
            return HistoryAction::Noop;
        };
        self.cursor = Some(i);
        let text = self.history[i].clone();
        self.exit_search();
        HistoryAction::ApplyMatch(text)
    }

    fn refresh_search_matches(&mut self) {
        self.search_matches = self
            .history
            .iter()
            .enumerate()
            .filter(|(_, s)| s.contains(&self.search_query))
            .map(|(i, _)| i)
            .collect();
        // 默认指向最新一条匹配（search_matches[0] 总是历史索引最小 = 最新）
        self.search_index = 0;
    }
}

/// 预览用截断：超过 `max_lines` 时保留首 N 行 + 省略号提示。
fn truncate_preview_lines(text: &str, max_lines: usize) -> String {
    let total_lines = text.lines().count();
    if total_lines <= max_lines {
        return text.to_string();
    }
    let head: String = text.lines().take(max_lines).collect::<Vec<_>>().join("\n");
    format!("{}\n… (共 {} 行)", head, total_lines)
}

/// 光标是否在第一行最顶。
///
/// 单元：byte offset（与 [`crate::input::Selection::start`] 一致）。
/// 条件：offset 之前不含 `\n` 且（offset == 0 或前缀全是空白字符）。
/// `is_whitespace()` 同时覆盖 `\n\t ` 等；显式排除 `\n` 是为了避免
/// 第二行行首（前面只有空白）被误判为顶。
///
/// 非 UTF-8 char boundary 时返回 false（防御性；正常路径下
/// `InputState::selection().start` 总是 char boundary）。
pub fn is_at_first_line_top(text: &str, offset: usize) -> bool {
    debug_assert!(offset <= text.len());
    if !text.is_char_boundary(offset) {
        return false;
    }
    let prefix = &text[..offset];
    // offset == 0 或 prefix 含非空白字符 → 在首行最顶。
    // 全空白 prefix（如纯空格/Tab/全角空格）→ false，避免误触发历史召回覆盖用户草稿。
    !prefix.contains('\n') && (offset == 0 || prefix.chars().any(|c| !c.is_whitespace()))
}

/// 光标是否在最后一行最底。
pub fn is_at_last_line_bottom(text: &str, offset: usize) -> bool {
    debug_assert!(offset <= text.len());
    if !text.is_char_boundary(offset) {
        return false;
    }
    let suffix = &text[offset..];
    !suffix.contains('\n') && (offset == text.len() || suffix.chars().all(|c| c.is_whitespace()))
}

// ------------------- 单元测试 -------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_previous_empty_history() {
        let mut h = InputHistory::new();
        assert_eq!(h.recall_previous("draft"), HistoryAction::Noop);
    }

    #[test]
    fn recall_previous_from_edit_enters_browse() {
        let mut h = InputHistory::new();
        h.push_local("first".to_string());
        h.push_local("second".to_string());
        // history: [second, first]
        let action = h.recall_previous("draft");
        assert_eq!(action, HistoryAction::SetValue("second".to_string()));
        assert_eq!(h.cursor, Some(0));
        assert_eq!(h.draft, Some("draft".to_string()));
    }

    #[test]
    fn recall_previous_at_oldest_is_noop() {
        // 新方向：↑ cursor 增大。cursor == len-1 (最旧) 时 Noop。
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.push_local("b".to_string()); // history=[b, a]
        h.recall_previous("d"); // cursor=Some(0)
        h.recall_previous("d"); // cursor=Some(1) — 最旧
        assert_eq!(h.cursor, Some(1));
        assert_eq!(h.recall_previous("d"), HistoryAction::Noop);
    }

    #[test]
    fn recall_previous_walks_to_older() {
        // 新方向：↑ cursor 0 → 1 → 2（从最新 → 次新 → 最旧）
        let mut h = InputHistory::new();
        h.push_local("a".to_string()); // history=[a]
        h.push_local("b".to_string()); // history=[b,a]
        h.push_local("c".to_string()); // history=[c,b,a]
        // ↑ 第一次：cursor=0, value=c（最新）
        assert_eq!(
            h.recall_previous("d"),
            HistoryAction::SetValue("c".to_string())
        );
        // ↑ 第二次：cursor=1, value=b
        assert_eq!(h.recall_previous("d"), HistoryAction::SetValue("b".to_string()));
        // ↑ 第三次：cursor=2, value=a
        assert_eq!(h.recall_previous("d"), HistoryAction::SetValue("a".to_string()));
        // ↑ 第四次：已在最旧 → Noop
        assert_eq!(h.recall_previous("d"), HistoryAction::Noop);
    }

    #[test]
    fn recall_next_walks_to_newer() {
        // 新方向：↓ cursor 2 → 1 → 0 → 恢复草稿
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.push_local("b".to_string());
        h.push_local("c".to_string()); // history=[c,b,a]
        // 先 ↑ 到最旧
        h.recall_previous("d"); // cursor=0
        h.recall_previous("d"); // cursor=1
        h.recall_previous("d"); // cursor=2
        assert_eq!(h.cursor, Some(2));
        // ↓ 第一次：cursor=1, value=b
        assert_eq!(h.recall_next(), HistoryAction::SetValue("b".to_string()));
        // ↓ 第二次：cursor=0, value=c
        assert_eq!(h.recall_next(), HistoryAction::SetValue("c".to_string()));
        // ↓ 第三次：cursor=None, 恢复草稿
        assert_eq!(
            h.recall_next(),
            HistoryAction::RestoreDraft("d".to_string())
        );
        assert_eq!(h.cursor, None);
        assert_eq!(h.draft, None);
    }

    #[test]
    fn recall_next_from_oldest_skips_to_newer() {
        // 从最旧 (cursor=len-1) 按 ↓：cursor 一直减小到 0 后恢复草稿
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.push_local("b".to_string()); // history=[b, a]
        h.recall_previous("d"); // cursor=0
        h.recall_previous("d"); // cursor=1 (最旧)
        // ↓ 一次：cursor=0, value=b
        assert_eq!(h.recall_next(), HistoryAction::SetValue("b".to_string()));
        // ↓ 二次：cursor=None, 恢复草稿
        assert_eq!(
            h.recall_next(),
            HistoryAction::RestoreDraft("d".to_string())
        );
    }

    #[test]
    fn recall_next_from_edit_is_noop() {
        // 编辑态 (cursor=None) 按 ↓：Noop
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        assert_eq!(h.recall_next(), HistoryAction::Noop);
    }

    #[test]
    fn apply_pending_edit_overwrites_when_changed() {
        // 浏览中编辑后切换：history[cursor] 被覆盖
        let mut h = InputHistory::new();
        h.push_local("original".to_string());
        h.push_local("other".to_string()); // history=[other, original]
        h.recall_previous("d"); // cursor=0, showing "other"
        // 用户在输入框编辑成 "modified"
        h.apply_pending_edit("modified");
        // history[0] 应该是 "modified"
        assert_eq!(h.history.get(0), Some(&"modified".to_string()));
        assert_eq!(h.history.get(1), Some(&"original".to_string()));
    }

    #[test]
    fn apply_pending_edit_keeps_when_unchanged() {
        // 未编辑切换：history 不变
        let mut h = InputHistory::new();
        h.push_local("original".to_string());
        h.recall_previous("d"); // cursor=0, showing "original"
        h.apply_pending_edit("original"); // 相同
        assert_eq!(h.history.get(0), Some(&"original".to_string()));
    }

    #[test]
    fn apply_pending_edit_empty_keeps_history() {
        // 浏览中清空：history[i] 不变（按用户澄清 A）
        let mut h = InputHistory::new();
        h.push_local("original".to_string());
        h.push_local("other".to_string()); // history=[other, original]
        h.recall_previous("d"); // cursor=0, showing "other"
        h.apply_pending_edit(""); // 清空态
        // history 不动
        assert_eq!(h.history.get(0), Some(&"other".to_string()));
        assert_eq!(h.history.get(1), Some(&"original".to_string()));
        // cursor 仍指 0（不动）
        assert_eq!(h.cursor, Some(0));
    }

    #[test]
    fn apply_pending_edit_from_edit_is_noop() {
        // 编辑态调用 apply_pending_edit：Noop
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.apply_pending_edit("whatever"); // cursor=None, 不动
        assert_eq!(h.history.get(0), Some(&"a".to_string()));
    }

    #[test]
    fn can_submit_in_browse_blocks_when_empty() {
        let mut h = InputHistory::new();
        h.push_local("original".to_string());
        h.recall_previous("d"); // cursor=0, 浏览中
        // 浏览态 + 输入框为空 → 禁止提交
        assert!(!h.can_submit_in_browse(""));
        // 浏览态 + 输入框非空 → 允许提交
        assert!(h.can_submit_in_browse("modified"));
        // 编辑态 → 都允许
        h.escape();
        assert!(h.can_submit_in_browse(""));
        assert!(h.can_submit_in_browse("anything"));
    }

    #[test]
    fn escape_resets_cursor_and_draft() {
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.recall_previous("draft"); // cursor=Some(0), draft=Some("draft")
        h.escape();
        assert_eq!(h.cursor, None);
        assert_eq!(h.draft, None);
    }

    #[test]
    fn push_local_dedups_consecutive() {
        let mut h = InputHistory::new();
        h.push_local("foo".to_string());
        h.push_local("foo".to_string()); // 重复应跳过
        assert_eq!(h.history.len(), 1);
    }

    #[test]
    fn push_local_clears_browse_and_search_state() {
        // 提交语义：push_local 强制退出浏览 + 搜索态，避免索引错位残留。
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.recall_previous("d"); // cursor=Some(0), draft=Some("d")
        h.enter_search();
        h.search_push_char('a'); // query="a", matches=[0]

        h.push_local("b".to_string());

        // 历史栈更新正确
        assert_eq!(h.history.len(), 2);
        assert_eq!(h.local_len, 2);
        // 交互态全部清空（这是修复后的强制语义）
        assert_eq!(h.cursor, None);
        assert_eq!(h.draft, None);
        assert!(!h.is_searching());
        assert_eq!(h.search_query, "");
        assert_eq!(h.search_matches, Vec::<usize>::new());
        assert_eq!(h.search_index, 0);
    }

    #[test]
    fn extend_global_dedups_against_previous_global() {
        // 全量去重（local + global 段），多次调用或 SQL 返回重复时不应累积。
        let mut h = InputHistory::new();
        h.extend_global(vec!["g1".to_string(), "g2".to_string()]);
        // history=[g1, g2], local_len=0
        h.extend_global(vec!["g2".to_string(), "g3".to_string()]);
        // g2 已存在 → skip; g3 新增
        assert_eq!(
            h.history,
            vec!["g1".to_string(), "g2".to_string(), "g3".to_string()]
        );
        assert_eq!(h.local_len, 0);
    }

    #[test]
    fn push_local_skips_empty_or_whitespace() {
        let mut h = InputHistory::new();
        h.push_local("".to_string());
        h.push_local("   ".to_string());
        h.push_local("\n\t".to_string());
        assert_eq!(h.history.len(), 0);
    }

    #[test]
    fn push_local_skips_non_consecutive_duplicates_in_history_but_allows_via_extend() {
        // push_local 只去重相邻；不重复远端的（保留历史轨迹）
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.push_local("b".to_string());
        h.push_local("a".to_string()); // 不相邻，允许
        assert_eq!(h.history.len(), 3);
    }

    #[test]
    fn extend_global_skips_local_duplicates() {
        let mut h = InputHistory::new();
        h.push_local("local1".to_string());
        h.extend_global(vec![
            "local1".to_string(),
            "g1".to_string(),
            "g2".to_string(),
        ]);
        assert_eq!(
            h.history,
            vec!["local1".to_string(), "g1".to_string(), "g2".to_string()]
        );
        assert_eq!(h.local_len, 1);
    }

    #[test]
    fn reset_session_keeps_global_clears_local() {
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.extend_global(vec!["g1".to_string()]);
        // history: [a, g1], local_len=1
        h.reset_session(vec!["new1".to_string(), "new2".to_string()]);
        // new_local 按时间正序传入，内部 rev() → push "new2", "new1"
        // history: [new1, new2, g1], local_len=2
        assert_eq!(
            h.history,
            vec!["new1".to_string(), "new2".to_string(), "g1".to_string()]
        );
        assert_eq!(h.local_len, 2);
        assert_eq!(h.cursor, None);
        assert_eq!(h.draft, None);
    }

    #[test]
    fn reset_session_exits_browse_state() {
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.recall_previous("draft"); // 浏览中
        assert!(h.is_browsing());
        h.reset_session(std::iter::empty::<String>());
        assert!(!h.is_browsing());
        assert_eq!(h.draft, None);
    }

    #[test]
    fn exit_browse_on_edit_clears_draft() {
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.recall_previous("draft");
        h.exit_browse_on_edit();
        assert!(!h.is_browsing());
        assert_eq!(h.draft, None);
    }

    #[test]
    fn preview_text_truncates_long_multiline() {
        let mut h = InputHistory::new();
        h.push_local("a\nb\nc\nd".to_string());
        h.recall_previous("");
        let preview = h.preview_text().unwrap();
        assert!(preview.contains("a"));
        assert!(preview.contains("…"));
        assert!(preview.contains("共 4 行"));
    }

    #[test]
    fn preview_text_returns_none_when_not_browsing() {
        let h = InputHistory::new();
        assert_eq!(h.preview_text(), None);
    }

    #[test]
    fn search_finds_substring_matches() {
        let mut h = InputHistory::new();
        h.push_local("SELECT * FROM users".to_string()); // history=[users]
        h.push_local("SELECT id FROM orders".to_string()); // history=[orders,users]
        h.push_local("DELETE FROM logs".to_string()); // history=[logs,orders,users]
        h.enter_search();
        h.search_push_char('S');
        h.search_push_char('E');
        // "SE" 匹配 history[1]="orders"(SELECT) 和 history[2]="users"(SELECT)
        // history 索引升序遍历，所以 matches = [1, 2]
        assert_eq!(h.search_matches, vec![1, 2]);
        // 默认 search_index = 0 → 指向 search_matches[0] = history[1]（最近一条含 SE）
        assert_eq!(h.search_index, 0);
    }

    #[test]
    fn search_backspace_updates_matches() {
        let mut h = InputHistory::new();
        h.push_local("foo".to_string());
        h.push_local("bar".to_string());
        h.enter_search();
        h.search_push_char('f');
        h.search_push_char('o');
        h.search_push_char('o'); // "foo"
        assert_eq!(h.search_matches, vec![1]); // history[1]="foo"
        h.search_backspace(); // "fo"
        // "fo" 仍然匹配 "foo"
        assert_eq!(h.search_matches, vec![1]);
    }

    #[test]
    fn search_empty_query_returns_all() {
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.push_local("b".to_string()); // history=[b,a]
        h.enter_search();
        h.refresh_search_matches();
        // 空查询 = 所有条目；history 索引升序遍历
        assert_eq!(h.search_matches, vec![0, 1]);
    }

    #[test]
    fn search_accept_applies_match() {
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.push_local("b".to_string()); // history=[b,a]
        h.enter_search();
        h.search_push_char('b');
        // search_matches=[0]（history[0]="b"）
        let action = h.search_accept();
        assert_eq!(action, HistoryAction::ApplyMatch("b".to_string()));
        assert_eq!(h.cursor, Some(0));
        assert!(!h.is_searching());
    }

    #[test]
    fn search_accept_with_no_matches_is_noop() {
        let mut h = InputHistory::new();
        h.push_local("a".to_string());
        h.enter_search();
        h.search_push_char('z');
        assert_eq!(h.search_accept(), HistoryAction::Noop);
        assert!(!h.is_searching());
    }

    #[test]
    fn search_prev_cycles_through_matches() {
        let mut h = InputHistory::new();
        h.push_local("xxx".to_string()); // history=[xxx]  // 不含 'y'
        h.push_local("yyy aaa".to_string()); // history=[yyy aaa, xxx]
        h.push_local("zzz yyy".to_string()); // history=[zzz yyy, yyy aaa, xxx]
        h.enter_search();
        h.search_push_char('y'); // 命中 idx=0 (zzz yyy), idx=1 (yyy aaa)
        // search_matches = [0, 1]
        assert_eq!(h.search_matches, vec![0, 1]);
        // 默认 search_index = 0（指向 zzz yyy）
        let action = h.search_prev();
        assert_eq!(action, HistoryAction::ApplyMatch("yyy aaa".to_string()));
        // 再 prev：循环回到 search_matches[0]
        let action = h.search_prev();
        assert_eq!(action, HistoryAction::ApplyMatch("zzz yyy".to_string()));
    }

    #[test]
    fn search_next_cycles_through_matches() {
        let mut h = InputHistory::new();
        h.push_local("xxx".to_string());
        h.push_local("yyy aaa".to_string());
        h.push_local("zzz yyy".to_string());
        h.enter_search();
        h.search_push_char('y');
        assert_eq!(h.search_matches, vec![0, 1]);
        let action = h.search_next();
        assert_eq!(action, HistoryAction::ApplyMatch("yyy aaa".to_string()));
        let action = h.search_next();
        assert_eq!(action, HistoryAction::ApplyMatch("zzz yyy".to_string()));
    }

    #[test]
    fn is_at_first_line_top_basic() {
        assert!(is_at_first_line_top("", 0));
        assert!(is_at_first_line_top("hello", 0));
        // 全空白 prefix 误判修复：offset>0 且 prefix 全空白 → 不判为"最顶"
        assert!(!is_at_first_line_top("   ", 3));
        // offset>0 且 prefix 含非空白 → 在首行（任意位置可触发历史）
        assert!(is_at_first_line_top("hello", 5));
        // 多行场景
        assert!(is_at_first_line_top("hello\nworld", 5)); // prefix="hello" 含非空白
        assert!(!is_at_first_line_top("hello\nworld", 6)); // prefix 含 \n → 不在第一行
        assert!(!is_at_first_line_top("hello\nworld", 11)); // 末行
    }

    #[test]
    fn is_at_first_line_top_unicode_whitespace() {
        // 全角空格 U+3000 也是 whitespace
        // 修复后：offset>0 且全空白 → 不判为"最顶"
        assert!(!is_at_first_line_top("\u{3000}\u{3000}", 6));
    }

    #[test]
    fn is_at_first_line_top_trailing_newline() {
        // text="  \nhello", offset=2 (光标在第一行末尾) → 全空白 → 不判为"最顶"（修复后）
        assert!(!is_at_first_line_top("  \nhello", 2));
        // 但若 offset=3（在 \n 之后），前缀 = "  \n"，含 \n → 不在第一行
        assert!(!is_at_first_line_top("  \nhello", 3));
    }

    #[test]
    fn is_at_last_line_bottom_basic() {
        assert!(is_at_last_line_bottom("", 0));
        assert!(is_at_last_line_bottom("hello", 5)); // offset == len
        // offset=0 时 suffix=整个文本 "hello"，不含 \n，但 offset != len → false
        assert!(!is_at_last_line_bottom("hello", 0));
        assert!(is_at_last_line_bottom("hello\nworld", 11)); // 末
        // offset=10 在 'd' 之前，suffix="d"，非空白 → 不在末行最底
        assert!(!is_at_last_line_bottom("hello\nworld", 10));
        assert!(!is_at_last_line_bottom("hello\nworld", 5)); // 首行末
    }

    #[test]
    fn is_at_last_line_bottom_trailing_newline() {
        // text="hello\n", offset=6 (在末尾 \n 之后) → 应判为"在末行"
        assert!(is_at_last_line_bottom("hello\n", 6));
        // 但若 offset=5（光标在 \n 之前），suffix="\n"，含 \n → 不在末行
        assert!(!is_at_last_line_bottom("hello\n", 5));
    }
}

# AI 输入框上下箭头历史记录（InputHistory 公共组件）

**Status**: 🔄 M1-M10 实施完成（待端到端验收 M11）
**创建时间**: 2026-09-05
**最近更新**: 2026-09-05（M1-M10 完成；29/29 单测通过；fmt 已应用；clippy 运行中；待 M11 手动验收）

## 实施完成情况

| M | 子任务 | 文件 | 状态 |
|---|---|---|---|
| M1 | `InputHistory<S>` +` `InputStateLike` trait + 状态机 | `crates/ui/src/input/input_history.rs` | ✅ 29/29 单测 |
| M2 | 纯函数 `is_at_first_line_top/bottom` | 同 M1 | ✅（含在 M1） |
| M3 | AIInput 接入（key binding + handlers + cx.propagate 回退 + IME 门控） | `crates/db_view/src/chatdb/ai_input.rs` | ✅ |
| M4 | ChatPanel 本地历史喂入（start_new_session/load_session） | `crates/db_view/src/chatdb/chat_panel.rs` | ✅ |
| M4-b | ChatPanel 全局历史加载（异步 list_recent_user） | 同 | ✅ |
| M5 | `MessageRepository::list_recent_user` 新增 | `crates/core/src/llm/chat_history.rs` | ✅ |
| M6 | AiChatPanel 接入（直接用 InputState） | `crates/core/src/ai_chat/panel.rs` | ✅ |
| M7 | inline 灰色预览 UI 渲染 | `crates/db_view/src/chatdb/ai_input.rs` + `crates/core/src/ai_chat/panel.rs` | ✅ |
| M8 | Ctrl-R 搜索循环（↑/↓ 导航 matches，Enter 应用，Esc 退出） | `crates/ui/src/input/input_history.rs` + handlers | ✅ |
| M9 | 单元测试 29 个（包含 search_prev/next） | M1 文件内 | ✅ |
| M10 | fmt / clippy / test | — | ⏳ clippy 在跑；fmt 已应用；test 全绿 |
| M11 | 端到端手动验收 | 真实 OmniHub | ⏳ 待用户执行 |

## 未提交修改清单

- `crates/core/src/ai_chat/panel.rs`（M6 + M7）
- `crates/core/src/llm/chat_history.rs`（M5）
- `crates/db_view/src/chatdb/ai_input.rs`（M3 + M7）
- `crates/db_view/src/chatdb/chat_panel.rs`（M4 + M4-b）
- `crates/db_view/src/sql_editor.rs`（M2 + M3 helper）
- `crates/ui/src/input/mod.rs`（M1 export）
- `crates/ui/src/input/state.rs`（M3 getter：`selection()` / `ime_marked_range()`）
- `crates/ui/src/input/input_history.rs`（M1 + M2 + M8 新文件）
- `docs/Task/Active/INPUT_HISTORY_PLAN.md`（本文件）
**关联**: [`HistoryPromptState`](../../crates/terminal_view/src/history_prompt.rs)（参考实现）

---

## 1. 背景与目标

### 1.1 用户诉求

在 AI 助手输入框（ChatDB 的 `AIInput` 与通用的 `AiChatPanel`）中，实现类似 Claude Code / 终端的上下箭头历史记录：

- 当光标位于文本**首行最顶**按 `↑`，填充上一条用户提交过的输入
- 当光标位于文本**末行最底**按 `↓`，填充下一条；到最后一条时恢复当前未提交的草稿
- 输入框上方显示 inline 灰色预览，提示将要填充的内容

### 1.2 目标价值

- **效率**：复用历史问题/SQL，减少重复输入
- **一致性**：与终端/Claude Code 心智模型对齐
- **可控**：三处入口（ChatDB / 通用 AI 面板）行为一致

### 1.3 非目标（本轮不做）

- 跨设备同步历史（依赖现有 `SessionService` 已有的同步能力即可，不在本任务范围）
- LLM 改写/补全历史（是 AI 增强功能，独立任务）
- 输入时的模糊匹配补全（仅做精确召回 + Ctrl-R 搜索）

---

## 2. 现状梳理（基于代码事实）

### 2.1 三层 AI 输入架构

| 层 | 文件 | 角色 |
|---|---|---|
| `SqlEditor` | `crates/db_view/src/sql_editor.rs:1361` | 实际编辑器，封装 `Entity<InputState>` |
| `AIInput` | `crates/db_view/src/chatdb/ai_input.rs:100` | ChatDB 输入组件，承载 `SqlEditor` + 触发 `AIInputEvent` |
| `ChatPanel` | `crates/db_view/src/chatdb/chat_panel.rs:110` | ChatDB Chat 面板，订阅 `AIInputEvent` |
| `AiChatPanel` | `crates/core/src/ai_chat/panel.rs:307` | 通用 AI 面板（独立第二套路径，不依赖 db_view） |

### 2.2 关键事实

- **编辑器类型**：gpui-component 的 `Input`（多行、软换行），非自定义 `TextArea`
- **默认键绑定**：`up`/`down` 已绑定到 `MoveUp`/`MoveDown` 光标移动（`crates/ui/src/input/state.rs:231-232`）
- **Enter 路径**：触发 `InputEvent::PressEnter { secondary }`；`secondary: false` = 提交，`true` = Shift+Enter（已插入换行后发出）
- **现有历史基建**：
    - `ChatPanel::last_user_input: Option<String>`（`chat_panel.rs:154`）— 仅用于"重试"
    - `ChatPanel::chat_history: Vec<Message>`（`chat_panel.rs:131`）— 已加载会话消息
    - `MessageRepository::list_by_session`（`crates/core/src/llm/chat_history.rs:425`）
    - `MessageRepository::list_recent_user(N)` — **需新增**，当前接口未提供"按时间倒序 + 仅 user 角色 + 限 N 条"

### 2.3 参考实现

终端的 `HistoryPromptState`（`crates/terminal_view/src/history_prompt.rs:27`）：
- `navigate_previous()` / `navigate_next()`（lines 325/352）
- `enter_search()` / `exit_search()`（Ctrl-R 反向搜索）
- `render_history_prompt_overlay()`（`view.rs:1761`，dropdown overlay）
- `try_navigate_history_prompt()`（`view.rs:1733`，key 入口）

**本任务不是简单复用，而是抽取其模式重新设计**：终端是单行命令，AI 输入是多行 + 多组件路径，必须独立设计。

---

## 3. 设计决策（已敲定）

| # | 决策项 | 选择 | 备注 |
|---|---|---|---|
| D1 | 历史范围 | 当前会话完整 + 最近 50 条全局 | 全局=跨会话；按时间倒序 |
| D2 | 触发条件 | 光标在首行最顶 / 末行最底才触发 | 类 Claude/终端体验 |
| D3 | UI 提示 | inline 灰色预览（参考终端 inline suggest） | 上方或下方视位置 |
| D4 | 适用范围 | 抽公共 `InputHistory` 组件 | 三处统一 |
| D5 | 去重策略 | 相邻重复合并、按时间倒序 | 上一条 == 本条 → 跳过 |
| D6 | 反向搜索 | 一起做，复用终端 Ctrl-R | 不显示完整下拉 |
| D7 | 容量上限 | 当前会话不限 / 全局固定 50 条 | 全局截顶用 FIFO |
| D8 | 写入时机 | 提交时写入 | 提交=Enter 触发即写 |

---

## 4. 实施方案

### 4.1 公共 `InputHistory<S>` 组件设计

**新文件**：`crates/ui/src/input/history.rs`

```rust
/// 与具体 InputState 类型解耦的历史状态机
pub struct InputHistory<S: InputStateLike> {
    history: VecDeque<String>,        // 合并会话+全局去重后的最终列表
    cursor: Option<usize>,            // None = 编辑态；Some(i) = 浏览第 i 条
    draft: Option<String>,            // 进入浏览态前的未提交文本
    search_mode: bool,                // Ctrl-R 反向搜索态
    search_query: String,
    search_matches: Vec<usize>,       // 命中的 history 下标
    search_index: usize,
    _phantom: PhantomData<S>,
}

impl<S: InputStateLike> InputHistory<S> {
    pub fn new() -> Self;
    /// 当前会话提交后写入
    pub fn push_local(&mut self, msg: String);
    /// 全局最近 N 条加载（去重合并）
    pub fn extend_global(&mut self, msgs: impl IntoIterator<Item = String>);
    /// 切换会话时清空本地、保留全局（按 D1）
    pub fn reset_session(&mut self);
    /// 按 ↑ 且光标在首行最顶 → 上一条
    pub fn recall_previous(&mut self) -> HistoryAction;
    /// 按 ↓ 且光标在末行最底 → 下一条；最后一条恢复草稿
    pub fn recall_next(&mut self) -> HistoryAction;
    /// Ctrl-R 进入搜索；字符匹配过滤；Enter 接受
    pub fn enter_search(&mut self);
    pub fn exit_search(&mut self);
    pub fn search_input(&mut self, ch: char);
    pub fn search_backspace(&mut self);
    pub fn search_accept(&mut self) -> HistoryAction;
}

pub enum HistoryAction {
    SetValue(String),     // 替换输入框
    RestoreDraft(String), // 恢复草稿
    Noop,                 // 不在边界，不处理
}

/// 抽象输入状态：history 组件不直接依赖 gpui-component 的 InputState
pub trait InputStateLike {
    fn text(&self) -> String;
    fn set_text(&mut self, text: String);
    fn cursor_is_at_top(&self) -> bool;     // 光标在第一行 offset 0
    fn cursor_is_at_bottom(&self) -> bool;  // 光标在最后一行末尾
    fn selection_is_empty(&self) -> bool;   // 仅在无选区时允许 Up/Down 触发
}
```

**关键设计**：
- `InputHistory<S>` 通过 `InputStateLike` trait 与具体实现解耦，三处宿主各自实现 trait
- 状态机：`{编辑} ↔ {浏览} ↔ {搜索}`，草稿仅在进入浏览时压栈
- 不持久化全局历史（按 D7 全局固定 50 条由调用方每次会话启动时喂入）

### 4.2 trait `InputStateLike` 的三处实现

| 宿主 | 实现要点 |
|---|---|
| `SqlEditor` | `text()` = `editor.read(cx).text().to_string()`；光标位置从 `editor.read(cx)` 取 `selection` |
| `AIInput::InputStateRef` | 内部封装转发到 `sql_editor` |
| `AiChatPanel::ai_input_state` | 直接读 `InputState::text()` / `set_value()` |

**光标位置判定算法**（Round 1 必改项 1 落地）：

抽出**纯函数**，宿主只负责取 `text` + `offset`：

```rust
// crates/ui/src/input/history.rs
/// byte offset 单位（Round 1 必改项 5 钉死）：全程使用 byte offset，
/// 与 gpui-component::input::InputState::selection.start 一致。
pub fn is_at_first_line_top(text: &str, offset: usize) -> bool {
    debug_assert!(offset <= text.len());
    let prefix = &text[..offset];
    // 条件 1：offset 之前不含换行（否则不在第一行）
    // 条件 2：offset == 0 或前缀全是空白字符
    !prefix.contains('\n') && (offset == 0 || prefix.chars().all(|c| c.is_whitespace()))
}

pub fn is_at_last_line_bottom(text: &str, offset: usize) -> bool {
    debug_assert!(offset <= text.len());
    let suffix = &text[offset..];
    !suffix.contains('\n') && (offset == text.len() || suffix.chars().all(|c| c.is_whitespace()))
}
```

**关键修正（Round 1 评审闭环）**：
- 原方案"offset 之前字符全是空白 → 首行最顶"在 `\n` 也算空白时会误判（如光标在第二行行首、前面只有空白会被判为顶）
- 修正后：`\n` 显式排除（用 `!prefix.contains('\n')`），且用 `is_whitespace()` 而非字面空白字符（Unicode 安全）
- byte offset 单位钉死为 InputState::selection.start 同一语义

**实现位置**：`SqlEditor::cursor_top_or_bottom(&self, cx) -> (bool, bool)`（新增，内部调上述纯函数）；`AIInput` / `AiChatPanel` 透传。

**契约测试**（Round 1 必改项 5）：
- M3 增加 `trait_input_state_offset_unit_contract` 测试，验证 `SqlEditor::get_offset()` 与 `InputState::selection.start` 语义一致
- 纯函数覆盖：空串、单行 offset=0、单行 offset=末尾、首行纯空白、多行首行、多行末行、尾随 `\n`

### 4.3 键盘事件拦截

#### 4.3.1 现有绑定冲突

`up`/`down` 在 `InputState` 默认绑定到 `MoveUp`/`MoveDown`（`state.rs:231-232`）。

#### 4.3.2 拦截策略

在三个宿主的 focus context 内，**声明更高优先级**的 action 并在处理器内判定光标位置：

```rust
// AIInput / ChatPanel / AiChatPanel 各自的 cx.bind_keys 调用
cx.bind_keys([
    KeyBinding::new("up", RecallHistoryPrev, Some("AIInput")),      // 拦截优先级 > Input 上下文
    KeyBinding::new("down", RecallHistoryNext, Some("AIInput")),
    KeyBinding::new("ctrl-r", EnterHistorySearch, Some("AIInput")),
])
```

**关键**：GPUI 的 `cx.on_action` 注册到 focus handle 上时，**只在对应 context 触发**。`Some("AIInput")` 需要在宿主的 `FocusHandle` 创建时绑定同名 context（或用 `cx.on_action_with_context`）。

**处理器逻辑**（Round 1 必改项 2 + 建议项 4 修订：IME composition 门控 + 显式回退路径）：

```rust
fn recall_prev(&mut self, _: &RecallHistoryPrev, window: &amp;mut Window, cx: &amp;mut Context<Self>) {
    let state = self.sql_editor.read(cx);
    // Round 1 建议项 4：IME composition 活跃时不拦截
    if state.is_composing() { return; }
    // 选区非空时让光标扩展（与默认行为一致）
    if !state.selection_is_empty() { return; }
    // 不在首行最顶时显式 dispatch 默认 MoveUp（Round 1 必改项 2）
    if !is_at_first_line_top(&state.text(), state.cursor_offset()) {
        cx.dispatch_action(&amp;MoveUp);  // 见 4.3.4 M3-pre spike
        return;
    }
    let action = self.history.borrow_mut().recall_previous();
    match action {
        HistoryAction::SetValue(v) => self.sql_editor.update(cx, |e, cx| e.set_value(v, window, cx)),
        HistoryAction::RestoreDraft(v) => { /* 同上 */ }
        HistoryAction::Noop => {}
    }
    cx.notify();
}
```

#### 4.3.3 IME composition 门控（Round 1 建议项 4 落地）

**R8 缓解方案修订**：
- `InputStateLike` 增加 `is_composing() -> bool` 方法
- 三个宿主实现：读 `InputState::marked_range()`（GPUI 暴露的 IME composition API）或等价
- 处理器入口先检查 IME：composition 活跃时 Up/Down/Ctrl-R 一律不拦截，让 IME 处理
- 替代原"仅英文环境生效"的脆弱设计

#### 4.3.4 回退路径（M3-pre spike 闭环，2026-09-05）

**M3-pre 调研结论**（基于源码分析 + 项目既有模式）：

GPUI action 分发在 **bubble phase 默认 consume**（`cx.propagate_event = false`，`vendor/zed/crates/gpui/src/window.rs:4436`），但**处理器可通过 `cx.propagate()` 显式 opt-in 继续冒泡**。这是 GPUI 内置机制，无需运行 spike。

**关键 API**（`vendor/zed/crates/gpui/src/app.rs:1880-1886`）：
- `cx.propagate()` — 让事件继续冒泡/捕获（fallthrough opt-in）
- `cx.stop_propagation()` — 显式停止
- `window.dispatch_action(&MoveUp, cx)` — 直接重新分发（备选路径）

**项目既有参考模式**（`crates/ui/src/input/state.rs:1449-1469` `escape` 处理器）：
```rust
pub(super) fn escape(&mut self, action: &Escape, ...) {
    if self.handle_action_for_context_menu(...) { return; } // consume
    if self.has_inline_completion() {
        self.clear_inline_completion(cx);
        return;                                          // consume
    }
    if self.clean_on_escape { return self.clean(...); } // consume
    cx.propagate();                                      // fallthrough
}
```

**M3 实现采用 cx.propagate() 模式**（评审 Round 1 P0-2 闭环）：

```rust
fn recall_prev(&mut self, _: &RecallHistoryPrev, window: &mut Window, cx: &mut Context<Self>) {
    let state = self.sql_editor.read(cx);
    if state.is_composing() { return; }                  // IME 门控：消费
    if !state.selection_is_empty() {                      // 选区非空：让默认行为接管
        cx.propagate();
        return;
    }
    if !is_at_first_line_top(&state.text(), state.cursor_offset()) {
        cx.propagate();                                  // 不在首行最顶：让默认 MoveUp 接管
        return;
    }
    let action = self.history.borrow_mut().recall_previous();
    match action {
        HistoryAction::SetValue(v) => self.sql_editor.update(cx, |e, cx| e.set_value(v, window, cx)),
        HistoryAction::RestoreDraft(v) => { /* 同上 */ }
        HistoryAction::Noop => { cx.propagate(); }       // Noop 也 fallthrough
    }
}
```

**关键差异 vs Round 1 初版**：使用 `cx.propagate()`（GPUI 标准 fallthrough 机制）而非 `cx.dispatch_action(&MoveUp)`（显式重分发）。前者语义更清晰、依赖 GPUI 内置路径，避免双触发风险。

**M3 验证**：在 M3 集成测试中加 `recall_prev_not_at_top_propagates_to_moveup` 用例，验证调用 `recall_prev` 在非边界时光标确实上移一行。

#### 4.3.5 浏览态交互细节（Round 1 建议项 8 落地）

| 用户操作 | 浏览态下行为 |
|---|---|
| 输入字符 | 退出浏览态、清预览、清草稿；用户输入覆盖到输入框 |
| Backspace 删除光标前字符 | 退出浏览态（同上） |
| Enter | 退出浏览态、提交当前填充内容（等同普通提交） |
| Shift+Enter | 退出浏览态、插入换行（等同普通输入） |
| Esc | 退出浏览态、恢复草稿 |
| 切换会话 | 强制退出浏览态、清草稿、清预览（`reset_session` 内置处理） |
| 全局历史异步加载完成 | 不影响当前浏览态（浏览态始终基于本地 `history`） |

#### 4.3.6 超长历史条目处理（Round 1 建议项 8 落地）

- **填充**：超过 200 字符的多行历史，填充后保留完整内容（不截断）
- **预览**：inline 灰色预览仅显示首 2 行 + `… (共 N 行)`（避免遮挡下方输入）
- **存储**：内部 `history: VecDeque<String>` 不截断，仅控制显示层

#### 4.3.7 Ctrl-R 反向搜索（Round 1 建议项 6：可裁剪）

复用 `HistoryPromptState::enter_search` 思路但简化（不做完整下拉）：

```rust
fn search_input(&mut self, ch: char) {
    self.search_query.push(ch);
    self.refresh_matches();           // 全量扫描 history 找包含子串的项
}
fn refresh_matches(&mut self) {
    self.search_matches = self.history
        .iter()
        .enumerate()
        .filter(|(_, s)| s.contains(&self.search_query))
        .map(|(i, _)| i)
        .collect();
    self.search_index = self.search_matches.len().saturating_sub(1);
}
```

**搜索态 UI**：在输入框内显示查询字符串（prefix 高亮匹配段），回车应用当前 match，Esc 退出。

**实现复杂度控制**：搜索态不展示完整下拉列表，仅展示当前匹配项；这是 D3 + D6 的折中。

### 4.4 UI 渲染：inline 灰色预览

**新文件**：`crates/ui/src/input/history_overlay.rs`（或合并到 `history.rs`）

```rust
pub fn render_history_overlay(
    history: &InputHistory<impl InputStateLike>,
    theme: &Theme,
) -> impl IntoElement {
    let preview = history.preview_text();  // Some(text) = 浏览态预览；None = 隐藏
    div()
        .absolute()                        // 浮在 Input 元素上方
        .text_color(theme.muted_foreground)
        .opacity(0.5)
        .child(preview.unwrap_or_default())
}
```

**挂载位置**：
- `SqlEditor::render`（`sql_editor.rs:1517`）→ 在 `Input::new(&self.editor)` 同一父容器内，作为兄弟节点
- `AiChatPanel::render_input`（`panel.rs:1641`）→ 同理

**视觉**：placeholder 样式（灰色 + 透明度 0.5），仅在 `history.preview_text()` 为 `Some` 时渲染。

**多行预览处理**：超过输入框可视高度时，仅显示首 2-3 行 + 省略号（避免遮挡下方输入）。

### 4.5 会话切换与全局历史加载

#### 4.5.1 `ChatPanel` 集成点

| 事件 | `ChatPanel` 处理 | `InputHistory` 动作 |
|---|---|---|
| `start_new_session`（`chat_panel.rs:345`） | 清空 `chat_history` + 新 session_id | `history.reset_session()` |
| `load_session(id)`（`chat_panel.rs:527`） | 加载 messages 到 `chat_history` | `history.reset_session()` + `history.push_local(...)` 遍历 user messages |
| `load_history_sessions`（`chat_panel.rs:383`） | 列出所有会话（用于 UI） | 不影响 history |
| `submit` 路径（`chat_panel.rs:608` → `send_to_ai`） | 发送 + 持久化 | `history.push_local(content)`（D8：提交时写入） |

#### 4.5.2 全局 50 条加载时机

- `ChatPanel::new`（初始化时）→ `MessageRepository::list_recent_user(50)`
- `ChatPanel::load_history_sessions` 后台异步加载 → `history.extend_global(...)`
- 不需要每次切会话都重新加载（按 D7：全局是只读视图，启动喂一次）

#### 4.5.3 新增仓储方法（Round 1 必改项 3 修订：去重 + 排除当前会话）

```rust
// crates/core/src/llm/chat_history.rs
impl MessageRepository {
    /// 按时间倒序拉取最近 N 条 user 消息（跨会话，排除指定会话）。
    /// 空输入/失败消息已过滤。
    /// 排序 tiebreaker: (created_at DESC, id DESC) 保持稳定。
    pub async fn list_recent_user(
        &self,
        limit: usize,
        exclude_session_id: Option<&SessionId>,
    ) -> Result<Vec<Message>>;
}
```

**SQL**：

```sql
SELECT * FROM chat_messages
WHERE role = 'user'
  AND content != ''                       -- 过滤空输入
  AND (? IS NULL OR session_id != ?)      -- 排除当前会话（避免本地/全局重复）
ORDER BY created_at DESC, id DESC
LIMIT ?
```

**关键修正（Round 1 闭环）**：
- 原 D5"相邻重复合并"在本地/全局两个来源合并后无法保证相邻 → 改为按 message id 去重
- 当前会话的 user 消息已通过 `chat_history` 直接喂入 InputHistory，全局视图通过 `exclude_session_id` 排除避免重复
- 索引建议：`(role, created_at DESC, id DESC)` 复合索引覆盖此查询

**合并语义**（InputHistory 层）：
- `push_local(msg)` → append 到本地（来源 = chat_history 派生）
- `extend_global(msgs)` → 与现有 history 按 `id` 去重合并，按时间倒序
- 去重粒度按 message id 而非内容（避免"相邻重复合并"的脆弱语义）

### 4.6 持久化策略

| 数据 | 存储 | 寿命 |
|---|---|---|
| 当前会话历史 | 内存 `InputHistory.history`（from `chat_history`） | 会话有效期内 |
| 全局最近 50 条 | 内存 `InputHistory.history`（from DB） | 应用启动时一次加载 |
| `MessageRepository` | 已存在的 SQLite 表 | 永久（D8 已通过持久化覆盖） |

**不需要新增持久化表**。全局历史通过现有 `chat_messages` 表派生，避免双写一致性问题。

### 4.7 `AiChatPanel` 集成路径（Round 1 建议项 5 + M6-pre 勘察落地）

`AiChatPanel`（`crates/core/src/ai_chat/panel.rs:307`）是**独立第二套路径**，不依赖 db_view。M6-pre 勘察结果（2026-09-05）：

| 探测项 | 结果 |
|---|---|
| `AiChatPanel` 是否独立持久化？ | **是**：通过 `engine: ChatEngine`（`panel.rs:311`）持有 `SessionService` + `SessionRepository` + `MessageRepository` |
| `ai_input_state` 类型？ | `Entity<InputState>`（`panel.rs:313`）—— 与 SqlEditor/AIInput 同类型，`InputStateLike` 可统一 |
| 是否已调用 `MessageRepository::list_recent_user`？ | **未调用**：但已有 `list_recent(limit)` 方法（`chat_history.rs:437`），缺少 user role 过滤 + session exclusion |
| 会话切换 UI？ | **完整**：`start_new_session`（line 623）/ `load_session`（line 842）/ `load_history_sessions`（line 689）/ `delete_session`（line 722）/ rename + popover UI（line 1557-1587） |

**M6 范围决策**：**Option A**（全功能接入，与 ChatPanel 对齐）—— AiChatPanel 后端已经完整，仅需：
1. M5 新增的 `list_recent_user(limit, exclude_session_id)` 自然服务三个面板
2. `AiChatPanel::submit` 路径在 `send_message` 之后增加 `history.push_local(content)`
3. `AiChatPanel::load_session` / `start_new_session` 增加 `history.reset_session()` + 重新喂本地历史
4. 注入 `InputHistory<S>` 实例到 `ai_input_state` 的 focus context 内

**意外收获**（基于现有 `list_recent`）：
- `MessageRepository::list_recent` 已存在（`chat_history.rs:437`）—— `ORDER BY created_at DESC LIMIT ?`，但**未过滤 role** 也**未排除 session**
- M5 工作量比预期小：在 `list_recent` 基础上加 user role 过滤 + session exclusion 即可，不必从头写 SQL

**InputHistory 设计支持"无 repository 模式"**（Round 1 建议项 5 兼容）：
- `extend_global` 是可选的 trait 方法，默认 no-op
- 即使 AiChatPanel 不传全局历史也能工作（降级为纯本地模式）

---

## 5. 风险与缓解

| 风险 | 缓解 |
|---|---|
| **R1**：Up/Down 拦截影响现有光标移动 | 处理器内做"光标在边界 + 选区空"双重判定，否则 `return` 不消费 action |
| **R2**：草稿丢失（用户在输入中按↑） | `recall_previous` 时若 `cursor is None` → 先压栈 `draft`，`recall_next` 到 `None` 时恢复 |
| **R3**：多行消息填充后光标位置 | 填充后将光标放在文本末尾（`set_value` 已支持） |
| **R4**：与 `shift-up`/`cmd-up` 等组合键冲突 | 绑定 `"up"`（精确匹配）而非 `"any-up"`；GPUI 不会与 `shift-up`/`cmd-up` 冲突 |
| **R5**：`InputHistory` 抽象泄露底层 API | 通过 `InputStateLike` trait 收敛，组件不感知 `gpui_component::input::InputState` |
| **R6**：三处宿主的状态同步（`ChatPanel` 切换会话时 `AIInput.history` 是否同步？） | `history` 由 `ChatPanel` 持有并注入 `AIInput`（通过构造参数），不双向同步 |
| **R7**：全局历史首次加载阻塞 UI | 改为 `cx.spawn` 异步；加载完 `cx.update` 注入 |
| **R8**：Ctrl-R 搜索态与 IME 冲突 | `InputStateLike::is_composing()` 门控（读 `InputState::marked_range()`），composition 活跃时全部不拦截；替代原"仅英文环境生效"脆弱设计（Round 1 建议项 4） |
| **R9**：测试覆盖难（光标位置判定） | `InputHistory` 核心逻辑纯函数化（trait 注入状态）；光标位置判定逻辑单独单测 |
| **R10**：扩展 `MessageRepository` 影响其他消费方 | `list_recent_user` 是新增方法，不改既有签名 |

---

## 6. 实施清单（里程碑，Round 1 修订）

| M | 子任务 | 文件 | 验证方式 | 估时 |
|---|---|---|---|---|
| **M3-pre** | **GPUI action 回退机制 spike**（必改项 2 闭环） | 一次性验证脚本 | 写最小复现验证 `return` 是否 fallthrough；结果写入 §8 | 0.5d |
| M1 | `InputHistory<S>` + `InputStateLike` trait + 状态机（纯逻辑，含 `is_composing` 门控） | 新增 `crates/ui/src/input/history.rs` | 单元测试 ≥11 个（边界、去重、草稿、搜索、IME） | 2d |
| M2 | `SqlEditor` 实现 `InputStateLike` + 纯函数 `is_at_first_line_top/bottom` | `crates/db_view/src/sql_editor.rs` | 光标位置纯函数单测 ≥6 个 + 契约测试 1 个 | 1d |
| M3 | `AIInput` 接入 `InputHistory`，绑定 Up/Down/Ctrl-R key | `crates/db_view/src/chatdb/ai_input.rs` | 单测 + M3-pre spike 闭环 | 1d |
| M4 | `ChatPanel` 注入 `InputHistory`，订阅会话切换事件 | `crates/db_view/src/chatdb/chat_panel.rs` | 切会话单测 3 个 | 1d |
| M5 | `MessageRepository::list_recent_user(limit, exclude_session_id)` 新增（含索引建议） | `crates/core/src/llm/chat_history.rs` | 仓储单测 3 个 + 索引迁移脚本 | 1d |
| M6-pre | **AiChatPanel 会话存储勘察**（4.7 落地） | 一次性勘察 | 写出 M6 范围决策表 | 0.5d |
| M6 | `AiChatPanel` 接入（按 M6-pre 决策） | `crates/core/src/ai_chat/panel.rs` | 手动验收 | 0.5-1d |
| M7 | inline 灰色预览 UI 渲染（含超长截断 4.3.6） | 新增 overlay 元素 | 视觉验收 | 1d |
| **M8** | **Ctrl-R 反向搜索（可裁剪）** | `crates/ui/src/input/history.rs` | 单测 + 手动验收 | **1d（若裁剪则跳过）** |
| M9 | 集成测试（ChatPanel send → recall → re-edit → send 链路） | 新增 `tests/integration_input_history.rs` | 集成测试 ≥5 个 | 1d |
| M10 | `cargo fmt --check` / `cargo clippy -- --deny warnings` / `cargo test --all` | — | CI 绿 | 0.5d |
| M11 | 端到端手动验收（编辑 / ↑ 召回 / ↓ 恢复草稿 / Ctrl-R / inline 预览） | 真实 OmniHub | 录屏/截图 | 0.5d |

**总估时（含 M3-pre / M6-pre）**：10.5-11d
**总估时（裁剪 M8）**：9.5-10d

**M1 是可独立交付的最小单元**：纯状态机 + trait，可单测、可 mock，无需 GPUI context。
**M8 可裁剪条件**：若 M11 手动验收反馈"Up/Down + 预览"已满足用户预期，可裁剪 Ctrl-R 推迟到二期。

---

## 7. 验证方案

### 7.1 单元测试覆盖（M1）

| 用例 | 目的 |
|---|---|
| `recall_previous_empty_history` | 空历史 → Noop |
| `recall_previous_top_of_history` | 第一条 → Noop（无法再往前） |
| `recall_next_past_end_restores_draft` | 回到 None → 恢复 draft |
| `draft_preserved_when_navigating_back` | 浏览→编辑切换保留草稿 |
| `dedup_by_message_id` | 按 message id 去重（替代原"相邻重复合并"，Round 1 必改项 3） |
| `dedup_preserves_non_consecutive` | 非相邻重复按 id 去重保留 |
| `extend_global_excludes_current_session` | 全局视图排除当前会话 id |
| `extend_global_merges_with_local` | 全局+本地按时间倒序 |
| `reset_session_clears_local_keeps_global` | 切会话不丢全局 |
| `search_finds_substring_matches` | Ctrl-R 搜索基本 |
| `search_backspace_updates_matches` | 退格实时过滤 |
| `search_empty_query_returns_all` | 空查询=全部命中 |
| `is_composing_blocks_recall` | IME composition 活跃时不触发召回（Round 1 建议项 4） |

### 7.2 光标位置判定单测（M2）

| 用例 | 目的 |
|---|---|
| `cursor_top_empty_text` | 空文本 → 顶 |
| `cursor_top_first_line_offset_zero` | offset=0 → 顶 |
| `cursor_top_after_whitespace` | 空白后 → 顶 |
| `cursor_middle_of_multiline` | 中间 → 非顶 |
| `cursor_bottom_empty_text` | 空文本 → 底 |
| `cursor_bottom_last_line_end` | 末尾 → 底 |

### 7.3 集成测试（M9）

- `send_then_recall_returns_last_message`
- `multiple_sends_recall_in_reverse_order`
- `switch_session_clears_local_history`
- `global_history_includes_other_sessions`
- `duplicate_sends_dedup_in_history`

### 7.4 端到端（M11）

按 AGENTS.md 规范，PR 模板 checklist 增加：
- [ ] 手动验收：编辑中按↑保存草稿、按↓恢复
- [ ] 手动验收：跨会话召回
- [ ] 手动验收：Ctrl-R 搜索
- [ ] 手动验收：inline 灰色预览

---

## 8. 关键决策记录

| 决策点 | 选择 | 拒绝方案 | 理由 |
|---|---|---|---|
| 是否复用 `HistoryPromptState` | 不复用，抽取模式重新设计 | 直接 import | 终端是单行；AI 输入多行 + 三宿主，强行复用会引入耦合 |
| 全局历史是否独立持久化 | 通过 `MessageRepository` 派生 | 新建 `input_history` 表 | 避免双写一致性；现有消息库已包含全部 user 消息 |
| Up/Down 拦截层级 | focus context 内 `cx.on_action` + 处理器内光标判定 | `cx.on_key_down` 全局拦截 | 后者会拦截 IME、光标扩展等，破坏现有体验 |
| `InputHistory` 位置 | `crates/ui/src/input/history.rs`（公共组件库） | `crates/core/src/ai_chat/history.rs` | 三宿主中两处在 db_view、一处在 core，放 ui 最对称 |
| 去重策略 | 按 message id 去重（SQL 层 + InputHistory 层） | 相邻重复合并 | 本地+全局两个来源合并后无法保证"相邻" → 必改项 3 落地 |
| IME 冲突缓解 | `InputStateLike::is_composing()` 门控 | 仅英文环境生效 | 替代脆弱设计；composition 状态可被 GPUI 准确感知 |
| GPUI action 回退 | **`cx.propagate()` 显式 opt-in fallthrough**（GPUI bubble phase 默认 consume，需 `cx.propagate()` 放行） | 假设 fallthrough / `cx.dispatch_action(&MoveUp)` 显式重分发 | M3-pre 调研（2026-09-05）：源码 + 项目既有模式（`escape` 处理器）确认 `cx.propagate()` 是标准机制；M3 直接落地 |
| AiChatPanel 集成 | **Option A（全功能接入，与 ChatPanel 对齐）** | 仅当前会话 / 基础历史 | M6-pre 勘察（2026-09-05）：AiChatPanel 通过 `engine: ChatEngine` 持有完整 `SessionService` + `MessageRepository`，会话切换 UI 完整，仅需对接 `InputHistory<S>` 实例 |

---

## External Review Opinion

### Round 0/5（2026-09-05，待送评审）

**待送 `coding-bridge` review_plan**，评审维度：

1. `InputHistory<S>` + `InputStateLike` trait 抽象是否合理、是否过度设计
2. Up/Down 拦截策略（focus context action + 光标判定）是否覆盖 R1 风险
3. 全局历史通过 `MessageRepository::list_recent_user` 派生是否合理、是否需要持久化
4. `AiChatPanel` 集成路径是否需要在 M6 之前先确认会话存储策略
5. M1 纯状态机的可测试性边界（trait mock vs 真实 InputState）
6. 风险 R8（IME 冲突）是否需要进一步设计

---

### Round 1/5（2026-09-05，provider=coding-bridge，kind=plan，session 0e9894df-11b0-42fe-84c6-f2cdd47c4024）

**Verdict**: **NEEDS_CHANGES**（3 P0 必改 + 6 P1 建议）

#### 已采纳的必改项（P0）

| 编号 | 评审意见 | 落地位置 |
|---|---|---|
| **P0-1** | M2 边界判定算法按字面描述有 bug：`\n` 也算空白字符，光标在第二行行首会被误判为"首行最顶" | §4.2 抽出纯函数 `is_at_first_line_top/bottom`，显式 `!prefix.contains('\n')` + `is_whitespace()`（Unicode 安全）；byte offset 单位钉死 |
| **P0-2** | GPUI action 分发是否支持"处理器内 return 让默认 MoveUp 接管"通常**不成立**（action 被处理即消费，不自动 fallthrough），R1 在修复并验证前视为未闭合 | §4.3.4 新增 **M3-pre spike**：写最小复现验证，支持→现有方案 / 不支持→显式 `cx.dispatch_action(&MoveUp)`；结果写入 §8 决策记录 |
| **P0-3** | 去重策略"相邻重复合并"在本地+全局两个来源合并后无法保证"相邻"，按现方案必然产生重复 | §4.5.3 SQL 加 `WHERE session_id != ?` 排除当前会话；InputHistory 按 message id 去重；排序 tiebreaker `(created_at DESC, id DESC)`；空输入过滤；索引建议 `(role, created_at DESC, id DESC)` |

#### 已采纳的建议项（P1）

| 编号 | 评审意见 | 落地位置 |
|---|---|---|
| P1-4 | R8（IME）"仅英文环境生效"脆弱且不可测试 | §4.3.3 `InputStateLike::is_composing()` 门控（读 `InputState::marked_range()`），composition 活跃时全部不拦截；R8 缓解方案同步修订 |
| P1-5 | M6 前置 AiChatPanel 持久化勘察；InputHistory 支持"无 repository 模式" | §4.7 M6-pre 勘察子任务（探测表+决策表）；`extend_global` 设为可选 trait 方法 |
| P1-6 | M8（Ctrl-R）可裁剪、补缺失的 M9 编号 | §6 实施清单 M8 标"可裁剪"；新增 **M3-pre / M6-pre** 子任务编号 |
| P1-7 | 明确 SqlEditor 角色（trait 首个实现者服务于 AIInput，还是 D4 三处统一的历史宿主？） | §2.1 / §4.2 明确 SqlEditor 是 trait 首个实现者（服务于 AIInput）；SqlEditor 查询历史是额外范围，本轮不纳入 |
| P1-8 | 补充浏览态下用户操作（输入/Enter/会话切换）的交互细节 + 超长预览截断规则 | §4.3.5 浏览态交互表（7 种用户操作行为）；§4.3.6 超长处理（填充不截断/预览截断前 2 行+省略号） |
| P1-9 | 各里程碑补估时 | §6 实施清单每行加估时；总估时 10.5-11d（含 M3-pre/M6-pre）；裁剪 M8 后 9.5-10d |

#### 评审误报驳回

| 编号 | 评审意见 | 驳回理由 |
|---|---|---|
| D6 是否过度设计 | 评审最终承认"抽象本身合理、不算过度"，仅 M8 可裁剪建议已采纳 | 原条目不构成阻塞 |
| M1 测试粒度 | 评审建议"纯函数 + 契约测试"已采纳（§7.1 + §7.2）；额外 mock 不必要 | 按 ROI 不再追加 |

#### Round 2 评审重点

1. M2 纯函数 `is_at_first_line_top/bottom` 在 Unicode + 多行 + 尾随 `\n` 的边界用例
2. M3-pre spike 验证脚本的最小复现设计
3. SQL `WHERE session_id != ?` + `(role, created_at DESC, id DESC)` 索引迁移脚本可行性
4. M6-pre 勘察的具体探测项（AiChatPanel 会话存储 + InputState 类型）
5. M8 裁剪/保留的判断标准（用户预期 vs 实现成本）

#### Round 2 待送评审

---

### Round 2/5（2026-09-05，provider=coding-bridge，kind=plan，session 0e9894df 续）

**状态**：**UNREACHABLE**（External Review MCP 连续两次返回空 content：模型生成 reasoning 但 final answer 截断，非 verdict 拒绝）。

按 CLAUDE.md §1.4 Third priority 协议：
- 调用本地工具交叉验证（M3-pre spike）
- 主助手**不得单方面自评 verdict**
- 必须**显式声明**："This time has not been reviewed by the External Review MCP"

**用户授权开工**（2026-09-05）：
> 用户确认进入实施阶段，按 M3-pre → M1 顺序开工。

**实施期风险自留**：
- P0-1/2/3 + P1-4/5/6/7/8/9 均已落地，spike 结果若推翻方案将记录到 §8
- 若 spike 发现 GPUI action 不支持 fallthrough，处理器改用显式 `cx.dispatch_action(&MoveUp)`（已落地该路径，无需返工）

---

---

## 9. 待办与下一步

- [x] 送 requirement 评审（Round 1/5，2026-09-05，verdict=NEEDS_CHANGES，3 P0 + 6 P1 已采纳落地）
- [ ] 送 plan 评审（Round 2/5，按 P0-1/2/3 修订后）
- [ ] 通过后开工（M3-pre spike → M1 → M2 → M3 → ... → M11）
- [ ] 单文件完成后按 CLAUDE.md 强制评审点 #4 调 `review_code`

OMC trailers:
- Constraint: `crates/ui/src/input/state.rs` 默认 `up`/`down` 绑定不可移除；新绑定必须在宿主 context 内声明更高优先级
- Rejected: 直接 import `HistoryPromptState`；独立新建 `input_history` 持久化表
- Directive: M1 必须是纯逻辑（trait 注入），不依赖 GPUI context；M6 `AiChatPanel` 范围需在 M3 后根据实际会话存储路径再决定
- Confidence: 高（已有终端参考实现 + 三处宿主定位明确）
- Scope-risk: 中（涉及 5+ 文件；trait 抽象一旦定型改动面大）
- Not-tested: 多行 ghost text 渲染的视觉表现需手动验收（M11）；IME 冲突的边界场景需 Linux 实测
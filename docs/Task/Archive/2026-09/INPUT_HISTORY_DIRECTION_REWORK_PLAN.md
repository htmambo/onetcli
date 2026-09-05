# AI 输入框历史记录方向 + 浏览语义重构

**Status**: 📋 计划中（待实施）
**创建时间**: 2026-09-05
**前置依赖**: 09e8f125 + d45361fe（M1-M11 实现）+ Round 2 审核（关键 bug 修复完成）

## 背景与目标

### 用户反馈（2026-09-05 实测）
1. **方向反了**：↑ 应该从最新往最旧走，↓ 从最旧往最新走 + 恢复草稿。
2. **临时副本语义**：浏览中编辑输入框是 history[i] 的临时副本，不直接修改原 history[i]。
3. **提交统一为最新**：所有提交（新输入 / 直接发送 / 修改后发送）都压入 history[0]。
4. **清空 = 仅清显示**：清空输入框不影响 history[i]，但清空状态下不允许提交。
5. **ESC**：清空输入框 + 复位（cursor=None, draft=None），history 不变。
6. **删除条目不需要实现**（用户澄清 C）。

### 与 Round 1 / Round 2 的差异

| 维度 | Round 1/2 实现 | 新需求 |
|---|---|---|
| ↑ 方向 | cursor 减小（向 history[0] 最新走） | **cursor 增大**（向 history[len-1] 最旧走） |
| ↓ 方向 | cursor 增大（向 history[len-1] 最旧走） | **cursor 减小**（向 history[0] 最新走） |
| 临时副本 | 无（直接 set_value history[i]） | **新增** apply_pending_edit |
| 清空提交 | 允许 | **禁止** |
| ESC | 未实现 | **新增** |
| 提交语义 | push_local | **保持**（所有提交都 push_local） |

## 完整需求规格

### 状态机
- **编辑态**: `cursor = None`，输入框是用户自由编辑内容
- **浏览态**: `cursor = Some(i)`，输入框是 `history[i]` 的**临时副本**

### 行为表

| 触发 | 条件 | 行为 |
|---|---|---|
| 编辑态按 ↑ | history 非空 | 保存当前输入（若非空）到 `draft`；`cursor = Some(0)`；显示 `history[0]` |
| 编辑态按 ↑ | history 为空 | Noop（fallthrough 到默认 MoveUp） |
| 编辑态按 ↓ | 任意 | Noop（编辑态无 ↑/↓ 历史可走） |
| 浏览态按 ↑（i < len-1） | 边界满足 | 应用临时副本（见应用表）→ `cursor = Some(i+1)`；显示 `history[i+1]` |
| 浏览态按 ↑（i = len-1） | 已在最旧 | Noop（fallthrough 到默认 MoveUp） |
| 浏览态按 ↓（i > 0） | 边界满足 | 应用临时副本 → `cursor = Some(i-1)`；显示 `history[i-1]` |
| 浏览态按 ↓（i = 0） | 边界满足 | 应用临时副本 → `cursor = None`；恢复 `draft`（若非空）或清空 |
| 浏览中编辑 | - | 输入框 = 临时副本，不立即修改 `history[i]` |
| 浏览中清空 | - | 输入框 = `""`；cursor 仍指 i；history[i] 不变；**后续提交被禁止** |
| 浏览态提交 | 输入框非空 | 提交内容压入 `history[0]`；原 history 整体后移；`cursor = None`；`draft = None` |
| 浏览态提交 | 输入框为空 | **禁止提交**（return） |
| 编辑态提交 | 任意 | 提交内容压入 `history[0]`（push_local 已实现） |
| ESC | 任意 | 清空输入框；`cursor = None`；`draft = None`；history 不变 |
| Ctrl-R | 浏览或编辑 | 进入搜索态（同 Round 1） |

### 临时副本应用表（切换或退出浏览前）

| 当前临时副本 vs `history[cursor]` | 处理 |
|---|---|
| 完全相同（未编辑） | 不动 `history[cursor]` |
| 不同（编辑过） | `history[cursor] = 临时副本`（覆盖更新，长度不变） |
| 空（用户清空过） | `history[cursor]` 不变（按用户澄清 A） |

### 边界条件

- **IME composing 活跃**: ↑/↓ 不拦截（让 IME 接管）
- **选区非空**: ↑/↓ fallthrough 到默认 MoveUp/MoveDown
- **光标不在第一行最顶**: ↑ fallthrough
- **光标不在最后行最底**: ↓ fallthrough

## 实施计划

### Phase 1: InputHistory 状态机改造

**目标**: 重写方向 + 新增临时副本应用 API + 空提交判定

**文件**: `crates/ui/src/input/input_history.rs`

**变更**:
1. `recall_previous(&mut self, current_text: &str) -> HistoryAction`:
   - `None` → 保存 draft（若非空）→ `cursor = Some(0)` → SetValue(`history[0]`)
   - `Some(i)` i < len-1 → SetValue(`history[i+1]`) → `cursor = Some(i+1)`（**方向：cursor 增大**）
   - `Some(i)` i = len-1 → Noop（已在最旧）
2. `recall_next(&mut self) -> HistoryAction`:
   - `Some(i)` i > 0 → SetValue(`history[i-1]`) → `cursor = Some(i-1)`（**方向：cursor 减小**）
   - `Some(i)` i = 0 → 应用临时副本 → `cursor = None` → RestoreDraft（draft 非空）或 SetValue("")
   - `None` → Noop
3. **新增** `apply_pending_edit(&mut self, current_text: &str) -> HistoryAction`:
   - 必须在切换或退出浏览前调用
   - 临时副本 vs `history[cursor]`:
     - 相同 → Noop（不修改 history）
     - 不同且非空 → 覆盖 `history[cursor] = current_text` → Noop
     - 空 → 不动 history[cursor]（用户澄清 A）→ Noop
   - 返回 Noop（不改变输入框）
4. **新增** `can_submit_in_browse(&self) -> bool`:
   - `cursor.is_some() && history[cursor].is_empty()` → false（禁止提交）
   - 其他情况 → true

**新增单元测试**:
- `recall_previous_walks_to_older`（覆盖新方向：cursor 0 → 1 → 2）
- `recall_next_walks_to_newer`（覆盖新方向：cursor 2 → 1 → 0）
- `recall_next_from_zero_restores_draft`
- `apply_pending_edit_overwrites_when_changed`
- `apply_pending_edit_keeps_when_unchanged`
- `apply_pending_edit_empty_keeps_history`
- `can_submit_in_browse_blocks_when_empty`

**更新现有测试**:
- `recall_previous_walks_backwards` → 重命名为 `recall_previous_walks_to_older`，反转断言
- `recall_next_walks_forward` → 重命名为 `recall_next_walks_to_newer`，反转断言
- 所有 cursor 移动方向断言需调整

### Phase 2: 宿主页改造（AIInput + AiChatPanel）

**目标**: 同步新方向 + apply_pending_edit + 空提交拦截

**文件**:
- `crates/db_view/src/chatdb/ai_input.rs`
- `crates/core/src/ai_chat/panel.rs`

**变更**:
1. `handle_recall_previous`:
   - 边界判定改用 `is_at_first_line_top`（不变）
   - 调用 `self.history.recall_previous(&text)` 后立即调用 `apply_history_action`
2. `handle_recall_next`:
   - 边界判定改用 `is_at_last_line_bottom`（不变）
   - 调用 `recall_next` 后立即调用 `apply_history_action`
3. Submit 分支:
   - 添加空提交拦截：`if self.history.is_browsing() && content.is_empty() { return; }`
4. **`apply_history_action` 增加前置调用**:
   - 在 `SetValue(v)` / `RestoreDraft(v)` / `ApplyMatch(v)` 之前先调用 `self.history.apply_pending_edit(&current_text)` 落定临时副本
   - current_text 需提前从 editor 读取

**新增需求**:
- AIInput 需要在 handle_recall_previous/next 之前读取当前 text（用于 apply_pending_edit）
- AiChatPanel 同理

### Phase 3: ESC 处理

**目标**: ESC 清空 + 复位

**文件**:
- `crates/db_view/src/chatdb/ai_input.rs`
- `crates/core/src/ai_chat/panel.rs`

**变更**:
1. `cx.bind_keys` 加 `KeyBinding::new("escape", HistoryEscape, None)`
2. 新增 `HistoryEscape` action（在 input_history.rs 中定义）
3. listener 注册到 container:
   ```rust
   .on_action(cx.listener(|this, _, window, cx| {
       this.handle_recall_escape(window, cx);
   }))
   ```
4. `handle_recall_escape`:
   - 清空输入框内容（editor.set_value("", window, cx)）
   - `self.history.escape()`：cursor=None, draft=None
   - `cx.notify()`

**InputHistory 新增**:
```rust
pub fn escape(&mut self) {
    self.cursor = None;
    self.draft = None;
}
```

### Phase 4: 清理诊断日志 + clippy

**目标**: 删除之前的诊断 tracing 日志，恢复干净代码

**变更**:
- 删除 `handle_recall_previous` 中的 `tracing::warn!` 调试日志
- 删除 InputHistory 添加的 `is_empty()` / `total_len()` / `local_len()` / `cursor_debug()`（仅用于诊断）
  - 保留 `is_empty` 因为 `apply_pending_edit` 需要；其他删除
- 跑 `cargo clippy -p gpui-component -p main` 确认无警告

### Phase 5: 编译 + 测试

**目标**: 完整验证

**步骤**:
1. `cargo build -p main` 通过
2. `cargo test -p gpui-component --lib input_history` 全绿
3. `cargo clippy --all` 无 warning（或已知 warning）
4. 删除 PLAN.md 中 Round 2 风险 5/6/7 跟踪项

### Phase 6: Round 3 review_code 循环审核

**目标**: 用 External Review MCP 循环验证每个 Phase

**步骤**:
1. Phase 1 完成后: review_code 验证 InputHistory 方向 + apply_pending_edit + can_submit_in_browse
2. Phase 2 完成后: review_code 验证宿主页接入一致性
3. Phase 3 完成后: review_code 验证 ESC 处理
4. Phase 5 完成后: review_code 综合验证

每轮根据审核结果迭代修复，直至 verdict = APPROVED。

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| **R1**: 现有 31/31 单测因方向反转失败 | Phase 1 同步更新测试断言 |
| **R2**: apply_pending_edit 与 push_local 冲突（边界） | 明确两者职责：apply_pending_edit 仅作用于浏览态切换/退出，push_local 仅在 Submit 时 |
| **R3**: ESC 与 InputState 内部 Escape action 冲突 | bind_keys 用 None predicate（depth 最大）保证优先级 |
| **R4**: 清空提交拦截需在 Submit 路径检查 content.is_empty() + history.is_browsing() | 两条件组合判定 |
| **R5**: 三处宿主（AIInput + AiChatPanel）保持一致性 | 抽取公共 helper 到 InputHistory（apply_pending_edit 已是公共 API） |

## 验证方案

### 单元测试（Phase 1）
- `recall_previous_walks_to_older`: history=[c,b,a]，按 ↑ 顺序显示 c, b, a
- `recall_next_walks_to_newer`: cursor=2，按 ↓ 顺序显示 b, a, draft
- `apply_pending_edit_overwrites_when_changed`: 编辑后切换，原 history[i] 被覆盖
- `apply_pending_edit_keeps_when_unchanged`: 未编辑切换，history 不变
- `apply_pending_edit_empty_keeps_history`: 清空后切换，history[i] 保留原值
- `can_submit_in_browse_blocks_when_empty`: 浏览态空内容返回 false

### 端到端（M11 用户验收）
- [ ] 按 ↑ 从最新往最旧逐条显示
- [ ] 按 ↓ 从当前位置往最新方向回退，最后恢复草稿
- [ ] 浏览中编辑后切换，被编辑条目更新
- [ ] 浏览中清空后切换，history 不变
- [ ] 浏览中清空后按 Enter，无任何反应（不提交）
- [ ] 编辑态提交 → history[0] = 提交内容
- [ ] 浏览态提交 → history[0] = 当前输入框内容
- [ ] ESC → 输入框清空 + cursor=None + draft=None

## 与 Round 2 审核风险追踪的关系

| 风险 ID | 状态 |
|---|---|
| Round 1 #5 handle_recall_* 重复 | 本次 Phase 2 重新评估：apply_pending_edit 落定到 InputHistory 后，三处宿主代码自然收敛 |
| Round 1 #6 异步阻塞 | Round 2e P1-2 已修复（smol::unblock） |
| Round 1 #7 SQL 去重/索引 | 仍 P2（用户未反馈性能问题） |

## OMC trailers

- **Constraint**: 仅 AI 输入框三处宿主接入；gpui-component 默认 MoveUp/MoveDown 保留；新增方向、临时副本语义、空提交拦截
- **Rejected**: 添加专用删除路径（用户澄清 C）；按 ↑ 直接修改 history[i]（采纳临时副本语义）
- **Directive**: 用户需求"提交统一压入最新一条"；方向 ↑→旧、↓→新；浏览中编辑是临时副本不立即修改
- **Confidence**: 高（用户已多次澄清细节）
- **Scope-risk**: 中（涉及 5 文件 + 状态机重写 + 单元测试更新）
- **Not-tested**: M11 真实 OmniHub 手动验收；ESC 在三个宿主中的边界行为

# AI 助手 Thinking 可折叠面板（... 块）

**Status**: 📋 计划中（待实施）
**创建时间**: 2026-09-05
**前置依赖**: InputHistory 方向重构（9c44352f，已推送）

## 背景与目标

### 用户反馈（2026-09-05）
日志中持续出现警告：

```
WARN gpui_component::text::format::markdown: unsupported inline html tag: ParsedDocument {
    source: "",
    blocks: [Root { children: [], span: None }],
}
```

AI 终端操作员的流式输出包含 `... ` 标签泄漏到 markdown 渲染，导致：
1. 警告刷屏
2. 思考内容被丢弃（虽然对最终用户无感知）
3. 调试时无法查看 AI 的推理过程

### 用户决策
**不**剥离丢弃，而是**展示**——默认折叠，支持点击展开：
- 默认显示有限高度（建议 60px ≈ 2 行）
- 支持展开/收起
- 与 `omnihub-tool` 围栏块共存
- 与现有 markdown 渲染兼容

## 根因初步分析

### Provider 行为
`extract_stream_text_parts` (crates/core/src/llm/mod.rs:44) 已正确分离：
- `response.choices.delta.reasoning_any()` → reasoning
- `response.choices.delta.content` → content

但**部分 provider** 可能把 `... ` 当作字面量文本塞进 content 字段（不是 reasoning 字段）。这种情况：
- extract_stream_text_parts 正确分类（content 中含 `... `）
- markdown 渲染器收到 `... ` 字面量 → 解析失败 → 警告 + 静默丢弃
- 最终用户看不到 AI 的思考过程

### 解决方案
**在 chat 消息渲染层**（panel.rs / terminal_operator.rs）做**前置解析**：
1. 检测 `... ` 块（正则匹配，含跨行）
2. 提取为独立 `ThinkingPanel` UI 组件
3. 剩余内容按 markdown 正常渲染
4. 默认折叠 + 可展开

## 完整需求规格

### 行为表

| 触发 | 条件 | 行为 |
|---|---|---|
| AI 消息含 `...xxx...` | - | 提取为 ThinkingPanel（默认折叠），剩余内容按 markdown 渲染 |
| ThinkingPanel 默认显示 | - | 前 2 行 + "展开 ▼" 按钮，高度限制 60px |
| 点击 "展开" | - | 显示完整 thinking 内容 + "收起 ▲" 按钮 |
| 点击 "收起" | - | 回到默认折叠状态 |
| 消息含多个 `... ` 块 | - | 合并为一个 ThinkingPanel（顺序拼接） |
| 消息不含 `... ` | - | 不渲染 ThinkingPanel，按纯 markdown 渲染 |
| ThinkingPanel 在 IME 中按 Enter | - | 不抢焦点，纯展示 |
| 与 `omnihub-tool` 围栏块共存 | - | 两者独立识别，thinking 在前、tool 块在后 |

### UI 规格

**折叠态**：
- 高度限制 60px（≈ 2 行）
- 灰色背景（与 omnihub-tool 围栏块颜色一致）
- 顶部标签：`💭 思考过程`（或中文 i18n）
- 右下角展开按钮 `▼`
- 内容溢出截断省略号

**展开态**：
- 高度自适应
- 同样灰色背景
- 右下角收起按钮 `▲`
- 可滚动

### 与现有功能的关系

| 维度 | `omnihub-tool` 围栏块 | `... ` 块 |
|---|---|---|
| 来源 | AI 工具调用 | AI 推理过程 |
| 显示 | 默认隐藏（按 plan 已实现） | 默认折叠（待实施） |
| 触发 | `strip_tool_record_blocks` 后从 content 中剔除 | 提取到独立 ThinkingPanel |
| 渲染 | 不渲染（已在持久化层剔除） | 折叠 UI |

## 实施计划

### Phase 1: 根因验证 + 通用解析函数

**目标**: 验证 provider 行为 + 抽取公共解析函数

**文件**: `crates/core/src/ai_chat/thinking.rs` (新文件)

**变更**:
1. 创建 `ParsedMessageContent` 结构体：
   ```rust
   pub struct ParsedMessageContent {
       pub thinking: Option<String>,  // 合并后的所有 `` 块内容
       pub body: String,              // 剩余 markdown
   }
   ```
2. 实现 `split_thinking_blocks(content: &str) -> ParsedMessageContent`:
   - 正则: `(?s)<think>(.*?)</think>`（DOTALL 模式，跨行）
   - 多块合并（按出现顺序 join）
   - 块前后空白修剪
3. 单元测试:
   - 单块解析
   - 多块合并
   - 嵌套块（贪婪/非贪婪边界 case）
   - 无块场景
   - 块前后空白处理

### Phase 2: ThinkingPanel 组件

**目标**: 创建可折叠 UI 组件

**文件**: `crates/core/src/ai_chat/thinking.rs` (复用) 或独立 `crates/one_ui/src/thinking_panel.rs`

**变更**:
1. 创建 `ThinkingPanel` 组件（RenderOnce）：
   ```rust
   pub struct ThinkingPanel {
       content: String,
       expanded: bool,
       max_collapsed_height: Pixels,
   }

   impl ThinkingPanel {
       pub fn new(content: impl Into<String>) -> Self { ... }
       pub fn max_collapsed_height(mut self, px: Pixels) -> Self { ... }
   }

   impl RenderOnce for ThinkingPanel { ... }
   ```
2. 内部状态 `expanded` 用 `Rc<RefCell<bool>>` 或 `useState` 风格（RenderOnce 限制）
3. 折叠态：高度限制 + 渐变遮罩 + "展开 ▼" 按钮
4. 展开态：自适应高度 + "收起 ▲" 按钮

**简化方案**：折叠态用 `max-height` CSS + `overflow: hidden`，避免状态管理。

### Phase 3: 集成到 AiChatPanel

**目标**: AiChatPanel 渲染时使用 split_thinking_blocks + ThinkingPanel

**文件**: `crates/core/src/ai_chat/panel.rs`

**变更**:
1. 在消息渲染处（render_message 之类）调用 `split_thinking_blocks`:
   ```rust
   let parsed = split_thinking_blocks(&content);
   v_flex()
       .when_some(parsed.thinking, |this, t| {
           this.child(ThinkingPanel::new(t))
       })
       .child(markdown_render(parsed.body))
   ```
2. 已有 `strip_tool_record_blocks` 调用保持（在 split 之后或之前均可，建议在 split 之后只对 body 做）

### Phase 4: 集成到 Terminal Operator

**目标**: 终端侧栏 AI 助手同样支持

**文件**: `crates/terminal_view/src/agents/terminal_operator.rs`

**变更**: 与 Phase 3 相同模式

### Phase 5: 清理 markdown.rs 警告

**目标**: 移除 cfg!(debug_assertions) warning（修复后无 unsupported tag）

**文件**: `crates/ui/src/text/format/markdown.rs:171-173`

**变更**:
```rust
// 旧:
if cfg!(debug_assertions) {
    tracing::warn!("unsupported inline html tag: {:#?}", el);
}

// 新: 静默（其他类型错误可保留 warn）
```

注意：保留 `failed parsing html` 警告，因为那是真正的解析错误。

### Phase 6: 编译 + 测试 + 循环审核

**步骤**:
1. `cargo build -p main` 通过
2. `cargo test -p one-core --lib thinking` 全绿
3. 调用 External Review MCP `review_code` 验证
4. 删除诊断 tracing 日志
5. 提交 + 推送

## 风险与缓解

| 风险 | 缓解 |
|---|---|
| **R1**: `... ` 块在跨行时正则不匹配 | 使用 DOTALL flag `(?s)` 允许 . 匹配换行 |
| **R2**: 多块嵌套（如 `...思考...结论...思考...`） | 非贪婪匹配 `.*?` 处理嵌套边界；合并所有块 |
| **R3**: ThinkingPanel 渲染阻塞主线程 | 纯展示组件，RenderOnce，无副作用 |
| **R4**: 展开/收起状态在消息列表中重复创建 | 每条消息独立 ThinkingPanel 实例，state 独立 |
| **R5**: 现有 `strip_tool_record_blocks` 与 `split_thinking_blocks` 顺序冲突 | 文档明确：先 split_thinking，后 strip_tool_record（对 body 处理） |
| **R6**: 用户复制消息时 thinking 是否包含 | 复制的是 markdown 源（含 `` 标签），符合"完整保留"原则 |

## 验证方案

### 单元测试（Phase 1）
- `split_thinking_blocks_single`: `<think>a</think>body` → thinking=Some("a"), body="body"
- `split_thinking_blocks_multiple`: `<think>a</think>body<think>c</think>` → thinking=Some("a\n\nc")
- `split_thinking_blocks_multiline`: `<think>line1\nline2</think>` → 跨行保留
- `split_thinking_blocks_empty`: `<think></think>` → thinking=None（空块剔除）
- `split_thinking_blocks_no_block`: `just body` → thinking=None, body="just body"
- `split_thinking_blocks_trims_whitespace`: 块前后空白修剪

### 端到端（M11）
- [ ] AI 助手返回 `... ` 内容 → 看到折叠的 "💭 思考过程" 面板
- [ ] 点击展开 → 显示完整推理
- [ ] 点击收起 → 回到折叠态
- [ ] 多个 `... ` 块 → 合并展示
- [ ] 无 `... ` 块 → 不显示 ThinkingPanel
- [ ] 终端侧栏 AI 助手同样行为
- [ ] 日志中无 `unsupported inline html tag` 警告

## 与 Round 1/2 审核风险追踪的关系

| 风险 ID | 状态 |
|---|---|
| Round 1 #5 handle_recall_* 重复 | 本次 Phase 3/4 重新评估：ThinkingPanel 抽取为公共组件，host 复用 |
| Round 1 #6/#7 | 仍 P3 跟踪 |
| 本次新增 #1 | Provider 不规范输出 `... ` 在 content 字段 |

## OMC trailers

- **Constraint**: 仅 UI 渲染层（panel.rs / terminal_operator.rs）+ 公共解析函数（thinking.rs）；不修改 provider 层（extract_stream_text_parts 已正确）
- **Rejected**: 剥离丢弃 thinking 内容（信息丢失）| 全展开（占用太多空间）| 在 ReasoningDelta 流式阶段就过滤（破坏可观察性）
- **Directive**: 用户明确要求"按常见的 `... ` 处理方式，默认展示有限高度，支持缩放显示"
- **Confidence**: 中（provider 行为不确定，需先验证 root cause）
- **Scope-risk**: 中（涉及 4 文件 + 公共组件抽取 + UI 状态管理）
- **Not-tested**: 真实 AI 输出的 `... ` 块格式（需实测确认）

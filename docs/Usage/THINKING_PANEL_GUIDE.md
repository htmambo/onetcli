# AI 助手思考过程（... 块）展示面板

> **状态**: ✅ 已发布（2026-09-05）
> **适用版本**: omnihub main 分支
> **适用面板**: ChatDB AI 输入框、AI Chat Panel（SSH/Serial 终端侧栏）

## 功能概述

AI 助手的思考过程（`` 块）以**可折叠面板**形式展示：

- 默认**折叠**：限制高度 2 行（≈ 60px），最新思考内容**自动贴底**显示（用户无需展开也能看到最新推理）
- 点击 `▶` **展开**：显示完整思考内容
- 点击 `▼` **收起**：回到折叠态
- 多个 `` 块**自动合并**为一个面板（按出现顺序用空行分隔）
- 未配对的 `` 标签视为普通文本（不剥离）
- 仅在当前对话中显示（**不持久化**）

## 使用方法

### 基本交互

| 状态 | 显示 |
|---|---|
| 折叠 | 标题 `💭 思考过程` + `▶` 按钮 + 最新 2 行思考贴底 |
| 展开 | 标题 + `▼` 按钮 + 完整思考内容 |

### 示例：AI 响应含 `` 块

```
┌─────────────────────────────────────────┐
│ 💭 思考过程                          ▶  │  ← 折叠: 显示最新 2 行
│ I should query the users table first.   │
│ SELECT * FROM users                     │
├─────────────────────────────────────────┤
│ I've queried the users table.           │  ← markdown body
└─────────────────────────────────────────┘
```

点击 `▶` 后：

```
┌─────────────────────────────────────────┐
│ 💭 思考过程                          ▼  │  ← 展开: 完整思考
│ The user wants to check the schema.     │
│ Let me query the users table first.     │
│ SELECT * FROM users                     │
├─────────────────────────────────────────┤
│ I've queried the users table.           │
└─────────────────────────────────────────┘
```

## 实现机制

### 解析时机
在 chat 消息渲染层（`render_assistant_content`）前置解析：
1. 调用 `split_thinking_blocks(msg.content)` 拆分 `` 块
2. 提取的 thinking 字符串渲染为 `ThinkingPanel`
3. 剩余 body 内容按 markdown 正常渲染

### 兼容性
- **流式阶段**：provider 返回 `delta.reasoning` 字段时，由 `render_reasoning_block`（已有）渲染
- **完成/重读阶段**：provider 把 `` 写入 `delta.content` 字段时，由 `ThinkingPanel`（新增）渲染
- 两套机制职责分离，互不冲突

### 状态隔离
每条消息的 `ThinkingPanel` 用 `{msgId}-thinking` 作为 `use_keyed_state` 的 key，多消息展开状态互不污染，重渲染后展开态保持。

### 不持久化
思考内容**不**写入 `chat_messages` 表。仅当前对话 in-memory 显示。重新打开会话后无 thinking 内容。

## 行为表

| 场景 | 行为 |
|---|---|
| AI 返回 `<think>reasoning</think>body` | 折叠态面板 + body markdown |
| 多个 `<think>a</think>between<think>b</think>` | 合并 thinking（`a\n\nb`）+ body（`between`）|
| 多行 `<think>line1\nline2</think>` | thinking = "line1\nline2" 跨行保留 |
| 仅空白 `<think>   \n </think>` | 视为无 thinking，不渲染面板 |
| 未配对 `<think>unclosed body` | 视为普通文本追加到 body |
| 前后夹带 `pre<think>t</think>post` | body = "prepost"，thinking = "t" |
| 内容为空 `<think></think>` | 视为无 thinking |
| 流式阶段 reasoning 字段 | 由 `render_reasoning_block` 渲染（独立组件） |

## 与 omnihub-tool 围栏块的关系

| 维度 | `... ` 块 | `omnihub-tool` 围栏块 |
|---|---|---|
| 来源 | AI 推理过程 | AI 工具调用记录 |
| 处理函数 | `split_thinking_blocks` | `strip_tool_record_blocks` |
| 渲染 | `ThinkingPanel`（折叠） | 已剔除，不渲染 |
| 时机 | 完成/重读时（fallback） | 全程（持久化前剥离） |

两者独立处理，互不干扰。

## 与 Markdown 渲染警告

修复前日志中持续出现：

```
WARN gpui_component::text::format::markdown: unsupported inline html tag
```

修复后：
- UI 层 split_thinking_blocks 已剥离 `` 标签 → markdown 渲染器不再遇到
- `markdown.rs` 把 `cfg!(debug_assertions)` warning 降级为 `tracing::trace`（debug build 才输出 trace）
- 日志噪音消除

## 已知限制

- 嵌套 `` 标签未深度处理（罕见场景，标签会泄漏到 thinking 文本）
- 变体标签 `<thinking>` 等不识别（仅匹配标准 `<think>`）
- 不持久化思考内容（按设计）
- 折叠态高度 60px 是固定上限；长思考被截断（保留最新 60px + 贴底）

## 验证清单

- [ ] AI 助手返回含 `` 块 → 看到折叠面板（最新 2 行贴底）
- [ ] 点击 `▶` → 展开完整思考
- [ ] 点击 `▼` → 收起
- [ ] 多个 `` 块 → 合并展示
- [ ] 无 `` 块 → 不显示面板
- [ ] 日志无 `unsupported inline html tag` warning
- [ ] 多消息间展开状态互不污染

## 故障排查

| 现象 | 可能原因 |
|---|---|
| 不显示折叠面板 | provider 用 `<thinking>` 等变体标签（不支持）；或 thinking 内容被 trim 后为空 |
| 面板内文字截断不完整 | 折叠态仅显示最新 60px；点击 `▶` 查看完整 |
| 多消息展开状态错乱 | 罕见；如果发生，重启应用即可 |

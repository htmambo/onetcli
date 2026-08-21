# AI 终端操作员

终端侧栏的 AI 聊天面板支持 Agent 调度模式：AI 可以读取终端输出、生成并执行命令、汇报结果，形成"读 → 执行 → 汇报"闭环。

## 启用与开关

- 默认启用。设置项 `ai_terminal_agent_enabled`（`GlobalChatSettings`）控制；关闭后终端侧栏 AI 聊天降级为纯聊天模式（只生成命令建议，由用户手动粘贴执行）。
- Agent 模式仅在终端桥接可用时激活；桥接不可用时静默降级为纯聊天（日志中有 `tracing::warn` 降级记录）。

## 使用方式

1. 打开任意终端（本地或 SSH），在侧栏打开 AI 聊天面板。
2. 直接描述任务，例如"查看当前目录并列出文件"。
3. Agent 会自动选择工具逐步执行：
   - `get_terminal_list` / `focus_terminal`：枚举与切换终端
   - `read_terminal_output` / `get_terminal_cwd` / `get_terminal_selection`：读取上下文
   - `write_to_terminal`：写入命令并等待输出
   - `task_complete`：结束任务并总结
4. 每次工具调用在聊天流中显示为可折叠卡片，头部为 `[序号]:动作摘要`（如 `[1]:获取所有终端`、`[2]:执行命令，ls -la`、`[3]:任务完成，正在准备回复`，序号在每次请求内从 1 递增）；状态由左侧色条与图标（转圈/√/!）表达；展开后可查看工具名、完整参数与输出摘要。

## 高危命令确认

命中风险规则的写入命令（如 `rm -rf`）会弹出确认对话框，显示命令全文与风险等级；只有你点击"执行"后才会写入终端，拒绝则不会执行。

## 会话持久化

工具调用记录以 `omnihub-tool` 代码块形式随助手消息持久化；重新加载会话后，这些记录会重建为只读工具卡片。

## 限制

- 需要支持工具调用（function calling）的 Provider（OpenAI 兼容系列）；Anthropic/Ollama 等不支持的 Provider 会收到明确报错提示。
- 本地终端命令完成检测为启发式（静默期判定），交互式命令（top、vim、sudo 密码提示）可能判定不准。
- 模型上下文无独立 token 预算，复用聊天面板的 `history_count` 截断；工具输出超过阈值会被截断。

## 假性终结防护（Reminder 机制）

`task_complete` 仍是唯一明确的完成信号。为缓解"模型多轮工具调用后返回纯文本中间态（如'继续'）而忘记调 `task_complete` 导致自动停止"，终端 Agent 在 2026-08-21 引入了 reminder 机制：

- **触发条件**：在已执行过 tool_call 的会话中，模型返回纯文本（非空）且 `finish_reason != "length"`，Agent 会注入一条 reminder 用户消息，让模型补 `task_complete` 或继续调用工具。
- **配额**：默认 1 次（`ai_terminal_agent_mid_session_reminders: 1`）。配额耗尽后仍 text-only，按假性终结处理，UI 会追加"reminder 配额耗尽"提示。
- **可关闭**：在 `AppSettings` 中将 `ai_terminal_agent_mid_session_reminders` 设为 0 即可关闭 reminder，行为完全等价于关闭前的版本（commit 0dddf152）。
- **可观测性**：6 个结构化埋点（`reminder_injected` INFO / `length_path_triggered` WARN / `premature_break_after_reminder_quota` ERROR / `content_filter_triggered` ERROR / `null_finish_reason` WARN / `unknown_finish_reason_break` WARN）。

如果遇到"自动停止但任务未完成"的现象，请检查日志中是否出现 `premature_break_after_reminder_quota`（说明 reminder 配额耗尽）或 `length_path_triggered`（说明触发了 max_tokens 截断）。

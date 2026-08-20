# AI 终端操作员（Terminal Operator Agent）实施方案

**Status**: 🔄 Plan Approved (2026-08-20，外部评审 Round 2 APPROVED，待用户批准开工)
**创建时间**: 2026-08-20
**目标**: 在 omnihub 中落地对齐 tabby-ai-assistant 核心能力的"AI 终端操作员"——基于原生 LLM 工具调用的多轮 Agent，可读取终端输出、向终端写入命令、跨终端操作，并带高危命令确认门控。

---

## 1. 需求与范围

### 1.1 首期范围（用户已确认）

- **核心版**：Agent Loop（多轮原生 tool calling）+ 终端工具集 + 高危命令确认门控
- **作用范围**：本地终端 + SSH 终端（同一 `Terminal` 引擎，API 同构）
- **安全策略**：高危命令写入前弹确认对话框，用户批准后才执行；中低危直接执行

### 1.2 明确不在首期（二期展望）

- MCP 集成（stdio/SSE/HTTP 传输、工具合并）
- 上下文压缩（compaction）与三层记忆
- 错误自动修复（auto-fix）、命令生成/解释快捷键
- 后台异步任务工具（`async_terminal_command` / `check_task_status`）
- 本地终端 OSC 133 命令生命周期解析（首期用静默期启发式）
- Token 用量显示

## 2. 可行性研究结论（已完成三路代码调研）

### 2.1 LLM 工具调用（llm-connector 1.1.14，crates.io 源码已核验）

- `ChatRequest` 已有 `tools: Option<Vec<Tool>>` / `tool_choice`；`Message` 已有 `tool_calls` / `tool_call_id`；`Role::Tool` 存在；构造器 `Message::tool()` / `assistant_with_tool_calls()` 齐备
- 流式 `Delta.tool_calls` 在 OpenAI 系 SSE 路径已做跨块累积（只吐 `is_complete()` 的调用）
- **Provider 支持矩阵（实测代码序列化路径）**：
  - ✅ OpenAI / DeepSeek / Moonshot / Volcengine / Zhipu / Azure / OpenAICompatible（含 Aliyun 兼容模式）：请求、响应、流式全支持
  - ❌ Anthropic 原生协议：请求无 tools 字段，tool_use 响应/流式均丢弃
  - ❌ Ollama：请求无 tools 字段
  - ⚠️ Google：非流式可用，流式丢弃 functionCall
- omnihub 现有封装 `LlmProvider::chat` 把 `ChatResponse` 缩减为 `String`，需扩展；`build_request` 从不设置 tools
- OmniHub 云端 Provider 走 `CloudApiClient`（OpenAI 兼容协议），绕过 `LlmConnector`，M1 中验证其 tools 透传

### 2.2 终端底层（crates/terminal + terminal_view）

已具备：`visible_content()` / `recovery_content(n)` 全量文本、`write_user_input()` 写入、`latest_working_dir()`（OSC 7 + /proc + cwd 文件）、`selection_text()`、`TerminalModelEvent` 订阅（`Wakeup`/`CommandStart` 等）、SSH 的 OSC 133 完整生命周期（`CommandFinished{exit_code}`）、`has_running_processes()`（SSH）。
缺口：`TerminalView.terminal` 字段私有、无跨窗口终端注册表、本地 PTY 无命令完成事件（静默期启发式兜底）。

### 2.3 Agent 框架与 UI 扩展点

- `Agent` trait + `AgentRegistry` 注册零框架改动（db_view 已有成熟先例）；`AgentContext` 的 capability map（`Box<dyn Any + Send + Sync>`）是终端句柄注入点
- `AgentEvent` 可扩展枚举，唯一穷举消费点在 `chatdb/chat_panel.rs:936`
- **结构性缺口**：终端侧栏 `AiChatPanel` 不走 `AgentDispatcher`（直连 `ChatStreamProcessor`），需增加可选 agent 调度模式
- 工具卡片 UI 两条路径：`MessageVariant` 扩展（实时）+ Markdown fenced 块（持久化重建，ChatDB 图表已验证该模式）
- 所有权边界：`one-core` 不能依赖 `terminal*` crate → capability 结构与 Agent 均放 `terminal_view`，镜像 db_view 的 `CAP_DB_METADATA` 模式

## 3. 总体架构

```
terminal_view crate
├── registry.rs          [新增] TerminalViewRegistry：全局终端注册表（id/标题/连接类型）
├── agent_bridge.rs      [新增] TerminalOperatorHandle（capability 值，mpsc 命令通道）
│                              + 主线程泵任务（cx.spawn 消费命令、执行终端操作、oneshot 回执）
├── risk.rs              [新增] 命令风险分级（~25 条规则，High/Critical 需确认）
└── agents/
    ├── mod.rs           [新增] init(cx)：注册 TerminalAgent
    └── terminal_operator.rs [新增] Agent Loop 实现

one-core crate
├── llm/connector.rs     [修改] LlmProvider 增加 chat_full / supports_tools；build_request 支持 tools
├── llm/omnihub_provider.rs [修改] 实现 chat_full（tools 透传验证）
├── agent/types.rs       [修改] AgentEvent 增加 ToolCallStarted / ToolCallFinished
├── ai_chat/types.rs     [修改] MessageVariant 增加 ToolCall
├── ai_chat/panel.rs     [修改] AiChatPanel 可选 agent 调度模式 + capability 注入 API
└── ai_chat/rendering.rs [修改] ToolCall 变体默认渲染（可折叠卡片，仿 reasoning.rs）
```

**数据流**（单轮工具调用）：

```
用户输入 → AiChatPanel(agent模式) → AgentDispatcher → TerminalAgent.execute(tokio::spawn)
  → LlmConnector.chat_stream(request{tools}) ──TextDelta──→ 面板流式渲染
  ← tool_calls(累积完整)
  → 对每个调用：emit ToolCallStarted → 风险评估(write_to_terminal)
      → 高危：ConfirmRisk 命令 → 主线程泵弹确认对话框 → oneshot<bool>
      → 执行：TerminalOpRequest::WriteCommand → 主线程泵 → terminal.write_user_input
      → 等待完成：SSH=CommandFinished/exit_code；本地=静默期+超时
  → emit ToolCallFinished → 追加 Message::tool 结果 → 下一轮
  → 无工具调用或 task_complete → AgentEvent::Completed → 持久化
```

## 4. 详细设计

### M1 — LLM 工具调用管道（crates/core/src/llm/）

1. **`LlmProvider` trait 扩展**（connector.rs）：
   - `async fn chat_full(&self, request: &ChatRequest) -> Result<ChatResponse>` —— 返回完整响应（含 `tool_calls()`）；现有 `chat` 改为默认实现（`chat_full` + 取 content），不破坏既有调用方
   - `fn supports_tools(&self) -> bool` —— 按 `ProviderType` 矩阵返回：OpenAI/DeepSeek/Moonshot/Volcengine/Zhipu/Azure/OpenAICompatible/Aliyun(兼容模式) = true；Anthropic/Ollama/Google = false（附文档说明原因）
2. **`build_request` 扩展**：新增 `build_request_with_tools(&self, messages, tools, tool_choice)`，原 `build_request` 保持不变
3. **消息构造辅助**：新增 `tool_message(content, tool_call_id)` / `assistant_tool_calls_message(tool_calls, content)` 包装 llm-connector 构造器
4. **OmniHub Provider**（omnihub_provider.rs）：`CloudApiClient` 为 OpenAI 兼容协议，实现 `chat_full` 透传 tools；M1 内以集成冒烟验证（不通过则该 Provider 标 `supports_tools=false`）
5. **测试**：`build_request_with_tools` 单测（tools 序列化/anthropic thinking_budget 共存）；mock `LlmProvider` 实现（供 M3 使用）

### M2 — 终端桥接层（crates/terminal_view/）

1. **`TerminalViewRegistry`**（registry.rs，GPUI Global）：
   - 条目：`{ id: u64 自增, weak: WeakEntity<TerminalView>, title, connection_kind }`
   - `TerminalView` 首次渲染时注册、Drop/失活时清理（`retain` 清扫弱引用）
   - `snapshot() -> Vec<TerminalInfo>`（id/标题/连接类型/cwd）；`focus(id)` 经 `window_handle` 激活
2. **`TerminalView` 公开访问器**（view.rs）：`pub fn terminal(&self) -> Entity<Terminal>`（clone 返回）
3. **`agent_bridge.rs`**：
   - `pub const CAP_TERMINAL: &str = "terminal"`
   - `TerminalOperatorHandle`（`Send + Sync + Clone`）：仅持 `mpsc::Sender<TerminalOpRequest>`
   - `TerminalOpRequest` 枚举（每变体带 `oneshot::Sender` 回执）：
     `ReadOutput{terminal_id, max_lines}` / `WriteCommand{terminal_id, data, wait_ms}` / `ListTerminals` / `GetCwd{terminal_id}` / `GetSelection{terminal_id}` / `Focus{terminal_id}` / `ConfirmRisk{command, level}`
   - **主线程泵**：`TerminalSidebar` 初始化时 `cx.spawn` 启动；循环 `rx.recv()` -> `window_handle.update(cx, ...)` 内执行（每个请求先 `WeakEntity::upgrade` 校验，失败返回明确错误）：
     - 读：`terminal.read(cx).visible_content()` / `recovery_content(max_lines)`
     - 写：先滚到底 + bracketed-paste 感知包装（复用 view.rs:2932 逻辑，经 `terminal.mode()` 判断），`write_user_input(data + "\r")`
     - 等待完成：SSH → 轮询 `has_running_processes()`（200ms 间隔）+ 超时；本地 → 静默期检测（`history_size`/内容哈希 800ms 无变化）+ 智能 wait（`wait_ms` 由模型按命令类型给出，默认 1500ms，上限 30s）
     - 等待超时处理：中断等待，把"命令可能仍在运行或已进入交互模式（top/vim/sudo 密码等）"作为工具结果告知模型，由模型决定下一步，不无限挂起
     - 确认：经面板回调弹 gpui-component 确认对话框（显示完整命令 + 风险等级），oneshot 回传用户决定
   - 写入路径**绕过** view.rs 现有高危/多行粘贴确认对话框（agent 侧已有统一门控，避免双重确认）
4. **`risk.rs`**：`RiskLevel { Low, Medium, High, Critical }`；`assess_command(&str) -> RiskLevel`，规则从 tabby 精选压缩约 25 条（`rm -rf /`、`mkfs`、`dd of=/dev/`、fork 炸弹、`curl|sh`、`chmod -R 777 /`、`> /dev/sda`、`sudo -i`、反弹 shell 特征、`shutdown/reboot` 等）；**High/Critical → 需确认**（用户已选策略）
5. **测试**：风险规则表驱动单测；registry 注册/清理/弱引用回收单测；bracketed-paste 包装单测；
   假终端后端（mock `TerminalBackend` + 脚本化输出）驱动的桥接集成测试（写入包装、静默期判定、超时中断、upgrade 失败回错）

### M3 — TerminalAgent（crates/terminal_view/src/agents/terminal_operator.rs）

1. **AgentDescriptor**：`id = "terminal_operator"`，`display_name = "终端操作员"`，`command_prefix = Some("/term")`，`keywords = ["终端", "terminal", "命令", "执行"]`，`required_capabilities = [CAP_TERMINAL]`，`priority = 5`（低于 SQL 系、高于 general_chat 的 100）
2. **系统提示词**（中文，参照 tabby 精简）：角色定位、必须通过工具操作终端（禁止虚构执行结果）、完成时调用 `task_complete`、命令输出截断约定、**交互式/TUI/常驻进程命令（top、tail -f、vim、sudo 密码等）须给出较长 `wait_ms` 或读取输出后及时结束，禁止假设其已完成**
3. **工具集**（JSON Schema 定义）：
   | 工具 | 参数 | 说明 |
   |---|---|---|
   | `task_complete` | `summary: String` | 任务完成信号 |
   | `read_terminal_output` | `terminal_id?, max_lines?` | 读屏幕+回滚（默认 200 行，上限 2000） |
   | `write_to_terminal` | `command, terminal_id?, wait_ms?` | 写命令并回车，等待完成（高危先经确认） |
   | `get_terminal_list` | - | 枚举终端（id/标题/类型/cwd） |
   | `get_terminal_cwd` | `terminal_id?` | 取工作目录 |
   | `get_terminal_selection` | `terminal_id?` | 取选区文本 |
   | `focus_terminal` | `terminal_id` | 聚焦指定终端 |
4. **Agent Loop**（execute 内，`MAX_ROUNDS = 20`）：
   - 前置：`supports_tools()` 为 false → `AgentEvent::Error`（i18n 提示选择支持工具调用的 Provider）
   - 每轮：`chat_stream(request{tools})` → `TextDelta`/`ReasoningDelta` 透传 UI → 收集流式累积的完整 `tool_calls`
   - 无工具调用（且非首轮空响应）或 `task_complete` → `Completed`
   - 有调用：**单轮内多个工具调用严格串行执行**（共享同一终端，避免写入时序冲突）逐个 `ToolCallStarted` → 执行（write 走风险评估→确认→桥接）→ `ToolCallFinished{ok, output 摘要}` → 追加 `assistant_with_tool_calls` + 各 `Message::tool` → 下一轮
   - 工具结果上下文保护：`read_terminal_output` 等结果超出阈值（约 2000 token，按字符近似）时截断并附加截断标记，防止终端日志撑爆上下文
   - 终止保护：`cancel_token` 每轮与每个工具间检查；单轮流式超时（120s）；重复工具调用检测（同 id+参数哈希连续 3 次 → 强制终止）
   - 轻量幻觉防护：最终回答含"已执行"类声明但全程无 write 工具调用 → 追加警示后缀
   - 持久化格式：工具调用记录以 fenced 块嵌入 assistant content（` ```omnihub-tool ` + JSON），重载后由 code_block_renderer 重建卡片（ChatDB 图表已验证该往返模式）
5. **注册**：`terminal_view::agents::init(cx)` → `cx.update_global::<AgentRegistry, _>`；从 `main/src/omnihub_app/mod.rs` 调用（紧随 `db_view::chatdb::agents::init` 之后）
6. **测试**：mock LlmProvider 驱动的循环单测（多轮工具链、task_complete 终止、取消、重复调用熔断、Provider 不支持报错）

### M4 — AiChatPanel agent 模式 + 工具卡片 UI（crates/core/src/ai_chat/）

1. **`AgentEvent` 扩展**（agent/types.rs）：`ToolCallStarted { call_id, name, args_summary }`、`ToolCallFinished { call_id, ok, output }`；补齐 `chatdb/chat_panel.rs:936` 穷举 match（新变体 → 忽略或状态条）
2. **`MessageVariant::ToolCall`**（ai_chat/types.rs）：`{ call_id, name, status: Running/Success/Failed, output }`，作为独立消息条目插入聊天流；`rendering.rs` 默认渲染可折叠卡片（图标 + 工具名 + 状态 + 输出，仿 reasoning.rs 的 keyed_state 折叠模式）
3. **`AiChatPanel` agent 调度模式**（panel.rs，纯增量、opt-in）：
   - `set_agent_dispatch(&mut self, cx)`：启用后 `send_message` 改走 `AgentDispatcher::dispatch`（镜像 ChatDB chat_panel.rs:780-1240 的事件循环），未启用时行为与现状完全一致
   - `set_capability_value(key, value)`：向 AgentContext 注入 capability（终端侧栏注入 `TerminalOperatorHandle`）
   - `set_code_block_renderer(...)`：透传 `TextView::markdown` 的 `code_block_renderer`（持久化工具卡片重建）
   - 事件映射：`Progress` → 状态条；`TextDelta`/`ReasoningDelta` → 流式追加；`ToolCallStarted/Finished` → 插入/更新 ToolCall 消息；`Completed` → 持久化（含 fenced 工具记录）
4. **终端侧栏接线**（terminal_view/src/sidebar/mod.rs）：启用 agent 模式 + 注入 handle + 注册 code_block_renderer；`TERMINAL_AI_SYSTEM_INSTRUCTION` 保留为非 agent 模式兜底
5. **i18n**：新增中英文案（工具卡片状态、确认对话框标题/正文/按钮、不支持工具调用的错误提示）

### M5 — 接线、验证、文档

1. `omnihub_app` 启动序列调用 `terminal_view::agents::init(cx)`
2. 手动验收清单（本地 + SSH × OpenAI 系 Provider）：
   - `/term 查看当前目录并列出文件` → 读取 → 生成命令 → 执行 → 汇报
   - 多终端场景：`get_terminal_list` + `focus_terminal` + 跨终端写入
   - 高危命令（如 `rm -rf /tmp/test` 命中规则）→ 确认对话框 → 拒绝不执行 / 批准执行
   - 流式中途取消；会话切换/重载后工具卡片重建；Anthropic/Ollama Provider 选中时的友好报错
3. `cargo clippy -- --deny warnings` / `cargo fmt --check` / `cargo test --all` 全绿
4. 使用文档（docs/Usage/）+ 本任务文档归档（docs/Task/Archive/2026-08/）

## 5. 实施顺序与依赖

```
M1 (llm 管道) ──→ M3 (TerminalAgent，依赖 M1 的 chat_full/tools + M2 的桥接)
M2 (终端桥接) ──↗
M3 ──→ M4 (面板 agent 模式 + UI，依赖 M3 的新 AgentEvent)
M4 ──→ M5 (接线与验收)
```

M1 与 M2 无相互依赖，可并行。每个里程碑独立可编译、可测试、可提交。

## 6. 风险与缓解

| 风险 | 缓解 |
|---|---|
| Anthropic/Ollama 无工具调用（llm-connector 1.1.14 限制） | `supports_tools` 门控 + 明确报错引导选择 OpenAI 系 Provider；后续可升级 llm-connector 版本解决 |
| 本地终端命令完成检测为启发式（无 OSC 133） | 静默期 + 模型给定的 `wait_ms` + 超时上限；文档说明局限；二期补本地 OSC 133 解析 |
| Agent 写入终端的实害风险 | 高危确认对话框 + 风险规则 + 绕过 UI 双重确认但保留 agent 门控单点 |
| `AiChatPanel` 双模式引入回归 | agent 模式 opt-in，未启用路径零改动；现有测试全量回归 |
| GPUI Entity 仅主线程可触 | 全部终端访问收敛到主线程泵（mpsc + window_handle.update），agent 侧只持 Send 句柄 |
| OmniHub 云端 Provider tools 透传未验证 | M1 冒烟验证，不通过则先标不支持，不阻塞整体 |
| 上下文无 token 预算 | 复用现有 `history_count` 截断（默认 10 条），工具结果输出截断（max_lines 上限） |

## 7. 验收标准

1. 终端侧栏 AI 聊天中，`/term` 前缀或终端相关意图可路由到终端操作员，完成"读→执行→汇报"闭环
2. 本地与 SSH 终端均可被读取/写入；跨终端枚举与聚焦可用
3. 高危命令写入前必现确认对话框；拒绝则不写入
4. 流式取消、会话持久化/重载、Provider 能力门控报错均符合预期
5. 全量 lint / fmt / test 通过；对既有功能零回归（非 agent 模式路径不变）

## 8. 可观测性与回滚（评审建议落实）

- **tracing 日志**：M2 桥接泵与 M3 Agent 循环埋点 -- 每轮 Prompt 摘要、工具入参/出参片段、风险评估拦截（WARN 级）、命令等待与超时均为 DEBUG/WARN 级
- **运行时开关**：`GlobalChatSettings.ai_terminal_agent_enabled`（默认 true）控制终端侧栏是否启用 agent 调度模式；关闭即降级回现有 `TERMINAL_AI_SYSTEM_INSTRUCTION` 纯聊天路径，无需回滚代码
- **Agent 操作状态指示**：工具执行等待期间，终端侧栏显示"Agent 操作中"状态并禁用聊天输入，防止人机并发写入冲突；终端被用户关闭时（`WeakEntity::upgrade` 失败）向 Agent 返回明确错误终止本轮

## 9. 二期展望（不在本方案内）

MCP 集成（三传输 + 工具合并）、上下文压缩与记忆、错误自动修复、命令生成/解释快捷键、后台异步任务工具、本地终端 OSC 133、Token 用量显示。

---

## External Review Opinion

### Round 1/5（2026-08-20，provider=coding-bridge，kind=plan）

- **结论**：未给出显式 verdict；总体评价"质量极高、架构思考深入"，可执行性认可，附 5 项风险与改进建议
- **已落实的整改**：
  1. 本地交互式命令（top/vim/sudo 密码提示）等待失效 -> M3 系统提示词明确要求长 `wait_ms`/及时 `task_complete`；M2 等待超时后中断并告知模型（见 M2.3、M3.2）
  2. 并行 tool calls 时序冲突 -> 明确**串行执行**单轮内的多个工具调用（见 M3.4）
  3. 可观测性缺失 -> 新增 §8 tracing 日志点要求
  4. 终端生命周期冲突 -> `WeakEntity::upgrade` 前置校验 + Agent 操作期间禁用输入/状态指示（见 §8）
  5. 回滚缺失 -> 新增 §8 运行时开关（`ai_terminal_agent_enabled`），关闭即降级
  6. 工具输出撑爆上下文 -> 工具结果超阈值（约 2000 token）截断并附加截断标记（见 M3.4）
  7. 测试强化 -> M2 增加假 PTY/假后端驱动的桥接集成测试（bracketed-paste 感知、静默期判定）（见 M2.5）

### Round 2/5（2026-08-20，provider=coding-bridge，kind=plan，续会话）

- **Verdict: APPROVED**
- 评审逐条确认 Round 1 的 7 项整改全部解决（交互式命令闭环降级、串行执行、tracing、生命周期校验+输入禁用、运行时开关回滚、2000 token 截断、假后端集成测试）
- M1-M5 核心架构维持不变，修订无缝融入原里程碑，无新增架构负担

### 实现评审记录（provider=coding-bridge review_code，kind=code）

- **M1（LLM 工具调用管道）**：评审后完成 R1 修复，通过。
- **M2（终端桥接层 registry/agent_bridge/risk）**：**APPROVED**。
- **M3（TerminalAgent Agent Loop，session c3d75403）**：CHANGES_REQUESTED → 修复 → **APPROVED**。主要修复：tools.rs 运行时 clamp（max_lines 1..=2000、wait_ms ≤30000）、JSON 解析失败报错回灌、`MAX_TOOL_OUTPUT_BYTES` 字节语义、schema 补 `additionalProperties:false`、send_event 通道关闭即终止、熔断计入 tool_records 且签名 JSON 归一化、claims_execution 补英文/繁体关键词、connector 的 assistant_tool_calls_message 保留空 content。
- **M4（AiChatPanel agent 模式 + 工具卡片 UI，session c3d75403 续，分 5 段）**：CHANGES_REQUESTED → 修复 → **APPROVED**（2026-08-21）。
  - 真实修复：`Completed` 终态优先取 `result.content`（原逻辑会丢失 `omnihub-tool` fenced 工具记录，导致重载无法重建卡片）；工具卡片 header 摘要截断 80 字符（`truncate_chars`）；`build_agent_provider_config` 的 `max_tokens=0` 传 `None`；侧栏桥接不可用时补 `tracing::warn` 降级日志。
  - 评审误报驳回：`history_count=0` 时 `split_off` 截断被指为 Bug，经复核 `split_off` 返回值赋给 `messages` 后语义本就正确；已改写为 `saturating_sub` 等价形式并补 2 个单元测试（`agent_history_count_zero_carries_nothing`、`agent_history_dedups_trailing_current_input`）坐实语义。
  - 侧栏接线时序：结构性保证（`omnihub_app::init` → `init_settings_with` → `open_window`，TerminalSidebar 仅在窗口内打开终端标签时构造），无需事件机制。
- **UX 修复轮（用户实测反馈，session c3d75403 续，2026-08-21）**：CHANGES_REQUESTED（评审基于摘要片段误报 `ok: true` 硬编码，verbatim 原文澄清后不成立）→ **APPROVED**。
  - 卡片顺序：ToolCallStarted 事件改为 `insert_tool_call_message` 插入流式占位消息之前，助手回复始终位于执行过程下方。
  - 摘要可读化：`summarize_args` 扩展为全工具可读摘要（命令/目标终端/行数），i18n 新增 4 键；实时卡片（rendering.rs）与重载卡片（tool_card.rs）统一规则——成功显示请求摘要、失败显示错误输出；ToolRecord 新增 `args` 字段。
  - 防重复渲染：Completed 展示内容经 `strip_tool_record_blocks` 剥离 fenced 工具记录块，持久化仍写完整内容（重载由卡片渲染器重建）。
  - 顺带修复本人代码 2 处 clippy 警告（`then_some`、`CapabilityInjector` 类型别名）。

### UX 卡片折叠态 `[序号]:动作摘要` 重构轮（session c3d75403 续，2026-08-21）

- **目标**：折叠态只显示 `[N]:动作摘要`（如 `[1]:获取所有终端`、`[2]:执行命令，ls -la`、`[3]:任务完成，正在准备回复`）；详细参数 / 输出 / 工具名仅展开时可见；序号每次用户请求从 1 递增。
- **实现**：
  - `core::agent::types::ToolCallStarted` 增加 `seq: u32` + `title: String`
  - `core::ai_chat::types::MessageVariant::ToolCall` 增加对应字段；`ChatMessageUI::tool_call` 构造签名扩展
  - `terminal_view::agents::terminal_operator` 新增 `tool_seq: u32` 计数器，`saturating_add(1)` 防回绕；写入 ToolCallStarted 事件 + 两条 tool_records JSON 路径
  - `terminal_view::agents::tools::action_summary` 引入 sanitize_for_header（去控制字符 + 折叠空白），新增 7 个 TerminalAgent.action_* i18n 键（en/zh-CN/zh-HK）
  - `terminal_view::sidebar::tool_card` 折叠卡 header 重写为 `[{seq}]:{title}`，展开详情用 `t!("AiChat.tool_call_detail_*")` 与实时卡片保持一致；ToolRecord 加 `#[serde(default)] seq/title` 字段保证向后兼容
  - 删 `terminal_view.yml` 中重复的 `TerminalAgent.detail_*` 键，避免 i18n 漂移
- **外部评审**：经 4 轮 review_code（Round 3 NEEDS_CHANGES → Round 4 验证修复）。
  - Round 3 修复采纳 6 项（`as_u64_lossy` 浮点 JSON 兼容、u32 saturating_add、原位 trim 去多余分配、去闭包包装、tracing::warn 记录 JSON 解析失败）。
  - Round 3 驳回 6 项（ANSI 注入 — 走 markdown 渲染链路、命令敏感信息脱敏 — UI/终端审计一致性、ToolMeta 统一抽象 — 扩大改动面、ToolRecord 用 ToolCallStatus — 持久化不应混入 Running、clone 顺序 — 收益小、truncate_chars UTF-8 — 已正确）。
  - Round 4 修复采纳 2 项（`action_summary`/`summarize_args` 的 max_lines 摘要也走 clamp、tracing::warn 改结构化 `error = %err` 字段）。
  - Round 4 驳回 10 项（wait_ms 下限 — 改变用户语义、as_u64_lossy 上界 — 调用方已有 clamp、JSON 解析抽公共函数 — 两处回退策略本意不同、rename sanitize_for_header — 私有函数无外部误用风险、测试断言 — 当前模糊匹配合理、6 个 sanitize 边界测试 — 超出本轮范围、trim 简化 — 已验证正确、字符串型数字 — 扩展 schema、u64 计数器 — 工程夸张、`MAX_SUMMARY_LEN` — 评审截断了代码未看见完整实现）。
- **本地验证**：cargo test -p terminal_view --lib agents → 21 passed；cargo test -p one-core --lib ai_chat → 9 passed；cargo fmt / check / clippy（仅 pre-existing 噪音）→ 干净。
- **Round 4 剩余 NEEDS_CHANGES 项**：3 P1（JSON 解析重复、命名误导、err 结构化已修）+ 3 P2（测试断言、测试覆盖、trim 简化）+ 3 P3。剩余项按工程 ROI 评估为低优先级；提交 Round 5 复审。
- **Round 5 评审**：1 P1 + 1 P2 + 1 P3
  - P1 采纳：`summarize_args` 截断逻辑去除 dead code（`map_or` + `is_char_boundary` 循环）。重构为 single-pass `char_indices().nth(MAX_SUMMARY_LEN)` 单次遍历，返回 `Some((end, _))` 时构造截断字符串，`None` 时原样返回。
  - P2 驳回（误报）：评审把 Round 5 prompt 中两个独立函数（`action_summary`/`summarize_args`）的代码片段看成同一 match；实际各自独立。
  - P3 采纳：测试 `sanitize_for_header_trims_in_place` → `sanitize_for_header_trims_whitespace_correctly`，避免"in_place"误导读者认为函数签名是 `&mut String`。
  - **本轮 ≥ 5 评审循环正式终止**（CLAUDE.md §1.5 5 轮上限规定；剩余 P2/P3 按工程 ROI 不再追加）。最终状态：21 tests passed + fmt clean + check clean。

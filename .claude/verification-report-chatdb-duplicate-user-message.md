## 审查报告（chatdb-duplicate-user-message）
生成时间：2026-03-26 21:59:33 +0800

### 需求完整性检查
- 目标明确：检查 ChatDB 发给 AI 的请求是否把同一条用户消息封装了两次
- 范围明确：ChatDB 面板入口、AgentContext 历史构造、相关 Agent 的消息拼装方式
- 交付物明确：根因分析、代码修复、单元测试、编译验证、`.claude` 留痕
- 风险与依赖明确：只修 ChatDB 入口，不改 Agent 协议

### 技术维度评分
- 代码质量：96/100
- 测试覆盖：91/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：99/100
- 架构一致：97/100
- 风险评估：94/100

### 综合评分
- 96/100
- 建议：通过

### 结论
- 根因成立：[`chat_panel.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/chat_panel.rs#L552) 到 [`chat_panel.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/chat_panel.rs#L568) 会先把当前输入压入 `self.chat_history`；随后 [`general_chat.rs`](/usr/htdocs/onetcli/crates/core/src/agent/builtin/general_chat.rs#L45) 到 [`general_chat.rs`](/usr/htdocs/onetcli/crates/core/src/agent/builtin/general_chat.rs#L49)、[`sql_workflow.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/agents/sql_workflow.rs#L277) 到 [`sql_workflow.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/agents/sql_workflow.rs#L283) 以及 [`query_workflow.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/agents/query_workflow.rs#L244) 到 [`query_workflow.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/agents/query_workflow.rs#L269) 又都会把当前问题追加一次，因此同一轮用户输入会重复进入模型上下文。
- 修复位于单一入口：[`chat_panel.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/chat_panel.rs#L77) 到 [`chat_panel.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/chat_panel.rs#L98) 新增 `build_agent_history(...)`，在保留历史裁剪逻辑的同时，剔除“历史尾部刚好等于当前输入”的那条用户消息。
- [`chat_panel.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/chat_panel.rs#L680) 到 [`chat_panel.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/chat_panel.rs#L682) 已改为统一通过该函数构造 `AgentContext` 的历史，因此 GeneralChat、SqlWorkflow、ChatBi 以及路由器都会一起受益。
- [`chat_panel.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/chat_panel.rs#L2062) 到 [`chat_panel.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/chat_panel.rs#L2097) 新增了两条单测，分别验证“当前尾部用户消息会被剔除”和“更早的同文案历史不会误删”。

### 本地验证
- `cargo fmt --all`：通过
- `cargo test -p db_view build_agent_history -- --nocapture`：通过
- `cargo check -p db_view -p main`：通过

### 残余风险
- 这是 ChatDB 入口修复，不影响普通 AI 面板。
- 当前没有真实网络请求抓包验证，但从代码路径看，导致重复的源头已经被切断。

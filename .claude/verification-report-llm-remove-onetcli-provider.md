## 审查报告（llm-remove-onetcli-provider）
生成时间：2026-03-26 21:40:39 +0800

### 需求完整性检查
- 目标明确：删除 `OnetCli AI` 的自动注入，并允许删除本地已有项
- 范围明确：设置页 provider 管理、AI Chat / ChatDB provider 加载、运行时 provider 过滤
- 交付物明确：代码修改、本地验证、风险说明与 `.claude` 留痕
- 风险与依赖明确：保留 `ProviderType::OnetCli` 以兼容旧数据库记录

### 技术维度评分
- 代码质量：95/100
- 测试覆盖：88/100
- 规范遵循：94/100

### 战略维度评分
- 需求匹配：98/100
- 架构一致：96/100
- 风险评估：93/100

### 综合评分
- 95/100
- 建议：通过

### 结论
- [`types.rs`](/usr/htdocs/onetcli/crates/core/src/llm/types.rs#L146) 到 [`types.rs`](/usr/htdocs/onetcli/crates/core/src/llm/types.rs#L181) 新增 `is_runtime_available()` 及对应单测，把“运行时可用 provider”收敛成统一语义：启用且非内置。
- [`engine.rs`](/usr/htdocs/onetcli/crates/core/src/ai_chat/engine.rs#L243) 到 [`engine.rs`](/usr/htdocs/onetcli/crates/core/src/ai_chat/engine.rs#L255)、[`panel.rs`](/usr/htdocs/onetcli/crates/core/src/ai_chat/panel.rs#L406) 到 [`panel.rs`](/usr/htdocs/onetcli/crates/core/src/ai_chat/panel.rs#L447)、[`chat_panel.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/chat_panel.rs#L247) 到 [`chat_panel.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/chat_panel.rs#L280) 已不再自动创建 `OnetCli AI`，并把它从 AI 运行时 provider 列表中剔除。
- [`llm_providers_view.rs`](/usr/htdocs/onetcli/main/src/settings/llm_providers_view.rs#L47) 到 [`llm_providers_view.rs`](/usr/htdocs/onetcli/main/src/settings/llm_providers_view.rs#L61) 现在直接展示仓库中的所有 provider，不再因登录态隐藏旧 `OnetCli` 记录。
- [`llm_providers_view.rs`](/usr/htdocs/onetcli/main/src/settings/llm_providers_view.rs#L150) 到 [`llm_providers_view.rs`](/usr/htdocs/onetcli/main/src/settings/llm_providers_view.rs#L205) 与 [`llm_providers_view.rs`](/usr/htdocs/onetcli/main/src/settings/llm_providers_view.rs#L417) 到 [`llm_providers_view.rs`](/usr/htdocs/onetcli/main/src/settings/llm_providers_view.rs#L476) 已移除内置 provider 的删除/禁用保护，旧 `OnetCli AI` 现在可删除。
- “没有任何 AI 提供商”场景仍是可控退化：[`provider_select.rs`](/usr/htdocs/onetcli/crates/core/src/ai_chat/components/provider_select.rs#L323) 到 [`provider_select.rs`](/usr/htdocs/onetcli/crates/core/src/ai_chat/components/provider_select.rs#L343) 会在空列表时清空选择；[`panel.rs`](/usr/htdocs/onetcli/crates/core/src/ai_chat/panel.rs#L804) 到 [`panel.rs`](/usr/htdocs/onetcli/crates/core/src/ai_chat/panel.rs#L817) 与 [`chat_panel.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/chat_panel.rs#L620) 到 [`chat_panel.rs`](/usr/htdocs/onetcli/crates/db_view/src/chatdb/chat_panel.rs#L637) 会提示先选择 provider，而不是崩溃。

### 本地验证
- `cargo fmt --all`：通过
- `cargo test -p one-core runtime_available -- --nocapture`：通过
- `cargo test -p one-core ensure_onetcli_provider_is_not_default_when_auto_created -- --nocapture`：通过
- `cargo check -p one-core -p db_view -p main`：通过

### 残余风险
- 旧数据库里如果还保留 `OnetCli` provider 关联的历史会话，重新进入会话后会回到“未选择 provider”状态，AI 功能不可用但不会崩。
- `ProviderRepository::ensure_onetcli_provider()` 和 `ProviderType::OnetCli` 仍保留在底层，用于兼容旧数据；这是有意保留，不是遗漏。

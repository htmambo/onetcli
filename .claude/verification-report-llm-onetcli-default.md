## 审查报告（llm-onetcli-default）
生成时间：2026-03-26 21:25:33 CST

### 需求完整性检查
- 目标明确：取消设置页里 `OnetCli AI` 的自动默认行为
- 范围明确：仅涉及 `one-core` 的 provider 仓库默认策略，不改设置页交互结构
- 交付物明确：自动默认逻辑移除、仓库层单测、本地验证、`.claude/` 留痕
- 风险与依赖明确：旧数据中已保存的默认标记不会被自动迁移清除

### 技术维度评分
- 代码质量：96/100
- 测试覆盖：91/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：97/100
- 架构一致：98/100
- 风险评估：93/100

### 综合评分
- 96/100
- 建议：通过

### 结论
- 根因定位准确：`crates/core/src/llm/storage.rs::ensure_onetcli_provider()` 之前会在“当前没有默认 provider”时把自动创建的 `OnetCli AI` 标成默认，这正是设置页里出现默认值的来源。
- 修复位置正确：现在自动创建的 `OnetCli AI` 固定 `is_default = false`，默认策略从源头移除，而不是在 UI 层做遮挡。
- 用户控制仍保留：设置页里既有的“设为默认 / 取消默认”逻辑没有被删，用户仍可手动把 `OnetCli AI` 设成默认。
- 选择链路安全：provider 选择器在无默认 provider 时会回退到首项，因此取消自动默认不会导致聊天或选择面板失效。
- 本地验证有效：`cargo test -p one-core ensure_onetcli_provider_is_not_default_when_auto_created -- --nocapture` 与 `cargo check -p one-core -p main` 均通过。

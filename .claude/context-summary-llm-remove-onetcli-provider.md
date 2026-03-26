## 项目上下文摘要（llm-remove-onetcli-provider）
生成时间：2026-03-26 21:35:23 +0800

### 1. 相似实现分析
- **实现1**: `main/src/settings/llm_providers_view.rs`
  - 模式：设置页直接从 `ProviderRepository` 读取 provider 列表并驱动卡片 UI。
  - 可复用：`load_providers()`、`delete_provider()`、`render_provider_card()`。
  - 需注意：当前对 `OnetCli` 有自动注入、内置保护和登录态过滤三层特殊逻辑。

- **实现2**: `crates/core/src/ai_chat/panel.rs`
  - 模式：聊天面板异步读取 provider 列表后写入 `ProviderSelectState`，由选择器负责清空或重置当前选择。
  - 可复用：`ProviderItem::from_config(...)`、`ProviderSelectState::set_providers(...)`。
  - 需注意：当前会在登录态下调用 `ensure_onetcli_provider()` 主动插入 `OnetCli AI`。

- **实现3**: `crates/db_view/src/chatdb/chat_panel.rs`
  - 模式：ChatDB 面板复用同一套 provider 加载与选择器同步逻辑。
  - 可复用：`ai_input.update_providers(...)` 负责同步 provider 与模型状态。
  - 需注意：这里也有 `ensure_onetcli_provider()` 自动注入路径。

- **实现4**: `crates/core/src/ai_chat/components/provider_select.rs`
  - 模式：`set_providers(...)` 在空列表时会清空 provider 和 model 选择，属于“无 provider”安全退化核心。
  - 可复用：空列表清空逻辑、默认 provider 选择逻辑。
  - 需注意：只要传入空列表，就不会因为缺少 provider 崩溃。

- **实现5**: `crates/core/src/ai_chat/engine.rs`
  - 模式：统一同步加载运行时 provider 列表，适合作为“运行时可用 provider”过滤入口。
  - 可复用：`load_provider_configs_sync(...)`。
  - 需注意：这里当前只按 `enabled` 过滤，没有排除 `OnetCli`。

### 2. 项目约定
- **命名约定**：provider 相关统一使用 `ProviderRepository`、`ProviderConfig`、`ProviderType`。
- **文件组织**：设置页逻辑在 `main/src/settings`，运行时聊天逻辑分别在 `crates/core/src/ai_chat` 与 `crates/db_view/src/chatdb`。
- **代码风格**：先在仓库/类型层沉淀小型判断辅助，再让 UI 和运行时调用同一语义。
- **导入顺序**：保持现有 `use` 分组，不额外重排无关导入。

### 3. 可复用组件清单
- `crates/core/src/llm/types.rs`: `ProviderConfig::is_builtin()` 可继续保留旧数据兼容语义。
- `crates/core/src/ai_chat/components/provider_select.rs`: `set_providers()` 可安全处理空 provider 列表。
- `crates/db_view/src/chatdb/ai_input.rs`: `update_providers(...)` 已封装 provider/model 同步行为。
- `crates/core/src/llm/storage.rs`: `ProviderRepository` 继续负责旧 `onet_cli` 记录的解析与删除。

### 4. 测试策略
- **测试框架**：Rust 单元测试，当前同类仓库测试集中在 `#[cfg(test)]` 模块内。
- **参考文件**：`crates/core/src/llm/storage.rs` 现有 provider 仓库测试。
- **覆盖重点**：
  - 运行时过滤后，`OnetCli` 不再出现在 AI provider 列表
  - 自定义 provider 仍可正常进入运行时列表
  - 没有 provider 时保持空状态而不是崩溃

### 5. 依赖和集成点
- **外部依赖**：无新增依赖。
- **内部依赖**：
  - 设置页依赖 `ProviderRepository`
  - AI Chat / ChatDB 依赖 `ProviderRepository + ProviderSelectState`
  - 运行时 provider 选择依赖 `ProviderConfig`
- **集成方式**：仓库读取 -> 过滤 -> 转成 `ProviderItem` -> 推送给选择器。

### 6. 技术选型理由
- **为什么不直接删除 `ProviderType::OnetCli`**：本地数据库可能已有 `provider_type = onet_cli` 的旧记录，直接删枚举分支会让旧记录解析失败。
- **当前方案**：只切断自动创建和运行时入口，保留底层类型兼容，让旧记录仍可显示并删除。
- **优势**：风险最小，用户可以自行清理历史数据，不会把现有数据库状态搞坏。

### 7. 关键风险点
- **旧会话残留 provider_id**：旧聊天会话可能仍引用已删除或已过滤的 `OnetCli` provider。
- **退化验证重点**：发送消息前必须继续走“未选择 provider”或“provider 不存在”提示，而不是崩溃。
- **兼容性边界**：本轮不删 `OnetCliLLMProvider` 与 `ProviderType::OnetCli`，只关闭入口。

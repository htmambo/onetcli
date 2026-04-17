## 项目上下文摘要（LLM 提供商 OnetCli 默认值）
生成时间：2026-03-26 21:25:33 CST

### 1. 相似实现分析
- **实现1**: `main/src/settings/llm_providers_view.rs`
  - 模式：设置页加载 provider 时，登录后会调用 `ProviderRepository::ensure_onetcli_provider()`。
  - 需注意：这里展示的是仓库里的真实 provider 状态，不是单纯的 UI 假数据。

- **实现2**: `crates/core/src/llm/storage.rs`
  - 模式：`ensure_onetcli_provider()` 负责“存在则启用，不存在则创建”内置 `OnetCli AI`。
  - 需注意：当前自动创建时会把 `is_default` 设为“如果还没有默认 provider，则设为默认”，这是用户看到它成为默认值的根源。

- **实现3**: `crates/core/src/ai_chat/components/provider_select.rs`
  - 模式：provider 下拉在没有默认 provider 时，会回退选择第一个 provider。
  - 需注意：因此取消自动默认不会破坏 provider 选择逻辑。

### 2. 项目约定
- **命名约定**: provider 默认状态统一落在 `ProviderConfig.is_default`。
- **文件组织**: 默认策略在 `one-core` 的仓库层，设置页只是读取和操作仓库数据。
- **代码风格**: 修复应落在数据源头，不在 UI 层做临时遮挡。

### 3. 可复用组件清单
- `crates/core/src/llm/storage.rs::ProviderRepository::ensure_onetcli_provider`
- `crates/core/src/ai_chat/components/provider_select.rs::set_providers / rebuild`
- `main/src/settings/llm_providers_view.rs::toggle_default`

### 4. 测试策略
- `cargo fmt --all`
- `cargo test -p one-core ensure_onetcli_provider_is_not_default_when_auto_created -- --nocapture`
- `cargo check -p one-core -p main`

### 5. 依赖和集成点
- **内部依赖**:
  - `ProviderRepository` 负责持久化 provider
  - `LlmProvidersView` 负责在设置页展示 provider
  - `ProviderSelectState` 负责聊天场景的默认选择回退

### 6. 技术选型理由
- **采用方案**: 去掉 `OnetCli AI` 自动创建时的默认标记，只保留内置 provider 自动创建本身。
- **优势**: 改动面最小，直接消除“自动默认”的来源，同时保留用户手动设为默认/取消默认的能力。
- **风险**: 旧库里已经保存成默认的 `OnetCli AI` 不会被强制回写，当前修复主要面向后续自动创建行为。

### 7. 关键风险点
- **行为边界**: 本次不删除 `OnetCli AI`，只取消其自动默认策略。
- **兼容性**: provider 选择器在无默认时会回退到首项，因此不会因为没有默认标记而崩掉。

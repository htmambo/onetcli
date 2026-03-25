## 项目上下文摘要（certificate-settings-navigation）
生成时间：2026-03-26 00:00:00 +0800

### 1. 相似实现分析
- **实现1**: `main/src/home/home_tabs.rs:563`
  - 模式：`HomePage` 统一负责打开设置标签页，具体页签切换通过 `SettingsPanel::request_page(...)` 先写全局待处理状态，再由 `add_settings_tab(...)` 激活或创建标签。
  - 可复用：本次可以复用 `open_account_settings_tab(...)` 的模式，新增一个“凭证管理”版本。
  - 需注意：真正需要 `window` 的地方是 `HomePage`，因此跨窗口导航最终还是要落回主窗口的 `HomePage`。

- **实现2**: `main/src/setting_tab.rs:79`、`main/src/setting_tab.rs:538`、`main/src/setting_tab.rs:571`
  - 模式：设置页通过 `SettingsPanelPage` + `SelectIndex` 控制默认选中页，并允许在 `SettingItem::render(...)` 中直接嵌入自定义 `Entity`。
  - 可复用：`LlmProvidersView` 的嵌入方式可直接套用到 `CertificateManagerView`。
  - 需注意：页序号是硬编码的，插入新页面后必须同步更新既有页的 `page_ix`。

- **实现3**: `crates/core/src/certificate_manager.rs:875`
  - 模式：当前凭证管理统一通过 `open_certificate_manager_popup(...)` 打开 `PopupWindow`。
  - 可复用：保留现有入口函数名不变，让所有表单继续调用同一个 API。
  - 需注意：从连接表单弹窗内点击时，`active_window()` 指向的通常是弹窗，不适合直接用于打开主窗口设置页。

- **实现4**: `main/src/onetcli_app.rs:323`、`main/src/onetcli_app.rs:343`
  - 模式：主窗口启动时会注册全局 `GlobalTabContainer` 和 `GlobalHomePage`。
  - 可复用：可继续沿用这种全局桥接方式，再补一个“主窗口句柄”全局状态，确保弹窗内也能稳定回到主窗口。
  - 需注意：`PopupWindow` 与主窗口都是独立 `Window`，不能依赖“当前激活窗口一定是主窗口”。

### 2. 项目约定
- **命名约定**: 页面枚举使用语义化名词，如 `General`、`Account`；跨模块入口函数保留已有对外名称，内部再补桥接逻辑。
- **文件组织**: 主页标签行为在 `main/src/home/home_tabs.rs`，设置页结构在 `main/src/setting_tab.rs`，通用凭证逻辑在 `crates/core/src/certificate_manager.rs`。
- **导入顺序**: 先标准库，再第三方，再 `one_core` / 本地模块。
- **代码风格**: 优先在通用层增加能力，不在各连接表单散落条件分支。

### 3. 可复用组件清单
- `main/src/home/home_tabs.rs::add_settings_tab`
- `main/src/home/home_tabs.rs::open_account_settings_tab`
- `main/src/setting_tab.rs::SettingsPanel::request_page`
- `main/src/settings/llm_providers_view.rs::LlmProvidersView`
- `crates/core/src/certificate_manager.rs::CertificateManagerView`

### 4. 测试策略
- **测试框架**: Rust 本地构建验证
- **测试模式**: `cargo fmt --all` + 相关 crate `cargo check`
- **参考命令**:
  - `cargo fmt --all`
  - `cargo check -p one-core -p main -p db_view -p terminal_view -p redis_view -p mongodb_view`
- **覆盖要求**: 至少覆盖设置页编译、主窗口导航桥编译、原有表单入口编译不回归。

### 5. 依赖和集成点
- **外部依赖**: `gpui`、`gpui-component`
- **内部依赖**:
  - `CertificateManagerView` 由 `one_core` 提供
  - 设置页和主窗口标签逻辑在 `main`
  - 各连接表单继续依赖 `open_certificate_manager_popup(...)`
- **集成方式**: `main` 注册一个导航回调到 `one_core`，`one_core` 优先走回调打开主窗口设置页，失败时再回退到旧的 `PopupWindow`
- **配置来源**: 不新增配置项，仅新增全局运行态导航桥

### 6. 技术选型理由
- **为什么用这个方案**: 改动最小，能同时满足“凭证管理迁移到设置页”和“各表单入口不改”的要求。
- **优势**: 表单调用点零改动；主窗口跳转稳定；保留 popup 回退路径，避免主窗口上下文丢失时功能不可用。
- **劣势和风险**: 需要在 `one_core` 和 `main` 之间增加一个全局导航器，页序号也需要同步维护。

### 7. 关键风险点
- **窗口定位风险**: 若误用 `active_window()`，会把导航落到当前弹窗；因此需要明确保存主窗口句柄。
- **分页索引风险**: 新增设置页后 `SettingsPanelPage::Account` 的 `page_ix` 需要同步修正。
- **兼容性风险**: `open_certificate_manager_popup(...)` 改成“优先导航、失败回退”时，不能影响当前 popup 方案的可用性。
- **工具约束**: 仓库规范要求优先使用 `desktop-commander`、`context7`、`github.search_code`、`sequential-thinking`，但本次会话未提供这些工具；已改用本地源码检索和 Rust 构建验证替代并留痕。

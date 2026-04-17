## 项目上下文摘要（sync-server-theme-kiro2api）
生成时间：2026-03-26 00:00:36 +0800

### 1. 相似实现分析
- **实现1**: `main/src/auth.rs::show_password_auth_dialog`
  - 模式：现有 `sync_server` 认证弹窗已经集中在单个对话框函数里，适合直接替换卡片、输入框和按钮视觉层。
  - 可复用：`DialogButtonProps`、`InputState`、现有登录/注册切换逻辑。
  - 需注意：不能破坏已有校验、提交和错误提示流程。

- **实现2**: `main/src/setting_tab.rs::render_account_section`
  - 模式：账户页通过 `SettingItem::render(...)` 插入自定义 GPUI 视图，适合局部换肤，不需要改全局设置页主题。
  - 可复用：现有用户状态读取、登出逻辑和设置页布局容器。
  - 需注意：已登录态和未登录态都要统一视觉语言。

- **实现3**: `main/src/home_tab.rs:2009`
  - 模式：仓库里已有 `ButtonVariant::Custom(ButtonCustomVariant::new(cx)...)` 的自定义按钮写法。
  - 可复用：自定义按钮 variant 的接入方式与交互态定义方式。
  - 需注意：配色改造应优先抽成可复用 theme helper，避免颜色散落在多个文件。

- **实现4**: `../kiro2api/frontend-vue/src/styles/globals.css`、`../kiro2api/frontend-vue/src/views/login/LoginPage.vue`
  - 模式：`kiro2api` 使用深色背景、深色面板、弱白边框、绿色强调按钮的统一视觉语言。
  - 可复用：`#0a0a0a` / `#12121a` / `#101114` / `rgba(255,255,255,0.05)` / `#00d9a3` 这组核心色阶。
  - 需注意：当前桌面端是 GPUI，不追求布局 1:1，只迁移配色与层级关系。

### 2. 项目约定
- **命名约定**: 主题辅助函数使用语义名，如 `panel_bg`、`text_muted`、`primary_button_variant`。
- **文件组织**: `sync_server` 相关 UI 仍放在 `auth.rs` 和 `setting_tab.rs`，配色抽到独立的 `main/src/sync_server_theme.rs`。
- **代码风格**: 优先局部换肤，不改全局主题 token；复用已有 GPUI 链式样式和按钮 variant 机制。

### 3. 可复用组件清单
- `gpui_component::dialog::DialogButtonProps`
- `gpui_component::button::ButtonCustomVariant`
- `main/src/auth.rs::show_password_auth_dialog`
- `main/src/setting_tab.rs::render_account_section`
- `main/src/home_tab.rs` 中的自定义按钮 variant 写法

### 4. 测试策略
- **测试框架**: Rust 本地格式化与编译检查
- **验证方式**: `cargo fmt --all`、`cargo check -p main`
- **覆盖重点**: 新增主题模块类型正确；弹窗和账户页链式样式 API 可编译；不破坏既有登录/登出逻辑

### 5. 依赖和集成点
- **外部依赖**: `gpui` 颜色类型 `Hsla` / `rgb` / `rgba`
- **内部依赖**: `DialogButtonProps`、`ButtonVariant::Custom`、`GlobalCurrentUser`、认证提交闭包
- **集成方式**: 通过 `mod sync_server_theme;` 引入统一主题，然后在 `auth.rs` 和 `setting_tab.rs` 局部消费

### 6. 技术选型理由
- **为什么用这个方案**: 用户要求把 `sync_server` 配色改成 `../kiro2api` 的方案，最稳妥的落点就是只替换 `sync_server` 相关局部 UI。
- **优势**: 风险小、范围清晰、后续继续调色时只需要改主题辅助模块。
- **风险**: 当前验证以编译通过为主，最终视觉细节仍需要桌面端手动确认。

### 7. 工具说明
- 仓库规范中提到优先使用 `sequential-thinking`、`desktop-commander`、`context7`、`github.search_code`。
- 本次会话未提供这些工具，已基于本地源码检索、相邻仓库对照和 Rust 构建命令完成上下文收集与验证。

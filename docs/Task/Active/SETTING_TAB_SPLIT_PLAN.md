# P1 拆分 main/src/setting_tab.rs 任务计划

**状态**: 🔄 进行中 (轮 8c 已完成于 2026-06-15，剩轮 9 SettingsPanel Render)

## 进度追踪

| 轮次 | 子模块 | 状态 |
|---|---|---|
| 轮 1 | `hotkey.rs` | ✅ 已完成 (2026-06-15) |
| 轮 2 | `global_user.rs` | ✅ 已完成 (2026-06-15) |
| 轮 3 | `cloud.rs` | ✅ 已完成 (2026-06-15) |
| 轮 4 | `saved_window.rs` | ✅ 已完成 (2026-06-15) |
| 轮 5 | `proxy.rs` | ✅ 已完成 (2026-06-15) |
| 轮 6 | `types.rs` | ✅ 已完成 (2026-06-15) |
| 轮 7 | `theme_utils.rs` | ✅ 已完成 (2026-06-15) |
| 轮 8a | `app_settings.rs` (struct + Default + Global) | ✅ 已完成 (2026-06-15) |
| 轮 8b | `app_settings.rs` (impl 方法) | ✅ 已完成 (2026-06-15) |
| 轮 8c | `migrations.rs` (迁移/同步辅助) | ✅ 已完成 (2026-06-15) |
| 轮 9 | `SettingsPanel` Render | 🔄 进行中（细分为 9a-9e） |
| 轮 9a | `about.rs` | ✅ 已完成 (2026-06-15) |
| 轮 9b | `shortcuts.rs` | ✅ 已完成 (2026-06-15) |
| 轮 9c | `auth_form.rs` | ⏳ 待执行 |
| 轮 9d | `proxy_view.rs` | ⏳ 待执行 |
| 轮 9e | `panel.rs` (SettingsPanel) | ⏳ 待执行 |

## 中停说明（已恢复）

按 `AGENTS.md` 流程升级规则，本任务影响面（31 处 `#[serde(default = "...")]` 路径修改
+ 8 个新文件 + 14 个 import 验证 + 跨模块 cx.listener 闭包）超出 initial 判断。

**不应在 fullauto 零询问模式下硬干**。改为：

1. 任务分多轮**人工协作**推进
2. 每轮只拆 1-2 个**真正独立**的子模块
3. 每轮后 `cargo check` 验证 0 error 再继续

## 建议执行顺序（按依赖最少 → 最多）

- 轮 1：拆 `hotkey.rs`（2 个常量 + 0 个引用关系）
- 轮 2：拆 `global_user.rs`（独立 Arc<RwLock> 类型）
- 轮 3：拆 `cloud.rs`（4 个独立 settings struct）
- 轮 4：拆 `saved_window.rs`（独立 + 居中辅助）
- 轮 5：拆 `proxy.rs`（独立 ProxyType + GlobalProxySettings）
- 轮 6：拆 `types.rs`（5 个纯枚举，**注意 AppSettings 用 #[serde(default)] 时引用**）
- 轮 7：拆 `theme_utils.rs`（**31 处 serde default 字符串全改路径**）
- 轮 8：拆 `app_settings.rs`（最大块，依赖前面所有）
- 轮 9：拆 SettingsPanel Render（1844 行，**最后做**）

每轮按 1 个子任务 = 1 个 fullauto 任务执行。



## 任务目标

将 4455 行的 `main/src/setting_tab.rs` 拆分为按"主题/页"组织的多个子模块，满足
`AGENTS.md:387` 硬门禁（文件 ≤ 300 行），**保持公开 API 与现有 14 处
`use crate::setting_tab::...` 跨文件引用 100% 等价**。

## 背景

- 当前文件 4455 行，包含 45 个 `pub` 声明（无内联子模块）
- 14 处跨文件 import 必须保持兼容
- `main/src/settings/` 已有 4 个 dialog，需要统一组织

## 拆分策略（采纳 Codex Phase 1 建议：**根 facade 不搬家**）

| 文件 | 主题 | 估算行数 |
|---|---|---|
| `main/src/setting_tab.rs`（**保留并瘦身**） | `init_settings` + `SettingsPanel` + Render + 测试 + 子 mod 声明 | ~2200（仍是最大，但已大幅下降） |
| `main/src/setting_tab/mod.rs`（**新增**） | 子 mod 声明 + 公开 API re-export | ~50 |
| `main/src/setting_tab/types.rs` | `DatabaseOpenMode` / `LargeTextCellEditorOpenMode` / `ConnectionListSort*` / `SettingsPanelPage` 等纯枚举 | ~250 |
| `main/src/setting_tab/app_settings.rs` | `AppSettings` struct + 默认值函数 + `impl AppSettings` | ~1000 |
| `main/src/setting_tab/cloud.rs` | `WebDavSettings` / `GistSettings` / `GoogleDriveSettings` / `OneDriveSettings` | ~120 |
| `main/src/setting_tab/hotkey.rs` | `DEFAULT_SYSTEM_HOTKEY_*` 常量 | ~10 |
| `main/src/setting_tab/saved_window.rs` | `SavedWindowBounds` + 居中辅助函数 | ~150 |
| `main/src/setting_tab/proxy.rs` | `ProxyType` / `GlobalProxySettings` | ~150 |
| `main/src/setting_tab/theme_utils.rs` | 默认值 / clamp / 主题辅助函数（被 `#[serde(default = "...")]` 引用，必须 pub(super)） | ~200 |
| `main/src/setting_tab/global_user.rs` | `GlobalCurrentUser` + `PendingSettingsPanelPage` | ~70 |

## 子任务列表

- [ ] S1. 在 `main/src/setting_tab/` 下创建子文件目录结构（仅 .rs 文件，mod.rs）
- [ ] S2. 把 `types.rs` 涉及的 5 个枚举从 `setting_tab.rs` 整体剪切过去
- [ ] S3. 把 `app_settings.rs` 涉及的 struct + 850 行 + impl 剪过去
- [ ] S4. 把 `cloud.rs` 涉及的 4 个 settings struct + Default 剪过去
- [ ] S5. 把 `hotkey.rs` 常量剪过去
- [ ] S6. 把 `saved_window.rs` 剪过去
- [ ] S7. 把 `proxy.rs` 剪过去
- [ ] S8. 把 `theme_utils.rs` 默认值函数剪过去（**注意 `#[serde(default = "fn_name")]` 路径**）
- [ ] S9. 把 `global_user.rs` 剪过去
- [ ] S10. `setting_tab.rs` 改为：use 子模块 + mod 声明 + re-export 全部 pub 符号 + 保留 `init_settings` + `SettingsPanel` UI
- [ ] S11. 调整 `#[serde(default = "...")]` 路径为 `crate::setting_tab::theme_utils::fn_name`（或 pub(super)）
- [ ] S12. 调整 `mod tests` 中的 `super::xxx` 引用
- [ ] S13. `cargo check -p main` 0 error
- [ ] S14. `cargo check -p main --tests` 0 error
- [ ] S15. `git grep "use crate::setting_tab::"` 14 处全部不变
- [ ] S16. 验收：单文件最大 ≤ 1100（panel UI 暂留 facade，单独后续任务）

## 验收标准

- [ ] `cargo check -p main` 0 error
- [ ] `cargo check -p main --tests` 0 error
- [ ] `git grep "use crate::setting_tab::"` 14 处全部不变
- [ ] 单文件最大 ≤ 1100（setting_tab.rs 留 2200 是临时状态；后续任务继续拆 SettingsPanel）
- [ ] 不修改任何业务逻辑
- [ ] Codex 顾问评审非 REJECTED

## 风险评估和缓解措施（采纳 Codex Phase 1 风险）

| 风险 | 缓解 |
|---|---|
| `#[serde(default = "fn_name")]` 路径依赖同模块私有函数，跨模块后失效 | 改用 `#[serde(default = "crate::setting_tab::theme_utils::fn_name")]` 或 `pub(super) fn` + 相对路径 |
| `mod tests` 用 `super::xxx` 引用私有 helper | 把测试迁到目标模块所在文件，或 `use super::theme_utils::*` |
| `SettingsPanel::render` 内部 cx.listener 闭包跨模块引用 | `SettingsPanel` UI 暂留原 `setting_tab.rs`，不跨模块（已采纳） |
| 14 个 import 路径全部要保持不变 | 根 `setting_tab.rs` 公开 `pub use 子模块::符号` re-export，**对外路径零变化** |
| 5 个 fn / 4 个 struct 与 `AppSettings` `#[derive(...)]` 直接关联（如 `#[serde(default = "default_font_family")]`） | 拆 `theme_utils.rs` 时必须同时改 `AppSettings` 的 `#[serde(default = "...")]` 字符串 |
| 单文件行数 1100 仍 > 300 硬门禁 | 在 PLAN 中显式标"部分达标"；后续独立任务继续拆 SettingsPanel UI |



## 风险评估和缓解措施

| 风险 | 缓解 |
|---|---|
| 4455 行一次性拆分，编译错误雪崩 | 严格按 S3-S7 顺序逐个模块迁移；每步后 `cargo check` 确认 0 error 再下一步 |
| 公开 API 漂移 | facade re-export 所有原符号（pub use ...） |
| 14 处 import 失效 | 保持 `crate::setting_tab::...` 路径不变（facade 在 main/src 下仍可被 `crate::setting_tab` 引用） |
| `SettingsPanel` 内部状态/泛型约束 | 暂留 facade，单独后续任务处理 |
| 巨文件拆分属于行为不变的"重构" → 触及多文件共享核心逻辑 | AGENTS.md:397 提示"必要时先补测试再重构"；本任务关键是"先建测试再拆"（TDD 红→绿） |

## 实施顺序

```
S1 (调研) → S2 (设计) → S3 (types) → S4 (app_settings) → S5 (cloud) → S6 (hotkey) → S7 (saved_window)
                                              ↓
                                  S8 (facade) → S9 (mod.rs) → S10 (main.rs) → S11 (cargo check) → S12 (验收)
```

## 阶段输出

### 轮 1：`hotkey.rs`（2026-06-15 完成）

**分支**：`refactor/setting-tab-split-r1-hotkey`

**改动**：
- 新增 `main/src/setting_tab/hotkey.rs`（17 行）
- 从 `setting_tab.rs` 移出：
  - `DEFAULT_SYSTEM_HOTKEY_MACOS` / `_OTHER` 两个常量
  - `default_system_hotkey_macos()` / `_other()` 两个 serde default 函数
- `setting_tab.rs` 内：
  - 新增 `mod hotkey;` + `pub(crate) use hotkey::{DEFAULT_SYSTEM_HOTKEY_MACOS, DEFAULT_SYSTEM_HOTKEY_OTHER};`
  - 4 处 `#[serde(default = "...")]` 与 `impl Default for AppSettings` 调用改为 `hotkey::` 前缀
- `setting_tab.rs` 行数：4558 → 4551（−7 行）

**可见性策略**：
- 常量 `pub(crate)`：经父模块 `pub(crate) use` re-export，外部 `crate::setting_tab::DEFAULT_SYSTEM_HOTKEY_*` 路径不变
- 默认函数 `pub(super)`：仅供父模块 `setting_tab` 内的 serde derive 与 `Default` impl 使用

**验证**：
- ✅ `cargo check -p main` 0 error
- ✅ `cargo check -p main --tests` 0 error
- ✅ 8/8 hotkey 相关测试通过（`hotkey_migration_tests` + `app_init::tests`）
- ✅ `git grep "use crate::setting_tab"` 外部引用 14 处全部不变
- ✅ Codex 审核：APPROVED（无 finding）

**经验**：
- Rust 2024 中 `setting_tab.rs`（父模块文件）与 `setting_tab/<child>.rs` 子模块文件可共存
- `#[serde(default = "hotkey::default_xxx")]` 路径字符串在 derive 展开点解析，相对父模块路径 OK
- `pub(crate) use private_mod::Symbol` 可让外部以 `crate::parent::Symbol` 访问私有子模块项

### 轮 2：`global_user.rs`（2026-06-15 完成）

**分支**：`refactor/setting-tab-split-r2-global-user`

**改动**：
- 新增 `main/src/setting_tab/global_user.rs`（61 行）
- 从 `setting_tab.rs` 移出：
  - `pub struct GlobalCurrentUser` + `impl Default` / `impl gpui::Global` / `impl GlobalCurrentUser { get_user, set_user }`
  - `struct PendingSettingsPanelPage` + `impl gpui::Global` / `impl PendingSettingsPanelPage { take }`
- `setting_tab.rs` 内：
  - 新增 `mod global_user;`
  - `pub(crate) use global_user::GlobalCurrentUser;`（保持外部 `home_tab.rs` 等的 `crate::setting_tab::GlobalCurrentUser` 路径不变）
  - `use global_user::PendingSettingsPanelPage;`（私有，仅本文件 9 处引用）
  - 移除 `std::sync::RwLock` 和 `one_core::cloud_sync::GlobalCloudUser` 的 `use`（父文件已无引用）
- `setting_tab.rs` 行数：4551 → 4506（−45 行）

**可见性策略**：
- `GlobalCurrentUser`：子模块 `pub struct` + 父模块 `pub(crate) use` re-export → 外部 `home_tab.rs:61` 5 处引用零改动
- `PendingSettingsPanelPage`：子模块 `pub(super) struct` + 父模块普通 `use` → 仅 setting_tab.rs 可见，等价于原私有 struct
- `super::SettingsPanelPage` 反向引用：临时模式，等轮 6 把 `SettingsPanelPage` 移到 `types.rs` 后改路径

**验证**：
- ✅ `cargo check -p main` 0 error
- ✅ `cargo check -p main --tests` 0 error
- ✅ `cargo test -p main --no-run` 测试二进制构建成功（35.6s）
- ✅ `cargo test -p main` 84 passed / 1 failed
  - 失败项 `setting_tab::tests::global_proxy_settings_validate_required_fields` **与本轮无关**（baseline `git stash` 验证同样失败，属 i18n 文案断言缺陷）
- ✅ `git grep "use crate::setting_tab"` 外部 14 处引用不变
- ✅ Codex 审核：APPROVED（无 finding）

**经验**：
- 子模块需要父模块类型时，`use super::ParentType;` 是常规模式，Rust 模块内 item 顺序不影响（父文件后置声明仍可被前置子模块 `use`）
- 抽取后必须用 `grep` 复查父文件是否还引用了原 `use` 的符号 — 本轮就因 `UserInfo` 仍在 `render_logged_in_user_sync` 用而**保留** import，仅删 `GlobalCloudUser` 和 `RwLock`
- 子模块标记 `pub(super) fn/struct` 比 `pub(crate)` 更收敛，最大化封装

### 轮 3：`cloud.rs`（2026-06-15 完成）

**分支**：`refactor/setting-tab-split-r3-cloud`

**改动**：
- 新增 `main/src/setting_tab/cloud.rs`（59 行）
- 从 `setting_tab.rs` 移出 4 个云同步配置 struct（共 50 行）：
  - `WebDavSettings` + 自定义 `impl Default`（vault_path 缺省为 `"ONetCli-vault"`）
  - `GistSettings`（derive Default）
  - `GoogleDriveSettings`（derive Default）
  - `OneDriveSettings`（derive Default）
- `setting_tab.rs` 内：
  - 新增 `mod cloud;`
  - `pub(crate) use cloud::{GistSettings, GoogleDriveSettings, OneDriveSettings, WebDavSettings};`（保持外部 `oauth_dialog.rs` 等 `crate::setting_tab::*Settings` 路径不变）
  - 移除 `one_core::cloud_sync::oauth::OAuthTokens` 的 `use`（父文件已无引用）
- `setting_tab.rs` 行数：4506 → 4455（−51 行）

**可见性策略**：
- 4 个 struct 全部 `pub`，外部以 `crate::setting_tab::XxxSettings` 访问（依靠父模块 `pub(crate) use` re-export）
- 父模块同时保有 `AppSettings.{webdav,gist,google_drive,onedrive}_config` 字段类型、13 处渲染代码内的 `XxxSettings::default()` 调用

**验证**：
- ✅ `cargo check -p main` 0 error
- ✅ `cargo check -p main --tests` 0 error
- ✅ `cargo test -p main` 84 通过 / 1 失败（baseline 已知 i18n 缺陷，与本轮无关）
- ✅ `git grep "use crate::setting_tab"` 外部 14 处不变
- ✅ Codex 审核：APPROVED（无 finding）

**经验**：
- 抽取整组同源 struct（云同步族）比单个迁更高效 — 共享 `OAuthTokens` 依赖、derive 不变、字段/可见性不变，行为零变化
- 移除父文件未使用的 `use` 项（如 `OAuthTokens`）前，必须 `grep -nw` 全文确认
- 子模块自行 `use one_core::...::OAuthTokens`，与父模块解耦更清晰

### 轮 4：`saved_window.rs`（2026-06-15 完成）

**分支**：`refactor/setting-tab-split-r4-saved-window`

**改动**：
- 新增 `main/src/setting_tab/saved_window.rs`（141 行）
- 从 `setting_tab.rs` 移出（共 ~130 行）：
  - `pub enum SavedWindowDisplayState`（Windowed/Maximized/Fullscreen + serde rename_all）
  - `pub struct SavedWindowBounds` + 7 个方法的 `impl` 块
  - 2 个自由函数：`centered_bounds_in_visible_area`（私有）、`centered_window_bounds_within_visible_area`（升 `pub(super)`）
- `setting_tab.rs` 内：
  - 新增 `mod saved_window;`
  - `pub(crate) use saved_window::SavedWindowBounds;`（保持外部 `onetcli_app/mod.rs` 3 处引用不变）
  - `#[allow(unused_imports)] pub(crate) use saved_window::SavedWindowDisplayState;`（lib check 视角下未直接使用，但测试模块需要 — 加 `#[allow]` 抑制 warning 又保留 re-export 路径）
  - `use saved_window::centered_window_bounds_within_visible_area;`（父模块 `AppSettings::restored_main_window_bounds` 调用）
  - 移除父文件已无引用的 `gpui::{Bounds, point, size}` import 项
- `setting_tab.rs` 行数：4455 → 4328（−127 行）

**可见性策略（关键）**：
- `from_window_bounds`、`to_restored_window_bounds` → `pub(super)`（父模块 `AppSettings::snapshot/restored_main_window_bounds` 调用）
- `to_window_bounds`、`fit_in_visible_bounds` → `pub(super)`（父模块测试 `mod tests` 调用）
- 内部私有：`from_bounds`、`is_valid`、`build_window_bounds`、`centered_bounds_in_visible_area`
- struct 字段全部 `pub`（测试以结构体字面量构造）

**验证**：
- ✅ `cargo check -p main` 0 error，0 setting_tab 相关 warning
- ✅ `cargo check -p main --tests` 0 error
- ✅ `cargo test -p main` 84 通过 / 1 失败（baseline i18n 缺陷）
- ✅ `git grep "use crate::setting_tab"` 外部 14 处不变
- ✅ `rustfmt` saved_window.rs 通过
- ✅ Codex 审核：APPROVED（仅一项 fmt 微调，已修复）

**经验**：
- **可见性边界必须穷举**：迁移前必须 grep 所有方法调用点（父模块 + 父模块测试 + 兄弟子模块），分类提升为 `pub(super)`，否则 lib check 通过但 `--tests` 失败
- **测试模块独有的 re-export 警告**：用 `#[allow(unused_imports)]` 只标该行，比改全局允许更精准
- **`use` 项清理**：抽走代码后，父文件顶层 `gpui::{...}` 必须 grep 验证哪些类型已无引用
- **rustfmt 单文件应用**：`rustfmt --edition 2024 <file>` 比 `cargo fmt -p main` 范围更可控，避免触碰不相关文件
- Codex 配合 grep 验证才是可靠的可见性映射方式 — Codex 提示了我自己没意识到的"测试也调用了私有方法"风险

### 轮 5：`proxy.rs`（2026-06-15 完成）

**分支**：`refactor/setting-tab-split-r5-proxy`

**改动**：
- 新增 `main/src/setting_tab/proxy.rs`（111 行）
- 从 `setting_tab.rs` 移出（共 ~100 行）：
  - `pub enum ProxyType`（Http/Https/Socks5）+ `impl ProxyType::as_str()`
  - `pub struct GlobalProxySettings`（6 字段，含 `#[serde(default)]` 与 `#[serde(default = "default_proxy_port")]`）
  - 自定义 `impl Default for GlobalProxySettings`
  - `impl GlobalProxySettings { validate, to_proxy_url }`
  - 私有 `fn default_proxy_port() -> u16`
- `setting_tab.rs` 内：
  - 新增 `mod proxy;`
  - `pub(crate) use proxy::{GlobalProxySettings, ProxyType};`
  - 移除父文件已无引用的 `gpui::http_client::Url` import 项
- `setting_tab.rs` 行数：4328 → 4232（−96 行）

**可见性策略**：
- 两个类型 `pub`、字段 `pub`，方法已是 `pub`（无需提升）
- `default_proxy_port` 保持私有 — 仅在 `proxy.rs` 内被 `#[serde(default = "...")]` 和 `Default` impl 引用
- UI 视图 `GlobalProxySettingsView`（Render 实现）暂留 `setting_tab.rs`，因依赖大量父模块 UI import

**验证**：
- ✅ `cargo check -p main` 0 error，9 warnings（与 baseline 完全一致）
- ✅ `cargo check -p main --tests` 0 error
- ✅ `cargo test -p main` 84 通过 / 1 失败（baseline i18n 缺陷）
- ✅ `rustfmt proxy.rs` 通过
- ✅ Codex 审核：APPROVED（无 finding）

**经验**：
- **`#[serde(default = "fn_name")]` 路径鲁棒性**：当 struct + 默认值函数**一起搬走**时，相对路径自动保留 — 无需改 attribute 字符串
- **同质化迁移**：序列化模型 + 默认值 + 校验 + 序列化辅助是"一族"，应一起迁；将 UI Render 留在父文件内分离关注点
- 跨 crate 同名类型混淆排除：`crates/ssh::ProxyType`、`crates/core::storage::ProxyType` 与本 crate 完全独立，grep `crate::setting_tab::ProxyType` 是过滤外部消费者的最准方法

### 轮 6：`types.rs`（2026-06-15 完成）

**分支**：`refactor/setting-tab-split-r6-types`

**改动**：
- 新增 `main/src/setting_tab/types.rs`（109 行）
- 从 `setting_tab.rs` 移出（共 ~100 行）：
  - `pub enum SettingsPanelPage` + `select_index` 方法
  - `pub enum DatabaseOpenMode` + `as_str` / `from_str`
  - `pub enum LargeTextCellEditorOpenMode` + `as_str` / `from_str` + `From<LargeTextCellEditorOpenMode> for LargeTextEditorOpenMode`
  - `pub enum ConnectionListSortField` / `ConnectionListSortOrder` / `ConnectionListViewMode`
- `setting_tab.rs` 内：
  - 新增 `mod types;`
  - `pub(crate) use types::{ConnectionListSortField, ConnectionListSortOrder, ConnectionListViewMode, DatabaseOpenMode, LargeTextCellEditorOpenMode, SettingsPanelPage};`
  - 移除父文件已无引用的 `LargeTextEditorOpenMode`（db_view 中）与 `SelectIndex`（gpui_component::setting 中）import 项
- `setting_tab.rs` 行数：4232 → 4135（−97 行）

**可见性策略**：
- 全部 enum `pub`，外部以 `crate::setting_tab::*` 访问（父 re-export）
- `SettingsPanelPage::select_index` 由原 `fn`（私有）显式标记为 `pub(super) fn`，因父模块 `SettingsPanel::render` 仍调用它

**Sibling 模块兼容**：
- `global_user.rs` 中的 `use super::SettingsPanelPage;` 依靠父模块的 `pub(crate) use types::SettingsPanelPage` re-export 保持解析正确，无需改动
- 等价的 `use super::types::SettingsPanelPage;` 是另一选择，但保留 `super::` 形式让 sibling 与父模块表面解耦更可控

**验证**：
- ✅ `cargo check -p main` 0 error，9 warnings（与 baseline 一致），0 setting_tab 相关 warning
- ✅ `cargo check -p main --tests` 0 error
- ✅ `cargo test -p main` 84 通过 / 1 失败（baseline i18n 缺陷）
- ✅ `rustfmt types.rs` 通过
- ✅ Codex 审核：APPROVED（无 finding）

**经验**：
- **子模块 import 必须精确到子路径**：`gpui_component::SelectIndex` 不存在 — 实际在 `gpui_component::setting::SelectIndex`。直接复制父文件 import 而不验证子路径是常见陷阱，靠 `cargo check` 立即捕获
- **`super::Symbol` 路径稳定性**：当子模块通过 `super::X` 引用父类型，而父类型再次被搬到 sibling 模块后，只要父模块以 `pub(crate) use` re-export，`super::X` 仍可解析 — 大幅降低跨轮联动改动量
- **批量同质迁移**：6 个枚举 + 4 个 impl 一次迁完比逐个更省事，因 enums 之间无依赖、derive 模式相同

### 轮 7：`theme_utils.rs`（2026-06-15 完成，高风险轮）

**分支**：`refactor/setting-tab-split-r7-theme-utils`

**改动**：
- 新增 `main/src/setting_tab/theme_utils.rs`（98 行）
- 从 `setting_tab.rs` 移出 22 个纯函数：
  - 17 个 `default_*` 默认值函数（含使用频次最高的 `default_true`）
  - 3 个 `clamp_*` 数值归一化函数
- 父模块**保留**`themed_setting_field/group/page`、`settings_group_*_style` 等 UI 主题化辅助函数 — 它们依赖 `cx.theme()` 与 47 处 render 调用强耦合，留待轮 9
- `setting_tab.rs` 内大规模重写：
  - 28 处 `#[serde(default = "default_X")]` → `#[serde(default = "theme_utils::default_X")]`（hotkey 已加前缀的 2 处保持不变）
  - 32 处 `impl Default for AppSettings` 字面量 + `migrate_legacy_theme_state` + clamp 调用 → `theme_utils::` 前缀
  - 测试 `mod tests` 中 `use super::{..., clamp_ui_surface_opacity, ...};` 拆为 `use super::theme_utils::clamp_ui_surface_opacity;`
  - 移除父文件已无引用的 `terminal_view::{DEFAULT_LINE_HEIGHT_SCALE, DEFAULT_RECOVERY_SCROLLBACK_LINES}` import
  - 保留父文件仍用的 `gpui_component::{MIN_GLASS_OPACITY, MAX_GLASS_OPACITY}` 和 `terminal_view::{MIN/MAX_LINE_HEIGHT_SCALE, MAX_RECOVERY_SCROLLBACK_LINES}`（render UI slider + setter 内 clamp）
- `setting_tab.rs` 行数：4135 → 4051（−84 行）

**实施手法（关键创新）**：
> **Python 批量重写脚本** — 60 处替换若靠 Edit 单条做，极容易遗漏。改用 Python `str.replace` 替换 serde 属性、`re.subn` + 负向先行断言 `(?<![:.\w])(?<!fn )name(` 替换裸调用，一次完成 28 + 32 处共 60 个替换点。后置 grep 验证：`grep '#\[serde\(default = "default_'` 返回 0 即全部清零。

**可见性策略**：
- 全部函数 `pub(super)`，仅父模块 serde derive 与 Default impl 调用
- 父模块测试 `mod tests` 通过 `super::theme_utils::clamp_ui_surface_opacity` 显式跨模块访问

**验证**：
- ✅ `cargo check -p main` 0 error，9 warnings（与 baseline 一致），0 setting_tab 相关 warning
- ✅ `cargo check -p main --tests` 0 error
- ✅ `cargo test -p main` 84 通过 / 1 失败（baseline i18n 缺陷）
- ✅ `rustfmt theme_utils.rs` 通过
- ✅ `cargo fmt -p main` 本轮引入的 2 处 fmt 差异已修复（剩余 3 处为 baseline）
- ✅ Codex 审核：APPROVED（修复 fmt 后无 finding）
- ✅ `grep '#\[serde\(default = "default_' setting_tab.rs` 返回 0（所有 serde 路径已更新）

**经验**：
- **大规模 grep + 批量改写需要脚本辅助**：60 处替换若用 Edit 单条做，单条遗漏即破坏 build。Python 脚本 + 后置 grep 验证是更可靠的工作流
- **正则负向先行断言精准过滤**：`(?<![:.\w])(?<!fn )name(` 同时排除 `hotkey::default_xxx` / `theme_utils::default_xxx`（已加前缀）和 `fn default_xxx` 定义
- **`#[serde(default = "module::function")]` 跨模块路径**：serde derive 在父模块展开，相对路径自动相对父模块解析 — `pub(super)` 子模块函数可被 serde 字符串路径调用
- **行为零变化保证**：本轮 60 处替换涉及 28 个 serde 路径 + 32 个调用点，但函数实现 byte-for-byte 完全相同，纯位置迁移
- **fmt 收尾纪律**：`cargo fmt -p main --check` 输出列出**所有**未格式化点，需区分本轮引入与 baseline；只修本轮引入项以避免 PR 范围爆炸

### 轮 8a：`app_settings.rs` 数据形状（2026-06-15 完成）

**分支**：`refactor/setting-tab-split-r8a-app-settings-struct`

**决策**：原 "轮 8" 单轮搬移 600+ 行风险过高，经 Codex 评估细分为 8a/8b/8c：
- **8a**：`struct AppSettings` + `impl Default` + `impl gpui::Global`（数据形状层，本轮）
- **8b**：`impl AppSettings { ... }` 行为方法层（~380 行）
- **8c**：迁移辅助函数（`HotkeyMigration`、`migrate_legacy_*`、`legacy_terminal_settings` 等）

**改动**：
- 新增 `main/src/setting_tab/app_settings.rs`（195 行）
- 从 `setting_tab.rs` 移出（共 ~250 行）：
  - `pub struct AppSettings`（~50 字段 + 28 个 `#[serde(default = "theme_utils::*")]` + 2 个 `#[serde(default = "hotkey::*")]`）
  - `impl Default for AppSettings`（~60 行字面量）
  - `impl gpui::Global for AppSettings {}`
- `setting_tab.rs` 内：
  - 新增 `mod app_settings;`
  - `pub(crate) use app_settings::AppSettings;`（保持外部 14 处引用不变）
  - 移除父文件已无引用的 `serde::{Deserialize, Serialize}` import
- 子模块 `app_settings.rs` 内 import：
  - 数据类型：`use super::{cloud::{...}, proxy::GlobalProxySettings, saved_window::SavedWindowBounds, types::{...}};`
  - 模块级：`use super::{hotkey, theme_utils};`（**关键**：模块级 use 让 serde 路径字符串 `"theme_utils::default_X"` 在子模块上下文中按原样解析，无需改写 28 处 serde 字符串）
- `setting_tab.rs` 行数：4051 → 3876（−175 行）

**可见性策略**：
- `pub struct AppSettings`（字段全 `pub`），外部以 `crate::setting_tab::AppSettings` 通过 re-export 访问
- `impl gpui::Global` 跨模块声明完全合法（无可见性问题）

**关键发现：serde 路径字符串的稳定性**：
- `#[serde(default = "theme_utils::default_X")]` 字符串路径在 derive 展开点解析（= `app_settings.rs`）
- 子模块加 `use super::theme_utils;` 后，`theme_utils::X` 在子模块上下文与父模块完全等价
- 因此 **28 处 serde 路径字符串 0 改动**！避免了大规模重写

**验证**：
- ✅ `cargo check -p main` 0 error，9 warnings（与 baseline 一致），0 setting_tab lib warning
- ✅ `cargo check -p main --tests` 0 error（4 处测试 unused import 暴露为预存死代码，留下个独立 cleanup 处理）
- ✅ `cargo test -p main` 84 通过 / 1 失败（baseline i18n 缺陷）
- ✅ `rustfmt app_settings.rs` 通过
- ✅ Codex 审核：APPROVED（无 finding）

**经验**：
- **大型迁移必须细分**：单轮 600+ 行风险极大，三分子轮（数据/方法/辅助）每轮可独立 `cargo check` 验证
- **`use super::{module_name};` 模式** > 单项 use：搬移 struct 时，让子模块以**模块名**导入 sibling，serde derive 字符串可不动 — 极大降低跨轮联动改动量
- **本轮副作用：暴露测试块 dead import**。`AppSettings` 搬走后，原本通过同文件 derive "间接证明已用"的几个测试 import（`Serialize/Deserialize` 等）变成 unused — 标记为 baseline-grade dead code，留单独清理

### 轮 8b：`app_settings.rs` 行为方法层（2026-06-15 完成）

**分支**：`refactor/setting-tab-split-r8b-app-settings-impl`

**改动**：
- 将 `impl AppSettings { ... }` 整块（386 行，25 个方法）追加到 `main/src/setting_tab/app_settings.rs`
- 子模块 `app_settings.rs` 行数：195 → 597
- `setting_tab.rs` 行数：3876 → 3483（−393 行）
- 父文件清理已无引用 import：
  - `std::path::PathBuf` / `DbViewSettings` / `Pixels` / `WindowBackgroundAppearance` / `WindowBounds` / `ThemeMode` / `get_config_dir` / `tracing::{error, info}` / 重复的 `saved_window::SavedWindowBounds` / `saved_window::centered_window_bounds_within_visible_area`
- 父文件保留：`set_recovery_scrollback_lines`（`sync_terminal_settings_to_all` 仍调用，轮 8c 处理）
- 测试 import 调整：`centered_window_bounds_within_visible_area` 从父级 multi-line `use super::{...}` 拆出，改为显式 `use super::saved_window::centered_window_bounds_within_visible_area;`

**可见性策略（关键）**：
- 5 个方法从私有 `fn` 升 `pub(super) fn`：
  - `theme_preference_value` / `set_theme_preference` / `apply_ui_font_preferences`（父 UI render 调用）
  - `effective_theme_mode`（父测试调用）
  - `normalized_terminal_recovery_scrollback_lines`（父 `sync_terminal_settings_to_all` 调用）
- 其余私有方法保持 `fn`（`config_path` / `write_to_disk` / `manual_theme_mode` / `find_matching_theme_name` / `apply_misc_appearance_preferences` / `apply_window_background_preferences`）
- **父模块 `sync_follow_app_terminal_themes` 与 `resolve_linux_window_appearance_override` 保持原可见性**（私有 `fn` 与 `pub(crate) fn`） — Codex 指出子模块本身就能调父模块私有项，无需提升

**子模块向父模块反向引用**：
```rust
#[cfg(target_os = "linux")]
use super::resolve_linux_window_appearance_override;
use super::{hotkey, sync_follow_app_terminal_themes, theme_utils};
```

**验证**：
- ✅ `cargo check -p main` 0 error，9 warnings（与 baseline 一致），0 setting_tab lib warning
- ✅ `cargo check -p main --tests` 0 error
- ✅ `cargo test -p main` 84 通过 / 1 失败（baseline i18n 缺陷）
- ✅ `rustfmt app_settings.rs` 通过
- ✅ Codex 审核：APPROVED（仅 baseline fmt 问题 2 处，与本轮无关）

**经验**：
- **子模块可访问父模块私有项**：Rust 模块系统允许子模块直接调用父模块的 `fn`，无需 `pub(super)` 提升 — 这是常见的可见性误区，应严格按"父调子需放 `pub(super)`，子调父无需任何调整"使用
- **父模块 import 清理时机**：搬走大块代码后，**必须**对每个保留的 import 跑 grep 确认是否还有引用（如本轮 `set_recovery_scrollback_lines` 误删后重新引入，cargo check 立即捕获）
- **测试 import 跨模块调整**：当父模块不再 re-export 私有 sibling 时，测试需直接走 `use super::sibling_mod::item`
- **大块搬移的恢复机制**：先用 Python 删除 387 行 → cargo check 列出新 import 失效 → 单次 `Edit` 批量清理 import（含 5 个 group），避免逐项处理

### 轮 8c：`migrations.rs` 迁移/同步辅助（2026-06-15 完成）

**分支**：`refactor/setting-tab-split-r8c-migration-helpers`

**改动**：
- 新增 `main/src/setting_tab/migrations.rs`（154 行）
- 从 `setting_tab.rs` 移出（共 ~140 行）：
  - `pub struct HotkeyMigration` + `impl::any_changed`
  - `migrate_legacy_system_hotkey` / `is_legacy_ctrl_space`（热键迁移）
  - `migrate_legacy_theme_state` + 内部 `LegacyState` deserialize struct（主题状态迁移）
  - `sync_terminal_settings_to_all` / `sync_follow_app_terminal_themes`（GPUI 终端设置同步）
  - `legacy_terminal_settings`（数据转换）
  - `editable_sync_server_url` / `normalize_sync_server_url`（同步 URL 归一化）
- `setting_tab.rs` 内：
  - 新增 `mod migrations;` + `pub(crate) use migrations::HotkeyMigration;`（保持外部 `main.rs` 引用不变）
  - 主代码 25 处裸调用加 `migrations::` 前缀（Python 批量改写 + 负向先行断言过滤）
  - 测试 mod 内 12 处需 `super::migrations::xxx`（测试 mod 无法看见父模块的 sibling）
  - 清理已无引用 import：`SyncServerClient`、`TerminalSettings`、`set_recovery_scrollback_lines`、`GlobalTerminalSettings`、`TerminalSettingsStore`
- `app_settings.rs` 内：
  - `use super::{hotkey, sync_follow_app_terminal_themes, theme_utils};` → 拆为 `use super::migrations::sync_follow_app_terminal_themes;` + `use super::{hotkey, theme_utils};`
- `setting_tab.rs` 行数：3483 → 3347（−136 行）

**关键 bug 与恢复**：
- 第一次 Python 删除使用 `start_marker..end_marker` 范围删除，因 `build_app_http_client` (line 384) **位于** `editable_sync_server_url` (393) 之前但在我的删除范围内，被误删
- `cargo check` 立即捕获 `unresolved import crate::setting_tab::build_app_http_client`（来自 `onetcli_app/mod.rs`）
- 立即用 Edit 在 `apply_sync_server_url_setting` 之前重新插入 `build_app_http_client` 函数
- **教训**：范围删除若包含中间需保留的导出函数，cargo check 是有效兜底；未来应优先用锚点验证的 anchor 模式删除

**可见性策略**：
- `HotkeyMigration` `pub`（外部 `main.rs` 用）→ 父模块 `pub(crate) use` re-export
- 8 个 helper `pub(super) fn`（父模块 init/render 与父测试调用）
- 内部 `LegacyState` struct 定义在 `migrate_legacy_theme_state` 函数体内，无跨模块边界

**sibling-to-sibling 访问验证**：
- `migrations::sync_terminal_settings_to_all` 调用 `settings.normalized_terminal_recovery_scrollback_lines()` — 该方法在 sibling `app_settings.rs` 中为 `pub(super)`
- 用 standalone rustc 测试预先验证：sibling 可访问另一 sibling 的 `pub(super) fn` — 因为它们对共同父模块（`setting_tab`）都可见，所以彼此可见
- Codex 二次确认：GO

**验证**：
- ✅ `cargo check -p main` 0 error，9 warnings（baseline 一致），0 setting_tab lib warning
- ✅ `cargo check -p main --tests` 0 error
- ✅ `cargo test -p main` 84 通过 / 1 失败（baseline i18n 缺陷）
- ✅ `rustfmt migrations.rs` 通过
- ✅ `cargo fmt` 本轮 fmt diff 已修复（剩 3 处为 baseline）
- ✅ Codex 审核：APPROVED（仅 baseline fmt 问题）

**经验**：
- **范围删除的固有风险**：连续行删除若包含中间需保留的导出符号会破坏外部 import；cargo check 是有效兜底，但更稳妥是 anchor 精确删除（删 `fn xxx` 块时只锚定该 fn 起止）
- **测试 mod 跨 sibling 模块路径**：父模块声明 `mod migrations;` 后，测试 mod（嵌套在父中）需用 `super::migrations::xxx`，而**主代码**用裸 `migrations::xxx`。Python 批量改写需区分作用域
- **dead use 行清理策略**：测试代码若改用全路径调用（`super::migrations::editable_sync_server_url(...)`），相应的 `use` 行就成为 dead code — 移除 use 比保留更清晰
- **Sibling-to-sibling `pub(super)` 访问规则**：两个 sibling 子模块通过共同父模块可见性互访 — Rust 模块系统允许，pre-test 用 standalone rustc 验证可省去后续意外

### 轮 9a：`about.rs`（2026-06-15 完成）

**分支**：`refactor/setting-tab-split-r9-panel-render`

**决策**：原轮 9 单轮搬 1900 行风险过高，经 Codex 评估细分为 9a-9e（按叶子→根顺序）：
- **9a**：`about.rs`（最简，本轮）
- 9b：`shortcuts.rs`
- 9c：`auth_form.rs`
- 9d：`proxy_view.rs`
- 9e：`panel.rs`（SettingsPanel 主体，最后做）

**改动**：
- 新增 `main/src/setting_tab/about.rs`（116 行）
- 从 `setting_tab.rs` 移出（共 ~108 行）：
  - `const GITHUB_URL: &str`
  - `pub(super) fn render_about_section(cx: &App) -> gpui::AnyElement`
- `setting_tab.rs` 内：
  - 新增 `mod about;` + `use about::render_about_section;`（叶子模块，无外部消费者，不需 `pub(crate) use`）
  - 移除父文件已无引用的 `gpui::ClickEvent`、`gpui_component::clipboard::Clipboard` import
- `setting_tab.rs` 行数：3347 → 3243（−104 行）

**可见性策略**：
- `render_about_section` `pub(super)`，仅 `SettingsPanel::render` 调用
- `GITHUB_URL` 子模块内 `const`，无外部访问

**验证**：
- ✅ `cargo check -p main` 0 error，9 warnings（baseline 一致），0 setting_tab lib warning
- ✅ `cargo check -p main --tests` 0 error
- ✅ `cargo test -p main` 84 通过 / 1 失败（baseline i18n 缺陷）
- ✅ `rustfmt about.rs` 通过

**经验**：
- **叶子 UI 模块抽取低风险**：单一渲染函数 + 私有常量 + 单一调用点 — 是最佳的轮 9 启动模块
- **`use ... 子模块::符号`**（私有 use）vs `pub(crate) use ...`（re-export）：本轮 `render_about_section` 不被外部消费，只需私有 use；与之前所有 pub 类型迁移不同
- **`gpui::IntoElement` trait import 易遗漏**：`.into_any_element()` 调用需要该 trait 在作用域，单 trait import 漏掉是常见错误，cargo check 立即捕获

### 轮 9b：`shortcuts.rs`（2026-06-15 完成）

**分支**：`refactor/setting-tab-split-r9-panel-render`（同 9a 共享分支）

**改动**：
- 新增 `main/src/setting_tab/shortcuts.rs`（207 行）
- 从 `setting_tab.rs` 移出（共 ~191 行）：
  - `struct ShortcutEntry` / `struct ShortcutGroup`（私有）
  - 3 个 `const ShortcutEntry[]`：`WINDOW_SHORTCUTS` / `TAB_SHORTCUTS` / `TERMINAL_SHORTCUTS`
  - `const SHORTCUT_GROUPS: &[ShortcutGroup]`
  - 3 个 fn：`shortcut_spec_for_entry` / `render_shortcut_value` / `render_shortcuts_section`
- `setting_tab.rs` 内：
  - `mod shortcuts;` + `use shortcuts::render_shortcuts_section;`
  - 子模块用 `use super::app_settings::AppSettings;` + `use super::hotkey::{DEFAULT_SYSTEM_HOTKEY_*};` 反向引用 sibling
  - 清理父文件已无引用 import：`gpui::Keystroke`、`gpui_component::kbd::Kbd`
- `setting_tab.rs` 行数：3243 → 3050（−193 行）

**经验**：
- 包含**常量数组 + 私有 struct + 多个辅助 fn 的紧凑功能模块**是理想抽取单元 — 高内聚低耦合
- 仅 `render_shortcuts_section` 标 `pub(super)`，其他 fn/struct/const 全为私有
- 静态数组中引用 `super::hotkey::DEFAULT_SYSTEM_HOTKEY_*` 直接 `use` 后照常使用 — Rust 允许 const 表达式间接引用 sibling 模块的 const

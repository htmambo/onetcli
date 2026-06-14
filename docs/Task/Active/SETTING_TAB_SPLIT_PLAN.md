# P1 拆分 main/src/setting_tab.rs 任务计划

**状态**: 🔄 进行中 (轮 3 已完成于 2026-06-15)

## 进度追踪

| 轮次 | 子模块 | 状态 |
|---|---|---|
| 轮 1 | `hotkey.rs` | ✅ 已完成 (2026-06-15) |
| 轮 2 | `global_user.rs` | ✅ 已完成 (2026-06-15) |
| 轮 3 | `cloud.rs` | ✅ 已完成 (2026-06-15) |
| 轮 4 | `saved_window.rs` | ⏳ 待执行 |
| 轮 5 | `proxy.rs` | ⏳ 待执行 |
| 轮 6 | `types.rs` | ⏳ 待执行 |
| 轮 7 | `theme_utils.rs` | ⏳ 待执行 |
| 轮 8 | `app_settings.rs` | ⏳ 待执行 |
| 轮 9 | `SettingsPanel` Render | ⏳ 待执行 |

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

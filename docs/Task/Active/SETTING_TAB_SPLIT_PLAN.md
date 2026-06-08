# P1 拆分 main/src/setting_tab.rs 任务计划

**状态**: ⚠️ 已中停 (中停时间: 2026-06-08)
> 对应 fullauto 状态：.omc/fullauto/setting-tab-split/state.json (phase=failed)

## 中停说明

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

<!-- 各阶段完成后追加 -->

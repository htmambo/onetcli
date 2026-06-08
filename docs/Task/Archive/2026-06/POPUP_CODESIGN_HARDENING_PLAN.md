# P2 经验沉淀硬化：RoundedPopup 助手 + bundle-macos codesign 任务计划

**状态**: ✅ 已完成 (完成时间: 2026-06-08)
> 对应 fullauto 状态：.omc/fullauto/popup-codesign/state.json

## 任务目标

把 `AGENTS.md:329-342` 中沉淀的两条"已验证经验"从**文字规则**升级为**代码契约 / 流程自动化**：

- **A. RoundedPopup 助手**（经验 1：Wayland popup 圆角外溢）
- **B. bundle-macos codesign**（经验 2：macOS 15+/26 局域网 EHOSTUNREACH）

## 问题分析

### 经验 1 现状

- 根因：GPUI `overflow_hidden` 仅支持矩形 content mask
- 现行做法：在 popup 内容层（标题栏 / 根容器 / footer）**各自手画圆角**
- 痛点：4-6 个 `*_dialog.rs` 中各画一次，规则散落 + 容易遗漏
- 范围：所有通过 `one_core::popup_window` 打开的表单类弹窗

### 经验 2 现状

- 根因：macOS 15+/26 Local Network Privacy + 未签名 .app
- 现行做法：用户 / 维护者手动 `codesign --force --deep --sign -`
- 痛点：容易遗忘，打包 .app 偶发回归
- 范围：`script/bundle-macos.sh` 输出的 .app

## 子任务列表

- [ ] A1. 调研 `one_core::popup_window` 现有 API 与典型调用点（≥4 个）
- [ ] A2. 设计 `popup_helpers::RoundedPopup` 的最小可用 API（不破坏现有用法）
- [ ] A3. 落地 `crates/one_ui/src/popup_helpers.rs`
- [ ] A4. 迁移 1-2 个调用点（建议 `provider_form_dialog.rs` / `oauth_dialog.rs` 之一）
- [ ] A5. `cargo check -p one-core && cargo check -p main && cargo check -p one-ui`
- [ ] B1. 阅读 `script/bundle-macos.sh` 末尾结构
- [ ] B2. 末尾追加 ad-hoc codesign 步骤（仅在 `uname = Darwin` 时执行）
- [ ] B3. 更新脚本顶部注释 + `bash -n` 语法检查
- [ ] B4. 在 PLAN 中记录"经验 2 升级为流程自动化"的位置
- [ ] 共同：归档 + 更新 README 索引

## 每个子任务的改动内容

### A3. `crates/one_ui/src/popup_helpers.rs`（新文件）

最小 API 草案（**会在实施时按真实签名调整**）：

```rust
//! 复用 popup 圆角助手：解决 Wayland 下 popup_window 圆角外溢。
//! AGENTS.md:329 已验证经验。
//
// 目标：标题栏/根容器/footer 三段都自带圆角，让 popup 壳层只管边框/阴影。

pub fn rounded_title_bar(...) -> impl IntoElement { ... }
pub fn rounded_content_root(...) -> impl IntoElement { ... }
pub fn rounded_footer(...) -> impl IntoElement { ... }
```

### A4. 迁移候选

按"被修改频次 + 体积"权衡：先迁 1 个最简弹窗（避免破坏面过大），再观察测试结果。

### B2. `script/bundle-macos.sh` 末尾追加

```bash
# 末尾（仅 macOS）：ad-hoc 签名避免 macOS 15+/26 Local Network Privacy 拦截局域网
if [ "$(uname)" = "Darwin" ] && [ -d "$APP_BUNDLE" ]; then
  codesign --force --deep --sign - "$APP_BUNDLE" || \
    echo "⚠️ codesign 失败；启动 .app 后系统设置需手动授权本地网络"
fi
```

## 预期效果和验收标准

- [ ] `cargo check -p one-core -p main -p one-ui` 0 error
- [ ] 至少 1 个 popup 调用点成功迁移并保留原有外观
- [ ] `bash -n script/bundle-macos.sh` 通过
- [ ] `script/bundle-macos.sh` 末尾出现 `codesign` 调用且仅 macOS 分支
- [ ] 不修改 `AGENTS.md` 经验原文（升级为"机制化"但保留经验作为 fallback 说明）

## 风险评估和缓解措施

| 风险 | 缓解 |
|---|---|
| `RoundedPopup` 与 GPUI 现有 `rounded()` API 重名/冲突 | 使用 `one_ui::popup_helpers` 模块作用域，避免污染全局 |
| popup 调用点迁移破坏现有 UI 布局 | 仅迁 1 个最简的（`provider_form_dialog`），保留其他作为对照组 |
| `codesign --force --deep --sign -` 在 CI 失败 | 仅在 `[ "$(uname)" = "Darwin" ]` 守护下执行；非 macOS 跳过；CI 失败用 `||` 兜底为警告 |
| AD-hoc 签名被 Gatekeeper 拦截 | 文档化为"本地开发 + 自发布渠道够用"；长期方案（Developer ID）留作后续任务 |
| 真实 macOS 机器未在本会话中验证 | 提交时附"待用户在 macOS 实测 codesign 效果"作为 Not-tested trailer |

## 实施顺序和依赖关系

```
A1 ─→ A2 ─→ A3 ─→ A4 ─→ A5
                  │
                  └──→ [依赖 A5] ─→ 归档

B1 ─→ B2 ─→ B3 ─→ B4
                  │
                  └──→ [独立] ─→ 归档
```

A 和 B 互相独立，可并行；同 Phase 2 启动。

## 阶段 0 输出（spec）

- 路径：`.omc/fullauto/popup-codesign/spec.md`
- 关键决策：A 任务**降级**为"抽常量 + 编译期断言 + 测试守护"（不再迁移调用点；popup 壳层已自带圆角，原 bug 已被基础设施修复大半）
- 包含：## Assumptions Made / ## Decisions Made

## 实施计划

- 路径：`.omc/plans/fullauto-popup-codesign-impl.md`
- 任务 A：`crates/one_ui/src/popup_helpers.rs` 新增 `ROUNDED_POPUP_RADIUS_PX = 8.0` + 工具函数 `popup_radius_gpui()`；`crates/core/src/popup_window.rs` 引入 + 1 个新单元测试
- 任务 B：`script/bundle-macos.sh` 末尾追加 6 行 `[ "$(uname)" = "Darwin" ]` 守护的 codesign 块
- 任务 C：归档 + 提交准备（不自动 commit）

## Codex 顾问意见（Phase 1）

**会话 ID**: `019ea690-7654-7441-be3e-c72748e0c766`
**结果**: APPROVED_WITH_CHANGES（6 风险点）

### 关键风险（已采纳）

| # | 风险 | 处理 |
|---|---|---|
| **2** | **`one-core` 不依赖 `one-ui`** → 计划中 `use one_ui::...` 会编译失败 | **采纳**：常量定义放在 `crates/core/src/popup_window.rs` 内部（caller 同 crate，零依赖变更），`one_ui::popup_helpers` 改为 re-export 供 main 等下游使用 |
| 1 | `px(8.)` 失去主题适配 | 保留 `cx.theme().radius_lg`；新增常量仅用于"测试守护 + 文档化"语义，不替换运行时 |
| 3 | "编译期断言"未落地 | `popup_helpers.rs` 内增加 `const _: () = assert!(ROUNDED_POPUP_RADIUS_PX == 8.0);` |
| 4 | `popup_radius_gpui()` 可能变未用 API | 直接在 `PopupWindowView::render` 调用 `popup_radius_gpui()`，测试引用常量 |
| 5 | `codesign \|\| echo` 吞掉失败 | 改为 if/else 分支，明确成功/失败两种 echo（含 `$APP_DIR` 与后果说明） |
| 6 | codesign 插入点位置 | 按 Codex 建议放在 `echo "目录内容："` 之前，并加成功 echo |

### 采纳后的最终落地路径

- A 任务修改文件：
  1. `crates/core/src/popup_window.rs` 新增 `pub const ROUNDED_POPUP_RADIUS_PX: f32 = 8.0;` + `pub fn popup_radius_gpui()` + 编译期断言 + 1 单元测试
  2. `crates/one_ui/src/popup_helpers.rs` 新增 `pub use one_core::popup_window::{ROUNDED_POPUP_RADIUS_PX, popup_radius_gpui};`（**re-export only**）
  3. `crates/one_ui/src/lib.rs` 添加 `pub mod popup_helpers;`
- B 任务修改文件：`script/bundle-macos.sh`（按 Codex 建议 if/else 分支）





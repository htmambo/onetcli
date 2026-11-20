# Linux 弹窗显式父窗口（修复启动期「恢复连接」弹窗被主窗口覆盖）

**Status**: ✅ Completed (completion: 2026-11-20)，自动化验证与实机验证通过，待用户手测
**Created**: 2026-11-20
**Creator**: Claude Code
**Branch**: refactor/remove-external-db-drivers

## 1. 目标与背景

应用启动时弹出的「恢复连接」Dialog 弹窗没有被置顶，点击主窗口会把弹窗盖住。macOS 已在 commit `14bb4928` 修复，Linux 至今没有对应处理。

用户已选定**方案 A：显式父窗口**——在 vendored GPUI 中引入 `parent_window` 概念，让 Linux 平台创建 Dialog 时不再依赖"当前键盘焦点窗口"这一隐式推断。

## 2. 范围

- 仅 Linux（Wayland / X11）平台的 Dialog / Floating 父窗口解析逻辑。
- `crates/core/src/popup_window.rs` 的弹窗创建链路显式传递父窗口。
- macOS / Windows 行为保持不变（新字段在这些平台不被读取）。
- 不涉及弹窗内容、样式、交互逻辑的改动。

## 3. 现状（代码事实）

### 3.1 现象与运行时证据

本机为 KDE Plasma / KWin / Wayland（`XDG_SESSION_TYPE=wayland`、`XDG_CURRENT_DESKTOP=KDE`）。在 `crates/core/src/popup_window.rs` 临时加日志后实跑：

```
DIAG open_popup_window popup_title=恢复连接 parent_window_active=false kind=Dialog
```

即创建弹窗那一刻，主窗口**没有拿到键盘焦点**。

### 3.2 根因链路

- Linux 平台的 Dialog 父窗口不是显式传入，而是隐式推断：
  - `vendor/zed/crates/gpui/src/platform/linux/wayland/client.rs:752`：`let parent = state.keyboard_focused_window.clone();`
  - `vendor/zed/crates/gpui/src/platform/linux/x11/client.rs:1691-1694`：同样取 `keyboard_focused_window`。
- `keyboard_focused_window` 只在收到 `wl_keyboard::Event::Enter` 时才赋值（`wayland/client.rs:1327-1335`）。应用启动阶段主窗口尚未获得键盘焦点，因此 parent 为 `None`。
- parent 为 `None` 时：
  - Wayland（`wayland/window.rs:192-210`）不调用 `xdg_toplevel.set_parent`，`xdg_dialog` 的 modal 因缺父 toplevel 失效，弹窗退化为独立 `xdg_toplevel`，合成器不保证其位于主窗口之上。
  - X11（`x11/window.rs:819-875`）不设置 `WM_TRANSIENT_FOR`，WM 同样不保证层级。`add_child`（`x11/window.rs:1473`）只是 bookkeeping（阻断父窗口输入、随父窗口关闭），不做 stacking。
- macOS 走 `NSApp.mainWindow` 作为 sheet 父级（与焦点无关），且 `14bb4928` 已补"无主窗口时降级为 `NSFloatingWindowLevel` 浮层"，因此 macOS 正常。
- 应用自身"窗口置顶"开关只实现 macOS / Windows（`main/src/omnihub_app/window_actions.rs:86-113`），Linux 返回 unsupported，无兜底手段。
- 次生问题：弹窗映射并获得焦点后它自己成为 `keyboard_focused_window`，此后新开的 popup 会以它为父级，父链串联。

## 4. 方案

### 4.1 GPUI 增加显式父窗口字段

- `vendor/zed/crates/gpui/src/platform.rs`
  - `WindowOptions` 增加 `pub parent_window: Option<AnyWindowHandle>`。
  - `WindowParams` 增加同名字段；非 Linux/FreeBSD 平台用 `cfg_attr(..., allow(dead_code))` 消除未使用告警。
  - `impl Default for WindowOptions` 补 `parent_window: None`。
- `vendor/zed/crates/gpui/src/window.rs`（`Window::new`）：把 `options.parent_window` 透传进 `WindowParams`。

### 4.2 Linux 平台优先使用显式父窗口

- X11（`x11/client.rs` `open_window`）：先按 `AnyWindowHandle` 在 `state.windows` 中反查窗口状态（`X11WindowState.handle` 为 `pub`），查不到再回退 `keyboard_focused_window`；显式父窗口"指定了但找不到"时打 `log::warn!`。
- Wayland（`wayland/client.rs` `open_window`）：同上，走新增的 `WaylandWindowStatePtr::handle_and_destroyed()`（单次借用取句柄与销毁标志）。
- 反查为线性扫描：窗口数量为个位数，且窗口创建是低频操作，成本可忽略。

### 4.3 应用侧接线

- 新增 `crates/core/src/popup_window/options.rs`：把窗口选项构造抽成可测的纯函数 `popup_window_options(config)`，其中显式填入 `parent_window: Some(parent_window_handle)`（`parent_window.window_handle()` 即 `AnyWindowHandle`）。
- `crates/core/src/popup_window.rs` 调用该函数；新增 `crates/core/src/popup_window/` 子模块以避免继续膨胀已超长的 `popup_window.rs`（423 行）。

### 4.4 行为变化

弹窗在 Linux 上从此与父窗口建立 transient / modal 关系：

- 父窗口在弹窗打开期间不再接收输入（Wayland `handle_input` 因 `is_blocked` 提前返回；X11 同样逻辑），与 macOS sheet 语义一致。
- 父窗口关闭时子窗口一并关闭（既有 `add_child` 语义，不变）。
- 用户主动打开的弹窗此前 parent 就是主窗口，行为不变；变化只发生在"父窗口无焦点"的场景（典型即启动期弹窗）。

## 5. 子任务

1. ✅ 根因定位与运行时证据确认（临时诊断日志，已回滚）
2. ✅ 外部评审（`mcp__coding-bridge__review_plan`）——批准执行，附 4 条改进建议
3. ✅ 任务计划文档归档
4. ✅ GPUI 增加 `parent_window` 字段并透传（platform.rs / window.rs）
5. ✅ X11 / Wayland 平台优先使用显式父窗口
6. ✅ `crates/core/src/popup_window.rs` 接线 + `popup_window/options.rs` 单测
7. ✅ 编译验证：`cargo build -p main` 通过、无新增 warning
8. ✅ 实机验证：启动期「恢复连接」弹窗在 KWin 侧为 `modal=true` 且 `transientFor` 指向主窗口
9. ✅ 外部代码评审（`mcp__coding-bridge__review_code`）——11 条建议，采纳 4 条、驳回 7 条（见 §7）
10. ✅ AGENTS.md 经验沉淀 + 归档 + README 索引更新

## 6. 验收结果

- `cargo build -p main`：通过，无新增 warning（gpui 4 条 / gpui-component 9 条 warning 为改动前既有）。
- `cargo test -p one-core --lib`：236 passed / 0 failed；其中 `popup_window` 相关 7 passed（含新增 2 条）。
- `cargo clippy -p one-core --no-deps --all-targets`：改动文件无任何 clippy 告警（仓库既有 209 条历史告警分布在其他文件，与本次无关；`crates/ssh` 的 `unwrap_used` 报错同为既有）。
- 实机（KDE Plasma / KWin / Wayland）验证：
  - 应用侧诊断：`DIAG wayland open_window explicit_parent=true`、`new_window kind=Dialog parent_present=true xdg_parent_present=true`。
  - 合成器侧（KWin 脚本枚举窗口）：主窗口 `modal=false transientFor=[null]`；「恢复连接」弹窗 `modal=true transientFor=[OmniHub - 首页]` —— 弹窗已成为主窗口的模态 transient 子窗口，合成器保证其位于父窗口之上并阻断父窗口输入。
  - 验证环境隔离：最终一次复现使用独立 `HOME`（`/tmp/omnihub-diag-home`）+ 人工快照，未改动用户真实配置；验证后已清理，用户会话状态（3 个 SSH 终端标签）保持完整。

## 7. 外部代码评审处置

| # | 建议 | 处置 |
|---|---|---|
| 🔴1 | `..Default::default()` 可能丢弃 `focus`/`show` | 驳回：原内联构造同样使用 `..Default::default()`，`WindowOptions::default()` 的 `focus`/`show` 本就是 `true`，行为无变化 |
| 🔴2 | 显式父窗口查找失败时静默回退 | **采纳**：改为 `match` 区分"未指定"与"指定但找不到"，后者打 `log::warn!` |
| 🔴3 | 父窗口 `xdg_toplevel` 可能未 configure | 驳回：xdg-shell 允许 `set_parent` 在 configure 之前调用；gpui 中父窗口的 `xdg_toplevel` 在窗口创建时即已生成，窗口销毁已由 `handle_and_destroyed()` 过滤 |
| 🔴4 | vendored 代码应使用英文注释 | 驳回：与本仓既有 vendor 补丁（commit `14bb4928` 的 `mac/window.rs`）保持一致，且项目规范要求注释使用简体中文 |
| 🟡5 | Wayland 双重借用 vs X11 单次借用 | **采纳**：新增 `handle_and_destroyed()` 单次借用返回 `(handle, destroyed)` |
| 🟡6 | FreeBSD cfg 不一致 | 驳回：`window_decorations` 的 `cfg(target_os = "linux")` 是改动前既有行为，未引入回归；`WindowParams` 的 cfg 已覆盖 FreeBSD 复用 Linux 平台层的情况 |
| 🟡7 | `tempfile` 可能未使用 | 驳回：`tempfile` 是改动前既有 dev-dependency，本次新增的是 `gpui` 的 `test-support` |
| 🟡8 | 线性扫描缺少性能假设注释 | **采纳**：两端均补注释说明窗口数量与调用频率假设 |
| 🟢9 | 测试覆盖不足 | **采纳**：补 `popup_window_options_dialog_has_no_titlebar` 用例（Dialog 不带标题栏是 macOS sheet 避免空指针的关键约束） |
| 🟢10 | `PopupWindowConfig` 可派生 Default / builder | 驳回：当前仅一处调用，YAGNI |
| 🟢11 | `TitleBar::title_bar_options()` 可换成 `Default` | 驳回：该静态方法携带既有语义，改动无收益 |

## 8. 风险与缓解

| 风险 | 缓解 |
|---|---|
| modal 语义使父窗口在弹窗打开期间不可交互 | 与 macOS sheet 语义一致，属预期；已列入回归项 |
| 反查 handle 时遍历窗口表 | 窗口数量个位数，线性遍历成本可忽略；不持有可变借用期间执行平台调用 |
| vendor 改动未来同步上游时冲突 | 改动集中在字段透传与父窗口解析两处，已在本文件与 AGENTS.md 记录 |
| 其他合成器（Mutter / Sway）表现未覆盖 | 本机仅 KWin 可验证；Wayland 走标准 xdg-shell transient 语义，Mutter/Sway 理论一致，风险可接受 |

## 9. 回滚

改动集中在 `vendor/zed/crates/gpui` 与 `crates/core/src/popup_window*`，回滚锚点为本次提交（标题 `fix(gpui/linux): 弹窗显式父窗口，修复启动期「恢复连接」弹窗被主窗口覆盖`；commit hash 会随修订变化，故此处不写死，用 `git log --oneline -1` 定位）。回滚方式：`git revert <该 commit>`。

## 10. 备注

- 评审建议的"GPUI 层单测验证 `WindowParams` 透传"未采纳：headless 测试平台不实现 transient 语义，断言不到真实行为；改为应用侧单测 + 实机验证，局限已在 §8 记录。
- 评审建议的"多 WM 覆盖"受限于本机仅 KDE/KWin 一套环境，其余合成器无法实测。
- 实机验证手段沉淀到 AGENTS.md：KWin 脚本枚举窗口可读出 `modal` / `transientFor`，比人工截图更可靠。

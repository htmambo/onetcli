## 项目上下文摘要（全局 UI 字体设置接通）
生成时间：2026-03-26 20:39:21 +0800

### 1. 相似实现分析
- **实现1**: `crates/ui/src/root.rs:449`
  - 模式：全局 Theme 的 `font_size` 会通过 `window.set_rem_size(...)` 影响整个窗口的 rem 基线。
  - 可复用：只要把设置写回 `Theme::global_mut(cx).font_size`，普通 UI 就会跟着变。
  - 需注意：需要刷新窗口才能立即看到变化。

- **实现2**: `crates/ui/src/root.rs:459`
  - 模式：根容器直接使用 `cx.theme().font_family` 作为默认字体。
  - 可复用：把设置写回 `Theme::global_mut(cx).font_family` 就能影响普通界面文本。
  - 需注意：这不会影响终端等自行维护等宽字体的模块。

- **实现3**: `crates/story/src/title_bar.rs:110`
  - 模式：Story 工具里改全局字号后会直接 `window.refresh()`。
  - 可复用：全局 Theme 改完后需要立即刷新窗口。
  - 需注意：主应用有多个窗口，最好统一 `cx.refresh_windows()`。

- **实现4**: `crates/core/src/themes.rs:45`
  - 模式：主题切换和主题配置变更后会 `cx.refresh_windows()`，确保所有窗口统一刷新。
  - 可复用：字体设置属于同类“全局 Theme 参数”，也应该走全窗口刷新。
  - 需注意：`Theme::change(...)` 会重新套用主题配置，后续自定义覆盖要重新写回。

- **实现5**: `main/src/setting_tab.rs:389`
  - 模式：`AppSettings::apply(...)` 目前只应用语言、明暗主题和自动保存配置。
  - 可复用：把 UI 字体设置并入这里，就能覆盖启动加载和磁盘重载两条路径。
  - 需注意：当前设置页里的“字体 / 字号”只保存不生效，正是这次要补的缺口。

### 2. 项目约定
- **命名约定**: 继续复用 `font_family` / `font_size` 作为普通 UI 设置字段；终端专属字段保持 `terminal_*`。
- **文件组织**: 仅修改 `main/src/setting_tab.rs`，不把普通 UI 字体逻辑散落到页面层。
- **代码风格**: 用一个统一辅助函数同步 Theme 和刷新窗口，避免每个设置项各写一套。

### 3. 可复用组件清单
- `Theme::global_mut(cx)`
- `Theme::change(...)`
- `cx.refresh_windows()`
- `Root::render(...)`
- `AppSettings::apply(...)`

### 4. 测试策略
- **验证方式**: `cargo fmt --all` + `cargo check -p main`
- **人工验证建议**: 修改首页设置里的字体/字号，观察首页、设置页、弹窗等普通 UI 是否立即变化，同时确认终端字号不受影响

### 5. 依赖和集成点
- **内部依赖**: `setting_tab.rs`、`root.rs`、`theme/mod.rs`
- **集成方式**: `AppSettings -> Theme::global_mut(cx) -> cx.refresh_windows()`

### 6. 技术选型理由
- **为什么用这个方案**: 这组设置本来就是普通 UI 设置字段，最合理的方式是接回全局 Theme，而不是再造一个应用级字体系统。
- **优势**: 启动加载、设置页修改、主题切换三条路径都能统一生效。
- **风险**: 只会影响使用全局 Theme 字体的普通 UI，不会改终端或代码块的等宽字体。

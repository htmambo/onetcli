# OnetCli 页面级颜色 Token 与透亮效果审计
更新时间：2026-04-18

## 范围与口径
- 统计范围：运行时产品界面，包含 `main/` 主应用页面，以及 `terminal_view`、`db_view`、`mongodb_view`、`redis_view`、`sftp_view` 的实际页面/窗口/标签页。
- 排除范围：`docs/` 文档站点、`crates/story/` 组件故事页、测试夹具、主题样例。
- 页面口径：按“用户可见页面/窗口/标签页”分组；同一页面内部再按层级描述。
- 颜色 token 口径：只统计 `cx.theme().<color>` 颜色字段；`window_blur_enabled`、`surface_opacity`、`radius`、`mode` 记为控制项，不算颜色 token。
- “透亮”口径：包括透明背景、alpha 计算、毛玻璃开关、Windows 分层透明度、局部 `.opacity(...)` 叠加、`glass_sidebar` / `windows_surface_color` / `modal_surface_palette` 等再计算入口。

## 实施更新（2026-04-18）
- 已落地：
  - `Home / Settings / Terminal` 的页面局部 alpha 公式已收口到 `crates/ui/src/surface_alpha.rs`
  - `Dialog` 的实现与测试语义已统一：content 最低 alpha 为 `0.70`，chrome/title/footer 走 `+0.20` 后 clamp
  - 已补充 `surface_alpha` 与 `plain_surface_tuning(...)` 的单元测试
  - `Terminal / SSH` 现有右侧窄工具栏背景与按钮态已切回应用主题语义，不再依赖 terminal palette；底板在 `sidebar` 基础上叠一层 `secondary` 做感知提亮，按钮态使用 `list_active / sidebar_accent`
  - `Terminal / SSH` 展开侧栏容器与子面板已避免再次用 `background` 覆盖外层 surface，文件管理搜索条与监控卡片改回更浅的 `secondary` 层
  - `DB / Mongo / Redis` 侧栏入口已统一改为消费 `sidebar_surface_color(...)`
  - `DatabaseObjects` toolbar 与输入框已改为消费 `sidebar_surface_color_with_offset(...)`
  - `Table Designer` 的表头 / 选中态 / hover / 行分隔 / 拖拽 chip 已改为消费 `table_*` 语义 token 与语义边框
  - `SFTP` 的 shell / search bar / header / path bar 已接入 `layered_surface_color(...)`，拖拽卡片 / drop overlay / connection overlay 卡片与 scrim 也已改为统一 token 和 Windows 分层规则
  - 已新增 `overlay_scrim_color(...)` 与 `OverlayScrimLevel`，先把 `Blocking / Loading` 两档 scrim 强度收口到统一 helper
- 已验证：
  - `cargo test -p gpui-component -- --nocapture`：通过
  - `cargo build -p terminal_view`：通过
  - `cargo build -p main`：通过
- 当前剩余重点：
  - 页面内仍有分散的局部透明度常数，主要集中在 import/export 强调态、终端监控图表与少量 hover 态
  - `overlay/scrim` 已有统一 helper，但目前只先覆盖了 `Blocking / Loading` 两档，Dialog / Sheet 等入口还未统一并到同一语义层

## 全局结论
- 项目采用“原始颜色基线 -> 语义 fallback -> 主题层 glass tuning -> 页面局部层级修正”的链路，不是页面零散硬编码透明色。
- 高频页面 token 主要是：`background`、`border`、`muted_foreground`、`primary`、`secondary`、`sidebar`、`list_hover`、`list_active`。
- 高频透亮入口主要是：`Theme::set_window_surface_preferences(...)`、`apply_glass_tuning(...)`、`glass_sidebar(...)`、`windows_surface_color(...)`、`modal_surface_palette(...)`。

## 全局颜色与透明度规则

### 亮色 / 暗色原始基线
来源：`crates/ui/src/tokens/color/primitives.rs:11-97`、`crates/ui/src/tokens/color/primitives.rs:104-190`

| 类别 | Dark | Light |
| --- | --- | --- |
| `BG_BASE` | `#1a1a1a` | `#ffffff` |
| `BG_ELEVATED` | `#242424` | `#f5f5f5` |
| `BG_SURFACE` | `#2d2d2d` | `#ebebeb` |
| `TEXT_PRIMARY` | `#ffffff` | `#1a1a1a` |
| `PRIMARY` | `#3b82f6` | `#2563eb` |
| `ACCENT` | `#8b5cf6` | `#7c3aed` |
| `SUCCESS` | `#22c55e` | `#16a34a` |
| `WARNING` | `#f59e0b` | `#d97706` |
| `DANGER` | `#ef4444` | `#dc2626` |
| `INFO` | `#06b6d4` | `#0891b2` |

### 语义 token fallback
来源：`crates/ui/src/theme/schema.rs:470-637`

- 公共 fallback：`muted_foreground = muted.blend(foreground.opacity(0.7))`，`primary_hover = background.blend(primary.opacity(0.9))`，`list_active = background.blend(primary.opacity(0.1))`，`list_active_border = background.blend(primary.opacity(0.6))`，`list_hover = accent.opacity(0.6)`，`sidebar = background.blend(border.opacity(0.15))`，`drag_border = primary.opacity(0.65)`，`drop_target = primary.opacity(0.2)`。
- 亮暗差异：
  - `active_darken`：暗色 `0.2`，亮色 `0.1`。
  - `group_box`：暗色走 `secondary.opacity(0.3)`，亮色走 `secondary.opacity(0.4)`。
  - `tab`：暗色为 `background.blend(border.opacity(0.15))`，亮色直接回退到 `secondary`。
  - `tab_active`：暗色回退到 `secondary`，亮色回退到 `background`。

### 全局毛玻璃与透明度
来源：`crates/ui/src/theme/mod.rs:29-80`、`crates/ui/src/theme/mod.rs:337-338`、`main/src/setting_tab.rs:1086-1135`

- 默认透明度：`0.84`；用户配置会被 clamp 到 `0.40~1.00`。
- `AppSettings::apply_theme_preferences(...)` 会调用 `Theme::set_window_surface_preferences(enable_glass_effect, glass_opacity, cx)`，并刷新整个主题色。
- `enable_glass_effect = false` 时窗口背景为 `WindowBackgroundAppearance::Opaque`；开启后 Linux / macOS / Windows 都走 `WindowBackgroundAppearance::Blurred`。

### glass tuning 与平台差异
来源：`crates/ui/src/theme/glass.rs:32-140`、`crates/ui/src/theme/glass.rs:143-217`、`crates/ui/src/theme/mod.rs:35-80`

- 统一被玻璃化的表面 token：`background`、`group_box`、`list*`、`muted`、`popover`、`secondary*`、`sidebar*`、`tab*`、`table*`、`title_bar*`、`tiles`、`window_border`。
- blur 开启时：
  - 非 macOS：`base = opacity`，`elevated` 约 `+0.06`，`chrome` 约 `-0.04`，`hover/active` 更高。
  - macOS：alpha 更低，给系统材质更多露出空间。
- blur 关闭时仍会执行 `plain_surface_tuning(...)`，所以“关毛玻璃”不等于所有页面都回到纯实色。
- Windows 还会额外乘分层系数：
  - `ContentBase`：blur 开启 `0.68`，关闭 `0.45`
  - `ContentSection`：blur 开启 `0.48`，关闭 `0.24`
  - `ContentCard`：blur 开启 `0.34`，关闭 `0.14`
  - `TerminalFallback`：blur 开启 `0.36`，关闭 `0.16`
  - `TerminalCanvas`：blur 开启 `0.52`，关闭 `0.34`

### 通用侧栏 / Dialog 规则
来源：`crates/ui/src/glass_sidebar.rs:1-31`、`crates/ui/src/theme/glass.rs:117-140`、`crates/ui/src/theme/glass.rs:256-266`

- `glass_sidebar(color, blur, opacity)`：blur 开启时统一使用 `alpha = opacity + 0.20`。
- `modal_surface_palette(...)` 使用 `colors_without_glass()`，即对话框先取未二次玻璃化的基础色，再单独算 alpha，避免弹窗叠得过实。
- Dialog 当前实现：`content alpha = max(opacity, 0.70)`，`title_bar/footer alpha = content + 0.20`。

### 审计备注
来源：`crates/ui/src/theme/glass.rs:4`、`crates/ui/src/theme/glass.rs:282-290`

- 代码常量 `DIALOG_SURFACE_BASE_OPACITY = 0.70`。
- 初次审计时发现 Dialog 相关测试真值与实现不一致；当前已统一到“实现为准”。
- 当前口径：content 最低 alpha `0.70`，chrome/title/footer 为 `content + 0.20` 后 clamp。

## 页面级审计

### 1. 应用壳层 / Root
核心文件：`crates/ui/src/root.rs:460-479`、`main/src/onetcli_app.rs:1374-1407`

- 颜色 token：`background`、`foreground`、`transparent`、`border`，状态条附带 `red` / `yellow` / `green` / `muted` / `muted_foreground`。
- 层级：
  - Root 层：blur 开启时直接用 `transparent`，关闭时回到 `background`。
  - `OnetCliApp` 层：固定 `bg(cx.theme().transparent)`，避免和页面内容双重铺底。
- 结论：壳层本身不承担主要色块，玻璃感主要交给页面内容层。

### 2. Home 页
核心文件：`main/src/home_tab.rs:122-136`、`main/src/home_tab.rs:2614-2720`、`main/src/home_tab.rs:3139-3339`、`main/src/home_tab.rs:4379-4475`、`main/src/home_tab.rs:5011-5088`、`main/src/home_tab.rs:5721-5810`、`main/src/home_tab.rs:7296-7372`

- 颜色 token：`background`、`border`、`danger`、`drag_border`、`drop_target`、`foreground`、`list_active`、`list_active_border`、`list_hover`、`muted`、`muted_foreground`、`primary`、`primary_foreground`、`sidebar`、`sidebar_accent`、`success`、`tab`、`transparent`、`warning`。
- 层级：
  - Layer 0，页面壳层：`home_shell_bg` 在 Windows 或 blur 开启时使用 `transparent`，否则使用 `background`。
  - Layer 1，主内容底板：`muted` 先走 `macos_home_glass(..., level_ratio=0.10)`，再按 Windows `ContentBase` 修正。
  - Layer 2，工具栏：`background` 走 `macos_home_glass(..., 0.14)`，再按 `ContentSection` 修正。
  - Layer 3，左侧侧栏：`sidebar`、`list_active`、`sidebar_accent` 统一走 `glass_sidebar_f64(...)`。
  - Layer 4，工作区 section：容器底色用 `tab`，hover 用 `list_hover`，都先经 `macos_home_glass(...)` 再做 Windows 分层。
  - Layer 5，连接列表项 / 卡片：列表项底板是 `background`，图标承托底色是 `muted`，卡片与 overlay 卡片走 `level_ratio=0.16 / 0.18`。
  - Layer 6，拖拽与投放提示：大量使用 `drop_target.opacity(...)` 与 `drag_border.opacity(...)`。
- 页面局部算法：`macos_home_glass(...)` 使用 `alpha = glass_opacity + (level_ratio - 0.08)`；Windows 下还会再被 `windows_surface_color(...)` 压缩。

### 3. Settings 页
核心文件：`main/src/setting_tab.rs:694-718`、`main/src/setting_tab.rs:3139-3177`、`main/src/setting_tab.rs:3165-3201`、`main/src/settings/llm_providers_view.rs:211-320`

- 颜色 token：`background`、`border`、`danger`、`foreground`、`group_box`、`link`、`muted`、`muted_foreground`、`popover`、`primary`、`primary_foreground`、`secondary`、`sidebar`、`sidebar_border`、`sidebar_foreground`、`success`、`transparent`。
- 层级：
  - Layer 1，设置侧栏：`sidebar_bg = settings_glass(sidebar, blur, glass_opacity)`，即 `alpha = glass_opacity + 0.20`。
  - Layer 2，页面背景：`page_bg = settings_glass_with_offset(background, ..., +0.02)`。
  - Layer 3，页面 header：`header_bg = settings_glass_with_offset(secondary, ..., +0.28)`。
  - Layer 4，分组容器：`group_box` 再经 `settings_glass_with_offset(..., +0.14)`。
  - Layer 5，LLM Provider 页面：外层容器使用 `background.opacity(0.10)`，卡片使用 `background.opacity(0.18)`。
- 结论：Settings 没复用 Home 的 `macos_home_glass(...)`，而是维护了一套更线性的 `settings_glass(...)` / `settings_glass_with_offset(...)`。

### 4. 通用弹窗 / Popover / 小窗口
核心文件：`crates/ui/src/dialog.rs:436-662`、`main/src/saved_connection_picker.rs:516-549`、`main/src/update/dialog.rs:326-417`、`main/src/connection_restore.rs:470-548`

- 颜色 token：`background`、`border`、`foreground`、`muted_foreground`、`popover`、`link`、`info`、`success`、`warning`、`danger`。
- 层级：
  - 通用 Dialog：标题栏用 `dialog_palette.title_bar`，内容区用 `dialog_palette.content`，footer 用 `dialog_palette.footer`。
  - Saved Connection Picker：按钮使用 `tab_active` / `tab.opacity(0.8)` / `tab.opacity(0.5)` / `tab_foreground`；Popover 面板本身是 `popover + border`。
  - Update Dialog：主体直接使用 `background`、`foreground`、`muted_foreground`、`link`，没有额外 page-local glass 公式。
  - Connection Restore Popup：卡片直接用 `background + border`，类型颜色分别用 `warning`、`info`、`success`、`danger`。

### 5. Terminal 工作区
核心文件：`crates/terminal_view/src/view.rs:131-147`、`crates/terminal_view/src/view.rs:3962-4109`、`crates/terminal_view/src/sidebar/mod.rs:543-651`、`crates/terminal_view/src/sidebar/server_monitor_panel.rs:787-1314`

- 颜色 token：`background`、`border`、`chart_1`、`chart_2`、`chart_3`、`chart_4`、`danger`、`foreground`、`list_active`、`list_hover`、`muted`、`muted_foreground`、`primary`、`primary_foreground`、`secondary`、`selection`、`title_bar`、`warning`。
- 控制项：`surface_opacity`、`window_blur_enabled`。
- 层级：
  - Terminal Canvas：Windows 走 `TerminalCanvas` 层；非 Windows + blur 开启时直接在终端主题上使用 `surface_opacity + 0.05`，并启用 `with_material_tint(true)`。
  - Canvas Fallback 背景：Windows 用 `windows_surface_color(background, ..., TerminalFallback)`；其他平台 blur 开启时直接用 `transparent`。
  - 右侧 Sidebar：现有窄工具栏已切回应用主题侧栏语义，底板使用 `sidebar + secondary` 的混合层再走 `sidebar_surface_color(...)`，按钮选中/悬浮态使用 `list_active / sidebar_accent`；展开面板与子面板也已避免用 `background` 把外层 surface 盖黑。
  - Server Monitor：危险块用 `danger.opacity(0.08)`，告警块用 `warning.opacity(0.12)`，图表渐变用 `chart_1.opacity(0.35) -> background.opacity(0.1)`。
- 结论：Terminal 是平台分支最明显的页面，既吃全局 glass，又重新换算 terminal canvas 的透明度。

### 6. Database 工作区
核心文件：`crates/db_view/src/database_tab.rs:655-706`、`crates/db_view/src/db_tree_view.rs:2100-2148`、`crates/db_view/src/sidebar/mod.rs:229-248`、`crates/db_view/src/database_objects_tab.rs:985-1038`、`crates/db_view/src/table_designer_tab.rs:2333-2924`

- 颜色 token：`accent`、`accent_foreground`、`background`、`blue`、`border`、`danger`、`danger_foreground`、`foreground`、`input_background`、`list_active`、`list_hover`、`muted`、`muted_foreground`、`primary`、`primary_foreground`、`secondary`、`sidebar`、`sidebar_accent`、`sidebar_accent_foreground`、`sidebar_border`、`sidebar_foreground`、`success`、`table_active`、`table_active_border`、`table_head`、`table_head_foreground`、`warning`。
- 控制项：`surface_opacity`、`window_blur_enabled`。
- 层级：
  - Shell 结构层：`DatabaseTabView` 主要用 `border` 切分三栏，具体表面透明度交给子视图。
  - DB Tree：背景与搜索框输入层已统一改为 `sidebar_surface_color(...)`，边框为 `sidebar_border.opacity(0.6)`。
  - AI Sidebar：背景、高亮、hover 已统一改为 `sidebar_surface_color(...)`。
  - Objects Toolbar：已统一改为 `sidebar_surface_color_with_offset(background, ..., -0.10)`，输入框使用 `sidebar_surface_color_with_offset(input_background, ..., -0.18)`。
  - Table Designer：表头、选中态、hover、行分隔、拖拽 chip 已统一回到 `table_head`、`table_head_foreground`、`table_active`、`table_hover`、`table_row_border`、`table_active_border`。
  - Import / Export / Run SQL：强调态多用 `primary.opacity(0.2)`。

### 7. MongoDB 工作区
核心文件：`crates/mongodb_view/src/mongo_tab.rs:375-423`、`crates/mongodb_view/src/sidebar.rs:113-196`、`crates/mongodb_view/src/collection_view.rs:2900-2923`

- 颜色 token：`accent`、`accent_foreground`、`background`、`border`、`danger`、`foreground`、`list_active`、`muted`、`muted_foreground`、`sidebar`。
- 控制项：`surface_opacity`、`window_blur_enabled`。
- 层级：
  - Shell：`MongoTabView` 用 `border` 做树区、内容区、侧边栏分隔。
  - Sidebar：工具按钮高亮、hover、工具栏背景、展开面板背景都已统一改为 `sidebar_surface_color(...)`。
  - CollectionView：主要是 header + tab bar + body，没有额外 page-local glass 公式，主要继承外围 shell 与全局 theme glass。

### 8. Redis 工作区
核心文件：`crates/redis_view/src/redis_tab.rs:401-446`、`crates/redis_view/src/sidebar.rs:113-193`、`crates/redis_view/src/key_value_view.rs:2798-2835`、`crates/redis_view/src/redis_tree_view.rs:1547-1851`

- 颜色 token：`accent`、`accent_foreground`、`background`、`border`、`danger`、`list_active`、`list_hover`、`muted`、`muted_foreground`、`primary`、`primary_foreground`、`secondary`、`secondary_foreground`、`success`、`warning`。
- 控制项：`surface_opacity`、`window_blur_enabled`。
- 层级：
  - Shell：`RedisTabView` 与 Mongo / DB 一样负责多栏布局与分隔。
  - Sidebar：与 Mongo / DB 同构，当前已统一改为 `sidebar_surface_color(...)`。
  - KeyValueView：主体直接使用 `background`，page-local 玻璃化很少；行操作按钮主要用 `opacity(0.) -> group_hover(... opacity(1.))`。
  - Redis Tree：选中态偏向 `list_active`，hover 用 `list_hover`，状态语义色使用 `success` / `warning` / `danger`。

### 9. SFTP 工作区
核心文件：`crates/sftp_view/src/lib.rs:4167-4181`、`crates/sftp_view/src/file_list_panel.rs:494-529`、`crates/sftp_view/src/file_list_panel.rs:600-658`、`crates/sftp_view/src/lib.rs:3217-3257`

- 颜色 token：`background`、`border`、`danger`、`drag_border`、`link`、`list_active`、`list_hover`、`muted_foreground`、`overlay`、`secondary`、`selection`、`title_bar`。
- 层级：
  - Shell：`SftpView` 根背景已接入 `layered_surface_color(background, ..., ContentBase)`。
  - 文件列表：搜索栏已接入 `layered_surface_color(background, ..., ContentSection)`，表头已接入 `layered_surface_color(title_bar, ..., ContentSection)`；hover 仍使用 `list_active` / `list_hover`，选中态使用 `selection`。
  - 路径栏：本地 / 远程 path bar 已接入 `layered_surface_color(secondary, ..., ContentSection)`。
  - 拖拽：句柄使用 `drag_border`，drop overlay 改为 `drag_border + drop_target`，拖拽卡片改为 `layered_surface_color(background, ..., ContentCard)`。
  - 断连遮罩：浮层卡片已改为 `layered_surface_color(background, ..., ContentCard)`；外层与 loading scrim 已统一改为 `overlay` token，不再硬编码黑色遮罩。
- 结论：SFTP 已基本接入统一 layered surface + overlay 体系，本轮已经把 `Blocking / Loading` scrim 强度抽成 helper，剩余问题是继续向 Dialog / Sheet 等入口扩展。

## 差异总结：亮色 / 暗色 / 透明度 / 毛玻璃
- 亮色与暗色不只是换色值，`active_darken`、`group_box`、`tab`、`tab_active` 等 fallback 也不同。
- `enable_glass_effect` 关闭后窗口背景会回到 `Opaque`，但主题层仍执行 `plain_surface_tuning(...)`，所以页面并非全部恢复纯实色。
- 设置项文案已明确当前产品语义：
  - `glass_effect_desc`: “为应用窗口启用平台毛玻璃或材质效果，不影响透明度”
  - `glass_opacity_desc`: “独立调整主界面表面的透明程度，不受毛玻璃开关影响”
- `glass_opacity` 会先 clamp 到 `0.40~1.00`，再影响全局 glass token、Windows 分层 alpha、Dialog alpha、Terminal canvas alpha。
- Windows 与非 Windows 观感差异很大：同样的 `glass_opacity`，Windows 会因为分层乘系数而更“薄”。
- Home、Settings、Terminal 各自又维护了一层局部透明度公式：
  - Home：`glass_opacity + (level_ratio - 0.08)`
  - Settings：`glass_opacity + extra_offset`
  - Terminal：在终端主题上二次换算 `surface_opacity`

## 问题分级与建议动作

### A. 已修复（2026-04-18）
- `Dialog` 最低透明度实现与测试断言冲突。
  - 修复结果：已统一到“实现为准”的单一真值，content 最低 alpha 为 `0.70`，chrome/title/footer 按 `+0.20` 后 clamp。
  - 当前状态：相关单元测试已通过。

### B. 已部分修复，仍需继续收口
- 页面局部透明度公式已经分叉。
  - 当前状态：Home、Settings、Terminal 已迁移到统一 helper；DB / Mongo / Redis 侧栏、DB Objects toolbar、Table Designer、SFTP 的主要壳层和状态层已进一步收口，本轮已清掉已知 scrim / drag chip 常量。
  - 风险：跨页面统一调参仍需要继续向其余模块扩展。
  - 建议：下一步继续把 `overlay/scrim` helper 扩展到 Dialog / Sheet 等入口，并清理其余零散 opacity 常数。
- “glass 开关不影响透明度”已由产品文案明确，但此前缺少代码级护栏。
  - 当前状态：`plain_surface_tuning(...)`、Dialog、surface helper 已补测试。
  - 风险：主要从“语义漂移”降为“后续新页面未遵循该约束”。
  - 建议：后续新增页面或改造 DB/SFTP 时，继续沿用现有测试口径与 helper。

### C. 一致性债务
- 页面仍有较多硬编码透明度常数。
  - 现状：`0.10`、`0.14`、`0.18`、`0.28`、`0.35` 这类常数分散在多个页面。
  - 风险：后续做主题统一调优时需要逐页排查，维护成本高。
  - 建议：把这些常数收敛为具名层级 token 或 helper。
- Overlay / scrim 语义已开始统一分级。
  - 现状：`surface_alpha` 已新增 `OverlayScrimLevel::{Blocking, Loading}` 与 `overlay_scrim_color(...)`，SFTP 已接入。
  - 风险：Dialog / Sheet 等入口还未统一使用同一 helper，仍存在多入口分叉。
  - 建议：继续把 overlay helper 推广到全部遮罩层入口。

### 推荐修复顺序
1. 先解决 `Dialog` 实现 / 测试冲突，统一最低透明度真值。
2. 再收口 Home / Settings / Terminal 的局部 alpha 公式。
3. 为“glass 开关不影响透明度”的现有产品语义补测试护栏。
4. 最后再处理 SFTP 接入与页面硬编码常数清理。

## 具体修复方案清单

### 方案 1：修正 Dialog 最低透明度冲突
- 涉及文件：
  - `crates/ui/src/theme/glass.rs`
- 建议动作：
  - 当前已完成，以实现为准固定真值：
    - content 最低 alpha 为 `0.70`
    - chrome/title/footer 为 `content + 0.20` 后 clamp
  - 后续仅需保持测试与实现同步，不再重复引入第二套真值。
- 验证入口：
  - 运行 `cargo test -p gpui-component theme::glass -- --nocapture`

### 方案 2：为既有 glass 语义补测试护栏
- 涉及文件：
  - `main/locales/main.yml`
  - `crates/ui/src/theme/glass.rs`
  - `crates/ui/src/surface_alpha.rs`
  - `crates/ui/src/theme/mod.rs`
- 建议动作：
  - 以设置项文案为准，固定 `enable_glass_effect = false` 仅关闭平台毛玻璃 / 材质，不改变 `glass_opacity` 对页面表面透明度的控制权。
  - 为 `plain_surface_tuning(...)`、Dialog 与统一 `surface_alpha` helper 增加测试，防止后续把“材质开关”和“透明度控制”混淆。
- 验证入口：
  - 运行 `cargo test -p gpui-component theme::glass -- --nocapture`
  - 运行 `cargo test -p gpui-component surface_alpha -- --nocapture`

### 方案 3：统一页面层级透明度模型
- 涉及文件：
  - `main/src/home_tab.rs`
  - `main/src/setting_tab.rs`
  - `crates/terminal_view/src/view.rs`
  - 可新增统一 helper，例如 `crates/ui/src/theme/surface_levels.rs`
- 建议动作：
  - 抽象统一层级，例如 `Shell / Section / Card / Overlay / Sidebar / TerminalCanvas`。
  - Home 的 `macos_home_glass(...)`、Settings 的 `settings_glass(...)`、Terminal 的 `effective_terminal_theme(...)` 改为消费同一层级模型，而不是各自手写偏移值。
  - Windows 分层透明度也应挂到同一层级枚举上，而不是页面自己决定用什么偏移量。
- 验证入口：
  - 同一组 `glass_opacity` 下，比较 Home 卡片、Settings 分组、Terminal canvas 的实际 alpha 是否可解释且可预测。

### 方案 4：让 SFTP 明确接入或明确排除
- 涉及文件：
  - `crates/sftp_view/src/lib.rs`
  - `crates/sftp_view/src/file_list_panel.rs`
- 建议动作：
  - 二选一：
    - 接入统一玻璃体系：将主容器、列表头部、面板表面迁移到统一 surface helper。
    - 或在设计规范里明确 SFTP 为“实体面板页”，避免后续反复误判为遗漏。
- 验证入口：
  - 与 DB / Redis / Mongo 页面并排对比，看壳层、头部、列表 hover、选中态是否还存在明显风格断层。

### 方案 5：收敛硬编码透明度常数
- 涉及文件：
  - `main/src/home_tab.rs`
  - `main/src/settings/llm_providers_view.rs`
  - `crates/db_view/src/database_objects_tab.rs`
  - `crates/db_view/src/table_designer_tab.rs`
  - `crates/terminal_view/src/sidebar/server_monitor_panel.rs`
- 建议动作：
  - 把 `0.10`、`0.14`、`0.18`、`0.28`、`0.35` 这类页面常数提升为具名常量或统一 token。
  - 优先收敛“表面层级常数”，其次才是状态色透明度常数。
- 验证入口：
  - 调一次全局层级参数后，确认多个页面都能同步变化，而不是仍需逐页补丁。

## 最后结论
- 当前项目颜色系统已经形成完整链路：全局 token 定义稳定，但页面层的 alpha 策略还不完全统一。
- 透亮策略最复杂的页面组是：Home、Settings、Terminal、Database。
- Mongo 与 Redis 主要复用公共侧栏玻璃算法。
- SFTP 目前仍以实色面板为主。
- 如果后续要继续统一视觉治理，优先应收口的不是 token 数量，而是 3 套页面局部 alpha 公式：`macos_home_glass(...)`、`settings_glass(...)`、`effective_terminal_theme(...)`。

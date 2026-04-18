# OnetCli 配色与 Token 快速参考
更新时间：2026-04-18

> 这是一份简版文档，用于快速了解当前项目的页面配色结构、核心 token、透明度/毛玻璃规则，以及最近已经完成的收口工作。
>
> 详细版请看：
> - `docs/page-color-token-audit-2026-04-18.md`
> - `docs/plans/2026-04-18-surface-alpha-unify.md`

## 1. 一句话结论

项目当前的配色体系不是“页面各自写透明色”，而是：

`原始颜色 -> 语义 token fallback -> 主题 glass tuning -> 页面局部层级修正`

也就是说，大多数页面颜色最终都来自 `cx.theme().*`，局部页面只是在统一主题规则之上再做层级透明度计算。

## 2. 最常用的颜色 Token

高频基础 token：

- `background`
- `border`
- `foreground`
- `muted`
- `muted_foreground`
- `primary`
- `secondary`
- `sidebar`
- `list_active`
- `list_hover`
- `table_*`

高频状态/交互 token：

- `sidebar_accent`
- `list_active_border`
- `drag_border`
- `drop_target`
- `overlay`
- `danger`
- `warning`
- `success`

## 3. 透明度与毛玻璃的核心规则

- 用户设置入口是 `enable_glass_effect` 和 `glass_opacity`。
- `enable_glass_effect` 控制是否启用平台毛玻璃/材质。
- `glass_opacity` 独立控制页面表面的透明程度。
- 即使关闭毛玻璃，页面表面也不一定回到纯实色；仍会走 `plain_surface_tuning(...)`。
- Windows 额外存在分层透明度压缩，`ContentBase / ContentSection / ContentCard / TerminalFallback / TerminalCanvas` 的 alpha 会进一步被乘系数。

当前统一 helper 入口：

- `crates/ui/src/surface_alpha.rs`
- `sidebar_surface_color(...)`
- `sidebar_surface_color_with_offset(...)`
- `layered_surface_color(...)`
- `layered_level_surface_color(...)`
- `terminal_canvas_surface_opacity(...)`
- `overlay_scrim_color(...)`

## 4. 亮色 / 暗色 / 透明度差异

亮暗主题差异不只是色值不同，还包括 fallback 规则不同：

- `tab`：暗色更偏边框混合，亮色更偏 `secondary`
- `tab_active`：暗色偏 `secondary`，亮色偏 `background`
- `group_box`：亮暗模式下透明度策略不同
- `active_darken`：暗色更重，亮色更轻

透明度相关结论：

- 默认 surface opacity 是 `0.84`
- 用户设置会被 clamp 到 `0.40 ~ 1.00`
- Dialog 当前口径是：
  - content 最低 alpha = `0.70`
  - chrome/title/footer = `content + 0.20`

## 5. 页面级快速分布

### Home

- 主内容、工具栏、卡片使用统一层级公式
- 左侧边栏主要使用 `sidebar / list_active / sidebar_accent`
- 是当前“应用主题语义最完整”的页面之一

### Settings

- 左侧导航使用 `sidebar_surface_color(...)`
- 页面主体和分组层按 `background / group_box / secondary` 展开

### Terminal / SSH

- Terminal Canvas 有自己的一套透明度换算
- 右侧窄工具栏已经从 terminal palette 脱钩，改回应用主题侧栏语义
- 展开面板与子面板已避免用 `background` 把外层 surface 压黑

### DB / Mongo / Redis

- 侧栏已统一接入 `sidebar_surface_color(...)`
- DB Objects toolbar / input 已用带 offset 的 helper
- Table Designer 的表头、选中、hover、拖拽态已改回 `table_*` 语义 token

### SFTP

- 主壳层、搜索栏、路径栏、状态卡片已接入 layered surface helper
- scrim 已改为统一 `overlay` token + helper，不再写死黑色遮罩

## 6. 最近已经完成的收口

- Home / Settings / Terminal 的局部 alpha 公式已抽到统一 helper
- Dialog 实现与测试已统一
- DB / Mongo / Redis 侧栏已统一
- Table Designer 主要表层 token 已统一
- SFTP 的状态层、遮罩层已统一
- Terminal 右侧窄工具栏已回到应用主题语义，不再直接受 terminal palette 影响
- `overlay_scrim_color(...)` 已统一 `Blocking / Loading` 两档遮罩强度

## 7. 当前剩余问题

- 还有一些分散的透明度常数没有完全抽象
  - 主要在 import/export 强调态、终端监控图表、少量 hover 态
- `overlay/scrim` helper 目前只先覆盖了两档
  - Dialog / Sheet 等入口还没完全切到同一套 helper

## 8. 推荐阅读顺序

如果你只是想快速了解：

1. 先看这份简版文档
2. 再看 `docs/page-color-token-audit-2026-04-18.md`
3. 最后看 `crates/ui/src/surface_alpha.rs`

如果你准备继续改 UI / token：

1. 先看 `crates/ui/src/surface_alpha.rs`
2. 再看 `docs/plans/2026-04-18-surface-alpha-unify.md`
3. 最后按页面回查详细审计文档

# 项目配色方案 Token 分析报告

> 生成时间：2026-05-02
> 分析范围：`themes/`、`crates/ui/src/theme/`、`crates/ui/src/`、`main/src/`、`crates/core/src/`、`crates/one_ui/src/`

---

## 一、现有配色方案的所有 Token

### 1.1 Token 来源与层级

项目配色体系分为三层：

| 层级 | 位置 | 说明 |
|------|------|------|
| **主题 JSON 文件** | `themes/*.json` / `themes/*.jsonc` | 15 套外部主题配置 |
| **默认主题** | `crates/ui/src/theme/default-theme.json` | 内置 Light / Dark 两套默认主题 |
| **语义 Token 定义** | `crates/ui/src/theme/schema.rs` + `theme_color.rs` | 108 个颜色字段的 Rust 结构体 |
| **语义化常量** | `crates/ui/src/tokens/color/` | primitives / semantic / modes 三层抽象 |

### 1.2 完整 Token 清单（108 个）

按功能域分组：

#### 基础色板（Base Palette）— 12 个
| Token | JSON Key | 说明 |
|-------|----------|------|
| `red` | `base.red` | 基础红色 |
| `red_light` | `base.red.light` | 基础浅红色 |
| `green` | `base.green` | 基础绿色 |
| `green_light` | `base.green.light` | 基础浅绿色 |
| `blue` | `base.blue` | 基础蓝色 |
| `blue_light` | `base.blue.light` | 基础浅蓝色 |
| `yellow` | `base.yellow` | 基础黄色 |
| `yellow_light` | `base.yellow.light` | 基础浅黄色 |
| `magenta` | `base.magenta` | 基础品红色 |
| `magenta_light` | `base.magenta.light` | 基础浅品红色 |
| `cyan` | `base.cyan` | 基础青色 |
| `cyan_light` | `base.cyan.light` | 基础浅青色 |

#### 核心语义色 — 18 个
| Token | JSON Key | 说明 |
|-------|----------|------|
| `background` | `background` | 默认背景色 |
| `foreground` | `foreground` | 默认文字色 |
| `border` | `border` | 默认边框色 |
| `muted` | `muted.background` | 弱化背景（Skeleton、Switch） |
| `muted_foreground` | `muted.foreground` | 弱化文字（禁用态） |
| `primary` | `primary.background` | 主要按钮背景 |
| `primary_foreground` | `primary.foreground` | 主要按钮文字 |
| `primary_hover` | `primary.hover.background` | 主要按钮悬停背景 |
| `primary_active` | `primary.active.background` | 主要按钮激活背景 |
| `secondary` | `secondary.background` | 次要按钮背景 |
| `secondary_foreground` | `secondary.foreground` | 次要按钮文字 |
| `secondary_hover` | `secondary.hover.background` | 次要按钮悬停背景 |
| `secondary_active` | `secondary.active.background` | 次要按钮激活背景 |
| `accent` | `accent.background` | 强调背景（MenuItem、ListItem hover） |
| `accent_foreground` | `accent.foreground` | 强调文字 |
| `ring` | `ring` | 焦点环颜色 |
| `selection` | `selection.background` | 文本选中背景 |
| `overlay` | `overlay` | 遮罩层背景 |

#### 状态语义色 — 21 个
| Token | JSON Key | 说明 |
|-------|----------|------|
| `danger` | `danger.background` | 危险背景 |
| `danger_foreground` | `danger.foreground` | 危险文字 |
| `danger_hover` | `danger.hover.background` | 危险悬停 |
| `danger_active` | `danger.active.background` | 危险激活 |
| `warning` | `warning.background` | 警告背景 |
| `warning_foreground` | `warning.foreground` | 警告文字 |
| `warning_hover` | `warning.hover.background` | 警告悬停 |
| `warning_active` | `warning.active.background` | 警告激活 |
| `success` | `success.background` | 成功背景 |
| `success_foreground` | `success.foreground` | 成功文字 |
| `success_hover` | `success.hover.background` | 成功悬停 |
| `success_active` | `success.active.background` | 成功激活 |
| `info` | `info.background` | 信息背景 |
| `info_foreground` | `info.foreground` | 信息文字 |
| `info_hover` | `info.hover.background` | 信息悬停 |
| `info_active` | `info.active.background` | 信息激活 |
| `link` | `link` | 链接文字 |
| `link_active` | `link.active` | 链接激活 |
| `link_hover` | `link.hover` | 链接悬停 |
| `bullish` | `bullish.background` | K线上涨色 |
| `bearish` | `bearish.background` | K线下跌色 |

#### 容器/面板色 — 10 个
| Token | JSON Key | 说明 |
|-------|----------|------|
| `popover` | `popover.background` | 弹出框背景 |
| `popover_foreground` | `popover.foreground` | 弹出框文字 |
| `group` | `group.background` / `panel.background` | GroupBox / Panel 背景 |
| `group_foreground` | `group.foreground` | GroupBox 文字 |
| `sidebar` | `sidebar.background` | 侧边栏背景 |
| `sidebar_foreground` | `sidebar.foreground` | 侧边栏文字 |
| `sidebar_accent` | `sidebar.accent.background` | 侧边栏强调背景 |
| `sidebar_accent_foreground` | `sidebar.accent.foreground` | 侧边栏强调文字 |
| `sidebar_border` | `sidebar.border` | 侧边栏边框 |
| `sidebar_active_foreground` | `sidebar.active.foreground` | 侧边栏激活文字 |

#### 列表色 — 7 个
| Token | JSON Key | 说明 |
|-------|----------|------|
| `list` | `list.background` | 列表背景 |
| `list_active` | `list.active.background` | 列表项激活背景 |
| `list_active_border` | `list.active.border` | 列表项激活边框 |
| `list_hover` | `list.hover.background` | 列表项悬停背景 |
| `list_even` | `list.even.background` | 列表偶数行背景 |
| `list_head` | `list.head.background` | 列表头部背景 |

#### 表格色 — 9 个
| Token | JSON Key | 说明 |
|-------|----------|------|
| `table` | `table.background` | 表格背景 |
| `table_active` | `table.active.background` | 表格选中行背景 |
| `table_active_border` | `table.active.border` | 表格选中行边框 |
| `table_hover` | `table.hover.background` | 表格悬停行背景 |
| `table_even` | `table.even.background` | 表格偶数行背景 |
| `table_head` | `table.head.background` | 表格头部背景 |
| `table_head_foreground` | `table.head.foreground` | 表格头部文字 |
| `table_row_border` | `table.row.border` | 表格行边框 |

#### 标签页色 — 8 个
| Token | JSON Key | 说明 |
|-------|----------|------|
| `tab` | `tab.background` | 标签页背景 |
| `tab_active` | `tab.active.background` | 标签页激活背景 |
| `tab_foreground` | `tab.foreground` | 标签页文字 |
| `tab_active_foreground` | `tab.active.foreground` | 标签页激活文字 |
| `tab_hover` | `tab.hover.background` | 标签页悬停背景 |
| `tab_bar` | `tab_bar.background` | 标签栏背景 |
| `tab_bar_segmented` | `tab_bar.segmented.background` | 分段控件背景 |

#### 滚动条色 — 3 个
| Token | JSON Key | 说明 |
|-------|----------|------|
| `scrollbar` | `scrollbar.background` | 滚动条轨道背景 |
| `scrollbar_thumb` | `scrollbar.thumb.background` | 滚动条滑块背景 |
| `scrollbar_thumb_hover` | `scrollbar.thumb.hover.background` | 滚动条滑块悬停背景 |

#### 表单控件色 — 8 个
| Token | JSON Key | 说明 |
|-------|----------|------|
| `input` | `input.border` | 输入框边框 |
| `caret` | `caret` | 光标颜色 |
| `switch` | `switch.background` | 开关轨道背景 |
| `switch_thumb` | `switch.thumb.background` | 开关滑块背景 |
| `slider_bar` | `slider.background` | 滑块轨道背景 |
| `slider_thumb` | `slider.thumb.background` | 滑块手柄背景 |
| `progress_bar` | `progress.bar.background` | 进度条背景 |
| `skeleton` | `skeleton.background` | 骨架屏背景 |

#### 其他 — 7 个
| Token | JSON Key | 说明 |
|-------|----------|------|
| `title_bar` | `title_bar.background` | 标题栏背景 |
| `title_bar_border` | `title_bar.border` | 标题栏边框 |
| `tiles` | `tiles.background` | 磁贴/网格背景 |
| `window_border` | `window.border` | 窗口边框（仅 Linux） |
| `drag_border` | `drag.border` | 拖拽边框 |
| `drop_target` | `drop_target.background` | 拖放目标背景 |
| `description_list_label` | `description_list.label.background` | 描述列表标签背景 |
| `description_list_label_foreground` | `description_list.label.foreground` | 描述列表标签文字 |
| `accordion` | `accordion.background` | 手风琴背景 |
| `accordion_hover` | `accordion.hover.background` | 手风琴悬停背景 |

#### 图表色 — 5 个
| Token | JSON Key | 说明 |
|-------|----------|------|
| `chart_1` | `chart.1` | 图表颜色 1 |
| `chart_2` | `chart.2` | 图表颜色 2 |
| `chart_3` | `chart.3` | 图表颜色 3 |
| `chart_4` | `chart.4` | 图表颜色 4 |
| `chart_5` | `chart.5` | 图表颜色 5 |

---

## 二、各页面布局信息及配色详情

### 2.1 应用级布局（`main/src/onetcli_app.rs`）

```
┌─────────────────────────────────────────┐
│  TitleBar (title_bar + title_bar_border) │  ← 顶部状态栏
├──────────┬──────────────────────────────┤
│          │                              │
│ Sidebar  │     Content Area             │  ← 主内容区
│ (sidebar │     (background / border)    │
│ + border)│                              │
│          │                              │
├──────────┴──────────────────────────────┤
│  StatusBar (border + foreground)        │  ← 底部状态栏
└─────────────────────────────────────────┘
```

**使用 Token**：
- `background`：全局窗口背景
- `border`：分隔线、面板边框
- `foreground`：状态栏文字
- `muted_foreground`：次要状态信息
- `title_bar` / `title_bar_border`：标题栏
- `sidebar` / `sidebar_border` / `sidebar_foreground`：侧边栏
- `drag_border` / `drop_target`：拖拽交互视觉反馈

### 2.2 主页（`main/src/home_tab.rs` — HomePage）

```
┌─────────────────────────────────────────────┐
│  Toolbar (background + border)               │
├──────────┬──────────────────────────────────┤
│          │  Connection Cards                │
│ Workspace│  (list / list_even / list_hover) │
│ Filter   │                                  │
│ Panel    │  Drag Preview Overlay            │
│          │  (drop_target + drag_border)     │
│          │                                  │
│ (sidebar)│  ─────────────────────────────── │
│          │  Connection Form                 │
│          │  (popover / border)              │
└──────────┴──────────────────────────────────┘
```

**使用 Token**：
- `list` / `list_even` / `list_hover` / `list_active` / `list_active_border`：连接卡片列表
- `popover` / `popover_foreground`：新建连接弹窗
- `primary` / `primary_foreground`：主按钮
- `warning` / `danger` / `success`：状态指示
- `drop_target` / `drag_border`：拖拽放置视觉反馈

### 2.3 设置页（`main/src/setting_tab.rs` — SettingsPanel）

```
┌─────────────────────────────────────────────┐
│  Settings Categories (sidebar)               │
├──────────┬──────────────────────────────────┤
│  General │  Form Rows                        │
│  Proxy   │  (background + border)            │
│  Sync    │                                  │
│  LLM     │  Input Fields                     │
│  ...     │  (input + background)             │
│          │                                  │
│          │  Buttons                          │
│          │  (primary / secondary / danger)   │
└──────────┴──────────────────────────────────┘
```

**使用 Token**：
- `background` / `border`：表单容器
- `input`：输入框边框
- `muted_foreground`：标签/描述文字
- `primary` / `secondary` / `danger`：操作按钮
- `link`：超链接文字
- `success` / `warning`：验证状态指示

### 2.4 Tab 容器（`crates/core/src/tab_container.rs`）

```
┌─────────────────────────────────────────────┐
│  TabBar (tab_bar + tab_bar_segmented)        │
│  ├── Tab 1 (tab + tab_foreground)            │
│  ├── Tab 2 (tab_active + tab_active_fg) ◄───┤
│  └── Tab 3 (tab + tab_foreground)            │
├──────────────────────────────────────────────┤
│  Tab Content                                 │
│  (background)                                │
└──────────────────────────────────────────────┘
```

**使用 Token**：
- `tab_bar`：标签栏背景
- `tab_bar_segmented`：分段控件背景
- `tab` / `tab_foreground`：普通标签页
- `tab_active` / `tab_active_foreground`：激活标签页
- `tab_hover`：标签页悬停色（通过 `inactive_tab_hover_color` 间接使用）

---

## 三、各组件配色详情

### 3.1 Button（`crates/ui/src/button/button.rs`）

| 变体 | 背景 Token | 文字 Token | 边框 Token |
|------|-----------|-----------|-----------|
| `Primary` | `primary` | `primary_foreground` | `primary` |
| `Secondary` | `secondary` | `secondary_foreground` | `secondary` |
| `Danger` | `danger` | `danger_foreground` | `danger` |
| `Warning` | `warning` | `warning_foreground` | `warning` |
| `Success` | `success` | `success_foreground` | `success` |
| `Info` | `info` | `info_foreground` | `info` |
| `Ghost` | `transparent` | `secondary_foreground` | `transparent` |
| `Link` | `transparent` | `link` | `transparent` |
| `Outline` | `background` | 根据变体 | 根据变体 |

- Hover：`primary_hover` / `secondary_hover` / `danger_hover` 等
- Active：`primary_active` / `secondary_active` / `danger_active` 等
- Focus：`ring` 焦点环
- Disabled：`muted` 背景 + `muted_foreground` 文字

### 3.2 Input（`crates/ui/src/input/input.rs`）

| 元素 | Token |
|------|-------|
| 背景 | `input_background()`（内部方法，通常映射 `background`） |
| 边框 | `input` |
| 文字 | `foreground` |
| Placeholder | `muted_foreground` |
| 光标 | `caret` |
| 选中背景 | `selection` |
| Focus 边框 | `input` + `ring` |
| 禁用态 | `muted` + `muted_foreground` |

### 3.3 List（`crates/ui/src/list/list.rs`）

| 状态 | 背景 Token | 边框 Token | 文字 Token |
|------|-----------|-----------|-----------|
| 默认 | `list` | — | `foreground` |
| 偶数行 | `list_even` | — | `foreground` |
| 悬停 | `list_hover` | — | `foreground` |
| 激活 | `list_active` | `list_active_border` | `foreground` |
| 头部 | `list_head` | `border` | `muted_foreground` |

### 3.4 Table（`crates/ui/src/table/`）

| 元素 | Token |
|------|-------|
| 表格背景 | `table` |
| 头部背景 | `table_head` |
| 头部文字 | `table_head_foreground` |
| 偶数行 | `table_even` |
| 悬停行 | `table_hover` |
| 选中行 | `table_active` |
| 选中行边框 | `table_active_border` |
| 行分隔线 | `table_row_border` |

### 3.5 Sidebar（`crates/ui/src/sidebar/`）

| 元素 | Token |
|------|-------|
| 菜单项默认 | `sidebar_foreground` |
| 菜单项悬停 | `sidebar_accent` |
| 菜单项激活 | `list_active` + `list_active_border` + `sidebar_foreground` |
| 头部强调 | `sidebar_accent` + `sidebar_accent_foreground` |
| 底部强调 | `sidebar_accent` + `sidebar_accent_foreground` |

### 3.6 Tab（`crates/ui/src/tab/tab.rs` + `crates/ui/src/dock/`）

| 元素 | Token |
|------|-------|
| 普通标签 | `tab` + `tab_foreground` |
| 激活标签 | `tab_active` + `tab_active_foreground` |
| 标签栏 | `tab_bar` |
| 分段控件 | `tab_bar_segmented` |
| 悬停（Dock）| `tab_hover`（通过 `inactive_tab_hover_color` 间接使用） |

### 3.7 Tooltip（`crates/ui/src/tooltip.rs`）

| 元素 | Token |
|------|-------|
| 背景 | `popover` |
| 文字 | `popover_foreground` |
| 边框 | `border` |
| 描述文字 | `muted_foreground` |

### 3.8 Notification（`crates/ui/src/notification.rs`）

| 类型 | Icon Token | 背景 |
|------|-----------|------|
| Info | `info` | `popover` |
| Success | `success` | `popover` |
| Warning | `warning` | `popover` |
| Error | `danger` | `popover` |

### 3.9 Accordion（`crates/ui/src/accordion.rs`）

| 元素 | Token |
|------|-------|
| 背景 | `accordion` |
| 悬停 | `accordion_hover` |
| 边框 | `border` |
| 标题文字 | `foreground` |
| 描述文字 | `muted_foreground` |

### 3.10 Chart（图表组件）

| 元素 | Token |
|------|-------|
| 系列 1 | `chart_1` |
| 系列 2 | `chart_2` |
| 系列 3 | `chart_3` |
| 系列 4 | `chart_4` |
| 系列 5 | `chart_5` |
| 上涨 | `bullish` |
| 下跌 | `bearish` |

---

## 四、多余配色 Token 检查

### 4.1 判定标准

以 `cx.theme().<token>` 或 `theme.<token>` 在**业务渲染代码**（排除 `schema.rs`、`theme_color.rs`、`glass.rs`、`mapper.rs` 等元数据/映射文件）中的直接引用为依据。

### 4.2 疑似冗余 Token（7 个）

| Token | 主题配置中定义 | 代码中实际引用 | 备注 |
|-------|-------------|--------------|------|
| `chart_2` | ✅ | ❌ 无直接引用 | 主题 Story 映射表中有定义，但渲染代码中未使用 |
| `list_head` | ✅ | ❌ 无直接引用 | 仅在 `glass.rs` 毛玻璃调谐和 Story 映射中使用；List 头部实际使用 `list_head` 的代码路径为空（grep 无结果） |
| `sidebar_active_foreground` | ✅ | ❌ 无直接引用 | 仅在 Story 映射表中出现；Sidebar 菜单激活态实际使用 `sidebar_foreground` |

> **说明**：`chart_1`, `chart_3`, `chart_4`, `chart_5`, `tab_hover` 在 `crates/story/`、`crates/terminal_view/`、`crates/core/src/tab_container.rs` 中有实际引用，**不属于冗余**。

### 4.3 低使用频率 Token（≤2 次直接引用）

以下 Token 在代码中使用极少，可视情况评估是否保留：

| Token | 引用次数 | 使用位置 |
|-------|---------|---------|
| `accordion` | 1 | `crates/ui/src/accordion.rs` |
| `accordion_hover` | 1 | `crates/ui/src/accordion.rs` |
| `bearish` | 1 | `crates/ui/src/chart/candlestick_chart.rs` |
| `bullish` | 1 | `crates/ui/src/chart/candlestick_chart.rs` |
| `caret` | 2 | `crates/ui/src/input/input.rs` |
| `cyan` | 2 | `crates/ui/src/table/`（行指示器） |
| `cyan_light` | 1 | 仅主题配置，无代码引用 |
| `green_light` | 1 | 仅主题配置，无代码引用 |
| `yellow_light` | 1 | 仅主题配置，无代码引用 |
| `magenta` | 1 | 仅主题配置，无代码引用 |
| `magenta_light` | 1 | 仅主题配置，无代码引用 |
| `red_light` | 1 | 仅主题配置，无代码引用 |
| `blue_light` | 1 | 仅主题配置，无代码引用 |
| `description_list_label` | 2 | `crates/ui/src/description_list.rs` |
| `description_list_label_foreground` | 1 | `crates/ui/src/description_list.rs` |
| `group_foreground` | 1 | `crates/ui/src/` |
| `info_hover` | 1 | 仅主题配置，无代码引用 |
| `link_hover` | 1 | 仅主题配置，无代码引用 |
| `list_even` | 1 | `crates/ui/src/list/list.rs` |
| `overlay` | 1 | `crates/ui/src/` |
| `slider_bar` | 1 | `crates/ui/src/` |
| `slider_thumb` | 1 | `crates/ui/src/` |
| `success_hover` | 1 | 仅主题配置，无代码引用 |
| `switch` | 1 | `crates/ui/src/` |
| `tiles` | 1 | `crates/ui/src/dock/tiles.rs` |
| `window_border` | 2 | `crates/ui/src/` |

### 4.4 高频核心 Token（Top 20）

| 排名 | Token | 代码引用次数 | 核心程度 |
|------|-------|-------------|---------|
| 1 | `muted_foreground` | 161 | ⭐⭐⭐ 极高 |
| 2 | `border` | 104 | ⭐⭐⭐ 极高 |
| 3 | `foreground` | 49 | ⭐⭐⭐ 极高 |
| 4 | `background` | 41 | ⭐⭐⭐ 极高 |
| 5 | `primary` | 28 | ⭐⭐⭐ 极高 |
| 6 | `danger` | 22 | ⭐⭐⭐ 极高 |
| 7 | `success` | 21 | ⭐⭐⭐ 极高 |
| 8 | `warning` | 17 | ⭐⭐⭐ 极高 |
| 9 | `accent` | 16 | ⭐⭐⭐ 极高 |
| 10 | `muted` | 15 | ⭐⭐⭐ 极高 |
| 11 | `secondary` | 12 | ⭐⭐⭐ 极高 |
| 12 | `info` | 12 | ⭐⭐⭐ 极高 |
| 13 | `table_head` | 10 | ⭐⭐⭐ 极高 |
| 14 | `table_active` | 10 | ⭐⭐⭐ 极高 |
| 15 | `tab_foreground` | 10 | ⭐⭐⭐ 极高 |
| 16 | `selection` | 10 | ⭐⭐⭐ 极高 |
| 17 | `popover` | 10 | ⭐⭐⭐ 极高 |
| 18 | `input` | 10 | ⭐⭐⭐ 极高 |
| 19 | `list_active` | 8 | ⭐⭐ 高 |
| 20 | `link` | 8 | ⭐⭐ 高 |

---

## 五、主题文件分布

项目共包含 **15 套**外部主题文件：

| 主题文件 | 模式 | 颜色 Token 数（约） |
|----------|------|-------------------|
| `adventure.json` | dark | ~110 |
| `catppuccin.json` | dark | ~110 |
| `codium_dark.jsonc` | dark | ~110 |
| `dracula.json` | dark | ~110 |
| `flexoki.json` | light/dark | ~90 |
| `gruvbox.json` | dark | ~90 |
| `iceberg.json` | dark | ~110 |
| `matrix.json` | dark | ~90 |
| `night-owl.json` | dark | ~80 |
| `nord.json` | dark | ~80 |
| `onehalfdark.json` | dark | ~80 |
| `red-alert-deep.json` | dark | ~90 |
| `solarized.json` | dark | ~80 |
| `tomorrow-night.json` | dark | ~90 |

以及内置默认主题 `default-theme.json`（Light + Dark 双模式）。

---

## 六、建议

### 6.1 关于冗余 Token

1. **`chart_2`**：建议在图表 Story 或终端监控面板中补充使用，或从主题配置中移除。
2. **`list_head`**：List 组件头部目前直接使用了 `list`（无独立头部样式），建议确认是否需要独立的 `list_head` 样式；如不需要，可移除该 Token。
3. **`sidebar_active_foreground`**：Sidebar 激活态实际复用 `sidebar_foreground`，该 Token 无实际消费者，建议移除。

### 6.2 关于低使用 Token

- `cyan_light`, `green_light`, `yellow_light`, `magenta_light`, `red_light`, `blue_light` 等浅色基色当前无直接代码引用，仅作为 `chart_1~5` 的 fallback 计算基础。如果图表组件未来不依赖这些基色，可考虑精简。
- `info_hover`, `success_hover`, `link_hover` 等 hover 态 token 定义完善但代码中未直接引用（组件内部通常通过 `.opacity()` 或 blend 计算 hover 色），可视设计规范决定保留或移除。

### 6.3 关于语义化 Token 体系

项目已建立 `crates/ui/src/tokens/color/` 三层语义体系：
- `primitives.rs`：Dark/Light 原始 HSLA 常量
- `semantic.rs`：`SemanticColorsDark` / `SemanticColorsLight` 语义映射
- `modes.rs`：`ColorModes` trait + `DarkMode` / `LightMode` 实现

建议后续新页面/组件优先使用语义化 Token（如 `GLASS_BASE`、`BORDER_SUBTLE`、`TEXT_PRIMARY`），减少直接引用具体颜色名，提升主题可维护性。

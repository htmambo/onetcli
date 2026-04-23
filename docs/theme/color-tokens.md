# OnetCli 主题配色方案参考

> 本文档列出 OnetCli 主题系统中所有可用的配色 Key、用途及默认值。
> 主题文件为 JSON 格式，存放在项目根目录的 `themes/` 文件夹中。

---

## 1. 主题文件结构

```json
{
  "$schema": "https://github.com/longbridge/gpui-component/raw/refs/heads/main/.theme-schema.json",
  "name": "主题集名称",
  "author": "作者",
  "url": "主题来源URL",
  "themes": [
    {
      "name": "主题名称",
      "mode": "dark",
      "font.size": 16,
      "font.family": ".SystemUIFont",
      "mono_font.size": 13,
      "mono_font.family": "Menlo",
      "radius": 6,
      "radius.lg": 8,
      "shadow": true,
      "colors": {
        // 颜色配置见下文
      },
      "highlight": {
        // 代码高亮配置
      }
    }
  ]
}
```

---

## 2. 颜色 Key 完整参考

### 2.1 基础色 (Base)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `background` | `background` | 默认背景色 | — |
| `foreground` | `foreground` | 默认文字颜色 | — |
| `border` | `border` | 默认边框颜色 | — |
| `muted.background` | `muted` | 弱化背景（骨架屏、开关） | — |
| `muted.foreground` | `muted_foreground` | 弱化文字（禁用态） | `muted` 混合 `foreground` 70% |

### 2.2 强调色 (Accent)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `accent.background` | `accent` | 强调背景（菜单项、列表项 Hover） | `secondary` |
| `accent.foreground` | `accent_foreground` | 强调文字颜色 | `foreground` |

### 2.3 主色 (Primary)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `primary.background` | `primary` | 主按钮背景 | — |
| `primary.foreground` | `primary_foreground` | 主按钮文字 | `foreground` |
| `primary.hover.background` | `primary_hover` | 主按钮 Hover 背景 | `background` 混合 `primary` 90% |
| `primary.active.background` | `primary_active` | 主按钮按下背景 | `primary` 加深 10%/20% |

### 2.4 次色 (Secondary)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `secondary.background` | `secondary` | 次按钮背景 | — |
| `secondary.foreground` | `secondary_foreground` | 次按钮文字 | `foreground` |
| `secondary.hover.background` | `secondary_hover` | 次按钮 Hover 背景 | `background` 混合 `secondary` 90% |
| `secondary.active.background` | `secondary_active` | 次按钮按下背景 | `secondary` 加深 10%/20% |

### 2.5 状态色 — 成功 (Success)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `success.background` | `success` | 成功背景 | `base.green` |
| `success.foreground` | `success_foreground` | 成功文字 | `primary_foreground` |
| `success.hover.background` | `success_hover` | 成功 Hover 背景 | `background` 混合 `success` 90% |
| `success.active.background` | `success_active` | 成功按下背景 | `success` 加深 10%/20% |

### 2.6 状态色 — 信息 (Info)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `info.background` | `info` | 信息背景 | `base.cyan` |
| `info.foreground` | `info_foreground` | 信息文字 | `primary_foreground` |
| `info.hover.background` | `info_hover` | 信息 Hover 背景 | `background` 混合 `info` 90% |
| `info.active.background` | `info_active` | 信息按下背景 | `info` 加深 10%/20% |

### 2.7 状态色 — 警告 (Warning)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `warning.background` | `warning` | 警告背景 | `base.yellow` |
| `warning.foreground` | `warning_foreground` | 警告文字 | `primary_foreground` |
| `warning.hover.background` | `warning_hover` | 警告 Hover 背景 | `background` 混合 `warning` 90% |
| `warning.active.background` | `warning_active` | 警告按下背景 | `warning` 加深 10%/20% |

### 2.8 状态色 — 危险 (Danger)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `danger.background` | `danger` | 危险背景 | `base.red` |
| `danger.foreground` | `danger_foreground` | 危险文字 | `primary_foreground` |
| `danger.hover.background` | `danger_hover` | 危险 Hover 背景 | `background` 混合 `danger` 90% |
| `danger.active.background` | `danger_active` | 危险按下背景 | `danger` 加深 10%/20% |

### 2.9 侧边栏 (Sidebar)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `sidebar.background` | `sidebar` | 侧边栏背景 | `background` 混合 `border` 15% |
| `sidebar.foreground` | `sidebar_foreground` | 侧边栏文字 | `foreground` |
| `sidebar.accent.background` | `sidebar_accent` | 侧边栏强调背景（Hover） | `accent` |
| `sidebar.accent.foreground` | `sidebar_accent_foreground` | 侧边栏强调文字 | `accent_foreground` |
| `sidebar.border` | `sidebar_border` | 侧边栏边框 | `border` |
| `sidebar.primary.background` | `sidebar_primary` | 侧边栏主背景 | — |
| `sidebar.primary.foreground` | `sidebar_primary_foreground` | 侧边栏主文字 | — |

### 2.10 列表 (List)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `list.background` | `list` | 列表背景 | `background` |
| `list.active.background` | `list_active` | 列表选中项背景 | `background` 混合 `primary` 10% |
| `list.active.border` | `list_active_border` | 列表选中项边框 | `background` 混合 `primary` 60% |
| `list.hover.background` | `list_hover` | 列表 Hover 背景 | `accent` 60% 透明度 |
| `list.even.background` | `list_even` | 列表偶数行背景 | `list` |
| `list.head.background` | `list_head` | 列表表头背景 | `list` |

### 2.11 标签页 (Tab)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `tab.background` | `tab` | Tab 背景 | 暗色：`background` 混合 `border` 15%；亮色：`secondary` |
| `tab.active.background` | `tab_active` | 激活 Tab 背景 | 暗色：`secondary`；亮色：`background` |
| `tab.active.foreground` | `tab_active_foreground` | 激活 Tab 文字 | `foreground` |
| `tab.foreground` | `tab_foreground` | Tab 文字 | `foreground` |
| `tab_bar.background` | `tab_bar` | TabBar 背景 | `background` |
| `tab_bar.segmented.background` | `tab_bar_segmented` | TabBar 分段背景 | `secondary` |

### 2.12 表格 (Table)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `table.background` | `table` | 表格背景 | `list` |
| `table.active.background` | `table_active` | 表格选中项背景 | `list_active` |
| `table.active.border` | `table_active_border` | 表格选中项边框 | `list_active_border` |
| `table.even.background` | `table_even` | 表格偶数行背景 | `list_even` |
| `table.head.background` | `table_head` | 表格表头背景 | `list_head` |
| `table.head.foreground` | `table_head_foreground` | 表格表头文字 | `muted_foreground` |
| `table.hover.background` | `table_hover` | 表格 Hover 背景 | `list_hover` |
| `table.row.border` | `table_row_border` | 表格行边框 | `border` |

### 2.13 弹窗与浮层 (Popover / Overlay)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `popover.background` | `popover` | 浮层背景 | `background` |
| `popover.foreground` | `popover_foreground` | 浮层文字 | `foreground` |
| `overlay` | `overlay` | 遮罩层背景 | `rgba(0,0,0,0.4)` |

### 2.14 面板与分组 (Panel / Group)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `group.background` / `panel.background` | `group` | 分组面板背景 | 暗色：`background` 混合 `secondary` 30%；亮色：40% |
| `group.foreground` | `group_foreground` | 分组面板文字 | `foreground` |
| `tiles.background` | `tiles` | Tiles 背景 | `background` |

### 2.15 输入与表单 (Input / Form)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `input.border` | `input` | 输入框边框 | `border` |
| `caret` | `caret` | 输入框光标 | `primary` |
| `selection.background` | `selection` | 文本选中背景 | `primary` |
| `ring` | `ring` | 焦点环 | `base.blue` |

### 2.16 滚动条 (Scrollbar)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `scrollbar.background` | `scrollbar` | 滚动条轨道背景 | `background` |
| `scrollbar.thumb.background` | `scrollbar_thumb` | 滚动条滑块 | `accent` |
| `scrollbar.thumb.hover.background` | `scrollbar_thumb_hover` | 滚动条滑块 Hover | `scrollbar_thumb` |

### 2.17 标题栏 (TitleBar)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `title_bar.background` | `title_bar` | 标题栏背景 | `background` |
| `title_bar.border` | `title_bar_border` | 标题栏边框 | `border` |

### 2.18 拖拽与拖放 (Drag & Drop)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `drag.border` | `drag_border` | 拖拽边框 | `primary` 65% 透明度 |
| `drop_target.background` | `drop_target` | 拖放目标背景 | `primary` 20% 透明度 |

### 2.19 链接 (Link)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `link` | `link` | 链接颜色 | `primary` |
| `link.active` | `link_active` | 激活链接 | `link` |
| `link.hover` | `link_hover` | Hover 链接 | `link` |

### 2.20 手风琴与描述列表 (Accordion / Description List)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `accordion.background` | `accordion` | 手风琴背景 | `background` |
| `accordion.hover.background` | `accordion_hover` | 手风琴 Hover | `accent` 80% 透明度 |
| `description_list.label.background` | `description_list_label` | 描述列表标签背景 | `background` 混合 `border` 20% |
| `description_list.label.foreground` | `description_list_label_foreground` | 描述列表标签文字 | `muted_foreground` |

### 2.21 图表 (Chart)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `chart.1` | `chart_1` | 图表颜色 1 | `base.blue` 提亮 40% |
| `chart.2` | `chart_2` | 图表颜色 2 | `base.blue` 提亮 20% |
| `chart.3` | `chart_3` | 图表颜色 3 | `base.blue` |
| `chart.4` | `chart_4` | 图表颜色 4 | `base.blue` 加深 20% |
| `chart.5` | `chart_5` | 图表颜色 5 | `base.blue` 加深 40% |
| `bullish` | `bullish` | K线上涨色 | `base.green` |
| `bearish` | `bearish` | K线下跌色 | `base.red` |

### 2.22 滑块与开关 (Slider / Switch)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `slider.background` | `slider_bar` | 滑块轨道背景 | `primary` |
| `slider.thumb.background` | `slider_thumb` | 滑块 thumb | `primary_foreground` |
| `switch.background` | `switch` | 开关背景 | `secondary_active` |
| `switch.thumb.background` | `switch_thumb` | 开关 thumb | `background` |

### 2.23 骨架屏与进度条 (Skeleton / Progress)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `skeleton.background` | `skeleton` | 骨架屏背景 | `secondary` |
| `progress.bar.background` | `progress_bar` | 进度条背景 | `primary` |

### 2.24 窗口边框 (Window)

| JSON Key | Rust 字段 | 用途 | Fallback |
|---|---|---|---|
| `window.border` | `window_border` | 窗口边框（仅 Linux） | `border` |

### 2.25 基础色板 (Base Palette)

| JSON Key | Rust 字段 | 用途 |
|---|---|---|
| `base.red` | `red` | 基础红色 |
| `base.red.light` | `red_light` | 基础浅红色 |
| `base.green` | `green` | 基础绿色 |
| `base.green.light` | `green_light` | 基础浅绿色 |
| `base.blue` | `blue` | 基础蓝色 |
| `base.blue.light` | `blue_light` | 基础浅蓝色 |
| `base.yellow` | `yellow` | 基础黄色 |
| `base.yellow.light` | `yellow_light` | 基础浅黄色 |
| `base.magenta` | `magenta` | 基础品红色 |
| `base.magenta.light` | `magenta_light` | 基础浅品红色 |
| `base.cyan` | `cyan` | 基础青色 |
| `base.cyan.light` | `cyan_light` | 基础浅青色 |

> 浅色变体的 Fallback：`background` 混合对应基础色 80%。

---

## 3. 非颜色配置项

| JSON Key | Rust 字段 | 默认值 | 说明 |
|---|---|---|---|
| `font.size` | `font_size` | `16` | 基础字体大小 |
| `font.family` | `font_family` | `.SystemUIFont` | 基础字体 |
| `mono_font.size` | `mono_font_size` | `13` | 等宽字体大小 |
| `mono_font.family` | `mono_font_family` | 平台相关 | 等宽字体 |
| `radius` | `radius` | `6` | 通用圆角 |
| `radius.lg` | `radius_lg` | `8` | 大圆角（Dialog、Notification） |
| `shadow` | `shadow` | `true` | 是否启用阴影 |

---

## 4. 组件与 Key 的映射

### 4.1 数据库连接页面左侧树 (DbTreeView)

| 视觉效果 | 使用的 Key |
|---|---|
| 树整体背景 | `sidebar.background` |
| 普通节点文字 | `sidebar.foreground` |
| 选中节点背景 | `list.active.background`（经 `sidebar_surface_color` 处理） |
| 选中节点左侧指示条 | `list.active.border` |
| 选中节点文字 | `sidebar.foreground` |
| Hover 节点背景 | `sidebar.accent.background`（经 `sidebar_surface_color` 处理） |
| 文件夹类型节点文字 | `muted.foreground` |
| 展开/折叠箭头 | `muted.foreground` |
| 加载中 Spinner | `muted.foreground` |
| 节点加载失败图标 | `warning.background` |
| 搜索无结果提示 | `muted.foreground` |
| 搜索框顶部边框 | `border` |

### 4.2 Tab 标签页

| 视觉效果 | 使用的 Key |
|---|---|
| Tab 背景 | `tab.background` |
| 激活 Tab 背景 | `tab.active.background` |
| 激活 Tab 文字 | `tab.active.foreground` |
| 非激活 Tab 文字 | `tab.foreground` |
| TabBar 背景 | `tab_bar.background` |
| TabBar 分段背景 | `tab_bar.segmented.background` |

### 4.3 表格 (Table / Data Grid)

| 视觉效果 | 使用的 Key |
|---|---|
| 表格背景 | `table.background` |
| 选中行背景 | `table.active.background` |
| 选中行边框 | `table.active.border` |
| 偶数行背景 | `table.even.background` |
| 表头背景 | `table.head.background` |
| 表头文字 | `table.head.foreground` |
| Hover 行背景 | `table.hover.background` |
| 行边框 | `table.row.border` |

---

## 5. 快速创建主题

最小可运行的暗色主题示例：

```json
{
  "name": "MyTheme",
  "themes": [
    {
      "name": "MyTheme Dark",
      "mode": "dark",
      "colors": {
        "background": "#1e1e1e",
        "foreground": "#d4d4d4",
        "border": "#3c3c3c",
        "primary.background": "#0e639c",
        "primary.foreground": "#ffffff",
        "secondary.background": "#252526",
        "sidebar.background": "#252526",
        "sidebar.foreground": "#d4d4d4",
        "tab.active.background": "#1e1e1e",
        "tab.active.foreground": "#ffffff",
        "list.active.background": "#094771",
        "muted.foreground": "#858585"
      }
    }
  ]
}
```

---

## 6. 相关文件

| 文件 | 说明 |
|---|---|
| `.theme-schema.json` | JSON Schema，用于 IDE 自动补全和校验 |
| `crates/ui/src/theme/theme_color.rs` | Rust 颜色结构体定义 |
| `crates/ui/src/theme/schema.rs` | 主题反序列化与 Fallback 逻辑 |
| `crates/ui/src/theme/default-theme.json` | 内置默认主题 |
| `themes/*.json` | 用户可切换的主题文件 |

---
order: -5
---

# Design System

OnetCLI UI 设计系统，为应用程序提供统一的设计语言和组件规范。

## 概述

设计系统采用三层 Token 架构，确保设计决策的一致性和可维护性：

```
┌─────────────────────────────────────────┐
│  组件层 (Components)                    │
│  使用语义化 token 如 glass_surface      │
├─────────────────────────────────────────┤
│  语义层 (Semantic)                      │
│  表达用途的名称如 glass_surface → bg    │
├─────────────────────────────────────────┤
│  Token 层 (Primitives)                  │
│  原子化值如 #2d2d2d                    │
└─────────────────────────────────────────┘
```

## 设计 Token

设计 Token 位于 `crates/ui/src/tokens/` 模块。

### 间距 Token

基于 4px 基准网格的间距系统：

```rust
use gpui_component::tokens::Spacing;

// 使用方式
.h(px(Spacing::Unit4.value()))  // 16px
.gap(Spacing::Unit2.px())       // 8px
```

| Token | 值 | 用途 |
|-------|---|------|
| `Spacing::Unit1` | 4px | 紧凑间距 |
| `Spacing::Unit2` | 8px | 小间距 |
| `Spacing::Unit3` | 12px | 中等间距 |
| `Spacing::Unit4` | 16px | 标准间距 |
| `Spacing::Unit5` | 20px | 较宽间距 |
| `Spacing::Unit6` | 24px | 宽间距 |
| `Spacing::Unit8` | 32px | 大间距 |
| `Spacing::Unit10` | 40px | 极大间距 |
| `Spacing::Unit12` | 48px | 超大间距 |
| `Spacing::Unit16` | 64px | 页面级间距 |

### 圆角 Token

```rust
use gpui_component::tokens::Radius;

.rounded(Radius::Md.px())  // 6px — 输入框、下拉框
.rounded(Radius::Lg.px())  // 8px — 卡片、面板
.rounded(Radius::Xl.px())  // 12px — 模态框
.rounded(Radius::Sm.px())  // 4px — 小按钮、标签
.rounded(Radius::Full.px()) // 全圆角 — 头像、徽章
```

### 字体 Token

```rust
use gpui_component::tokens::TypographyTokens;

let typo = TypographyTokens::default();
cx.set_font_size(typo.font_size_md);   // 13px
cx.set_font_family(typo.font_family);  // 系统默认
```

### 阴影 Token

```rust
use gpui_component::tokens::ShadowToken;

let (blur, offset, spread, alpha) = ShadowToken::Md.values();
// blur=8px, offset_y=2px, spread=0, alpha=0.12
```

## 语义化颜色

语义化颜色通过 `app_style` 模块提供，推荐使用辅助函数而非直接访问 `ThemeColor`：

```rust
use gpui_component::app_style::*;

// 背景色
div().bg(glass_surface())     // 卡片/面板背景
div().bg(glass_sidebar())     // 侧边栏背景
div().bg(glass_base())        // 窗口基础背景

// 边框色
div().border_color(border_subtle())    // 细线分隔
div().border_color(border_default())   // 标准边框

// 文字色
div().text_color(text_primary())     // 主要文字
div().text_color(text_secondary())    // 次要文字

// 交互色
div().bg(interactive_primary())      // 主按钮背景
div().bg(interactive_hover())        // 悬停状态

// 区域色
div().bg(toolbar_bg())      // 工具栏背景
div().bg(tab_bar_bg())      // Tab 栏背景
div().bg(title_bar_bg())    // 标题栏背景
```

## 布局常量

布局常量位于 `crates/core/src/layout.rs`，用于统一面板和侧边栏尺寸：

| 常量 | 值 | 用途 |
|------|---|------|
| `SIDEBAR_DEFAULT_WIDTH` | 255px | Home 侧边栏默认宽度 |
| `SIDEBAR_MIN_WIDTH` | 200px | Home 侧边栏最小宽度 |
| `SIDEBAR_MAX_WIDTH` | 400px | Home 侧边栏最大宽度 |
| `TOOLBAR_WIDTH` | 36px | 工具栏高度（等于 TOOLBAR_HEIGHT） |
| `TREE_PANEL_DEFAULT_SIZE` | 250px | 数据库对象树默认宽度 |
| `TREE_PANEL_MIN_SIZE` | 150px | 数据库对象树最小宽度 |
| `TREE_PANEL_MAX_SIZE` | 500px | 数据库对象树最大宽度 |
| `CHAT_SIDEBAR_DEFAULT_WIDTH` | 360px | 聊天侧边栏默认宽度 |
| `CHAT_SIDEBAR_MIN_WIDTH` | 320px | 聊天侧边栏最小宽度 |
| `CHAT_SIDEBAR_MAX_WIDTH` | 600px | 聊天侧边栏最大宽度 |
| `TITLE_BAR_HEIGHT` | 38px | 标题栏高度 |
| `PANEL_MIN_SIZE` | 100px | 面板最小尺寸 |

使用方式：

```rust
use crate::layout::{SIDEBAR_DEFAULT_WIDTH, TOOLBAR_WIDTH};

div()
    .w(SIDEBAR_DEFAULT_WIDTH)
    .h(TOOLBAR_WIDTH)
```

## 组件开发规范

### 原则

1. **使用 Token 而非硬编码** — 所有样式值使用 token
2. **语义优先** — 使用语义化名称（如 `glass_surface`）而非具体值
3. **Theme 感知** — 通过 `cx.theme()` 访问颜色，使用 `ActiveTheme` trait

### 正确示例

```rust
use gpui_component::{
    ActiveTheme as _, app_style::*, Styled,
    tokens::{Spacing, Radius},
};

impl Render for MyComponent {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w_full()
            .p(Spacing::Unit4.px())           // ✅ 使用 token
            .bg(glass_surface())               // ✅ 语义化颜色
            .rounded(Radius::Lg.px())          // ✅ 语义化圆角
            .border_1()
            .border_color(border_subtle())     // ✅ 语义化边框
            .text_color(text_primary())        // ✅ 语义化文字
    }
}
```

### 错误示例

```rust
// ❌ 硬编码像素值
div().h(px(36.0)).w(px(255.0))

// ❌ 直接使用底层颜色
div().bg(Hsla::new(0.0, 0.0, 0.18, 0.84))

// ❌ 硬编码圆角
div().rounded(px(6.))
```

## 主题系统

### 主题模式

支持 Light/Dark 两种模式，通过 `ThemeMode` 访问：

```rust
use gpui_component::theme::ThemeMode;

if cx.theme().mode.is_dark() {
    // 深色模式处理
}
```

### 语义层

`Theme::semantic()` 方法提供语义化颜色访问：

```rust
let semantic = cx.theme().semantic();
div().bg(semantic.bg_surface());
```

## 毛玻璃效果

毛玻璃效果通过 `glass_*` 系列函数实现：

```rust
use gpui_component::app_style::*;

// 不同层的毛玻璃透明度递减
glass_base()      // 最底层，高透明度
glass_elevated()  // 浮层，中透明度
glass_surface()    // 卡片，低透明度
```

## 附录

### 文件结构

```
crates/ui/src/tokens/
├── mod.rs              # 模块入口
├── color/
│   ├── mod.rs
│   ├── primitives.rs   # 原子化颜色常量
│   ├── modes.rs       # 颜色模式 trait
│   └── semantic.rs     # 语义化颜色
├── spacing.rs          # 间距 token
├── typography.rs       # 字体 token
├── radius.rs          # 圆角 token
├── shadow.rs          # 阴影 token
├── motion.rs          # 动效 token
└── tests.rs           # 单元测试

crates/ui/src/theme/
└── semantic.rs         # Theme 语义层集成

crates/ui/src/app_style.rs   # 向后兼容辅助函数

crates/core/src/layout.rs    # 布局常量
```

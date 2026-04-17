# UI 设计系统 - 阶段一：设计 Token 落地实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **分支:** `feat/ui-design-system`
>
> **前置条件:** 此计划在 `feat/ui-design-system` 分支上执行，基于现有主题系统进行增量扩展。

**Goal:** 在 `crates/ui/src/tokens/` 下建立完整的设计 Token 系统（颜色原语、语义色、间距、字体、圆角、阴影、动效），通过语义层提供统一的设计值访问，同时保持对现有 `ThemeColor` 和 `app_style` 的向后兼容。

**Architecture:** 采用三层 Token 架构：
1. **Primitives 层** — 原子化原始值（HslColor 常量），不直接被组件使用
2. **Semantic 层** — 语义化命名，引用 Primitives，内部由 glass 系统调整 alpha
3. **App Style 层** — 向后兼容辅助函数，映射到现有 `ThemeColor` 字段

**Tech Stack:** Rust (GPUI), gpui::Hsla,现有 `crates/ui/src/theme/` 模块

---

## 文件结构

```
crates/ui/src/tokens/
├── mod.rs              # 统一导出所有 token
├── color/
│   ├── mod.rs
│   ├── primitives.rs   # 原子化颜色常量（dark/light 两套）
│   ├── semantic.rs     # 语义化颜色 token（引用 primitives）
│   └── modes.rs        # 语义色的 dark/light 模式变体
├── spacing.rs          # 间距 enum + px 值
├── typography.rs       # 字体 family/size/weight
├── radius.rs          # 圆角 enum + px 值
├── shadow.rs          # 阴影 token
└── motion.rs           # 动效时长/缓动函数

crates/ui/src/theme/semantic.rs   # 新增语义层，内部调用 tokens 模块
```

---

## Task 1: 创建 tokens 模块目录结构和基础框架

**Files:**
- Create: `crates/ui/src/tokens/mod.rs`
- Create: `crates/ui/src/tokens/color/mod.rs`
- Create: `crates/ui/src/tokens/color/primitives.rs`
- Create: `crates/ui/src/tokens/color/modes.rs`
- Create: `crates/ui/src/tokens/color/semantic.rs`
- Create: `crates/ui/src/tokens/spacing.rs`
- Create: `crates/ui/src/tokens/typography.rs`
- Create: `crates/ui/src/tokens/radius.rs`
- Create: `crates/ui/src/tokens/shadow.rs`
- Create: `crates/ui/src/tokens/motion.rs`
- Modify: `crates/ui/src/lib.rs` — 添加 `pub mod tokens;`

- [ ] **Step 1: 创建 tokens/mod.rs 入口文件**

```rust
//! Design Token 系统 — 原子化设计决策存储
//!
//! 架构分层：
//! - `color/` — 颜色 token（Primitives + Semantic）
//! - `spacing.rs` — 间距 token
//! - `typography.rs` — 字体 token
//! - `radius.rs` — 圆角 token
//! - `shadow.rs` — 阴影 token
//! - `motion.rs` — 动效 token

pub mod color;
pub mod spacing;
pub mod typography;
pub mod radius;
pub mod shadow;
pub mod motion;

pub use color::semantic::SemanticColors;
pub use spacing::Spacing;
pub use typography::TypographyTokens;
pub use radius::Radius;
pub use shadow::ShadowToken;
pub use motion::MotionTokens;
```

- [ ] **Step 2: 创建 color/primitives.rs — 原子化颜色常量**

```rust
//! 原子化颜色常量（Primitives）
//!
//! 仅作为语义 token 的基础值。不直接被组件使用。

use gpui::Hsla;

/// Dark 模式原始色值
pub mod dark {
    use super::*;

    pub const BG_BASE: Hsla = Hsla::new(0.0, 0.0, 0.10, 1.0);       // #1a1a1a
    pub const BG_ELEVATED: Hsla = Hsla::new(0.0, 0.0, 0.14, 1.0);   // #242424
    pub const BG_SURFACE: Hsla = Hsla::new(0.0, 0.0, 0.18, 1.0);    // #2d2d2d

    pub const BORDER_SUBTLE: Hsla = Hsla::new(0.0, 0.0, 0.23, 1.0);  // #3a3a3a
    pub const BORDER_DEFAULT: Hsla = Hsla::new(0.0, 0.0, 0.29, 1.0); // #4a4a4a

    pub const TEXT_PRIMARY: Hsla = Hsla::new(0.0, 0.0, 1.00, 1.0);   // #ffffff
    pub const TEXT_SECONDARY: Hsla = Hsla::new(0.0, 0.0, 0.63, 1.0); // #a0a0a0
    pub const TEXT_MUTED: Hsla = Hsla::new(0.0, 0.0, 0.44, 1.0);    // #707070

    pub const PRIMARY: Hsla = Hsla::new(0.60, 0.80, 0.55, 1.0);     // #3b82f6
    pub const ACCENT: Hsla = Hsla::new(0.75, 0.75, 0.90, 1.0);      // #8b5cf6
    pub const SUCCESS: Hsla = Hsla::new(0.35, 0.75, 0.50, 1.0);      // #22c55e
    pub const WARNING: Hsla = Hsla::new(0.08, 0.90, 0.60, 1.0);     // #f59e0b
    pub const DANGER: Hsla = Hsla::new(0.00, 0.80, 0.60, 1.0);       // #ef4444
    pub const INFO: Hsla = Hsla::new(0.50, 0.90, 0.55, 1.0);       // #06b6d4
}

/// Light 模式原始色值
pub mod light {
    use super::*;

    pub const BG_BASE: Hsla = Hsla::new(0.0, 0.0, 1.00, 1.0);       // #ffffff
    pub const BG_ELEVATED: Hsla = Hsla::new(0.0, 0.0, 0.96, 1.0);   // #f5f5f5
    pub const BG_SURFACE: Hsla = Hsla::new(0.0, 0.0, 0.92, 1.0);    // #ebebeb

    pub const BORDER_SUBTLE: Hsla = Hsla::new(0.0, 0.0, 0.88, 1.0);  // #e0e0e0
    pub const BORDER_DEFAULT: Hsla = Hsla::new(0.0, 0.0, 0.82, 1.0);  // #d0d0d0

    pub const TEXT_PRIMARY: Hsla = Hsla::new(0.0, 0.0, 0.10, 1.0);   // #1a1a1a
    pub const TEXT_SECONDARY: Hsla = Hsla::new(0.0, 0.0, 0.40, 1.0); // #666666
    pub const TEXT_MUTED: Hsla = Hsla::new(0.0, 0.0, 0.60, 1.0);    // #999999

    pub const PRIMARY: Hsla = Hsla::new(0.60, 0.75, 0.45, 1.0);     // #2563eb
    pub const ACCENT: Hsla = Hsla::new(0.75, 0.70, 0.55, 1.0);      // #7c3aed
    pub const SUCCESS: Hsla = Hsla::new(0.35, 0.70, 0.35, 1.0);     // #16a34a
    pub const WARNING: Hsla = Hsla::new(0.08, 0.85, 0.45, 1.0);     // #d97706
    pub const DANGER: Hsla = Hsla::new(0.00, 0.75, 0.50, 1.0);      // #dc2626
    pub const INFO: Hsla = Hsla::new(0.50, 0.85, 0.40, 1.0);       // #0891b2
}
```

- [ ] **Step 3: 创建 color/modes.rs — 语义色的 dark/light 变体**

```rust
//! 语义色的 dark/light 模式变体
//!
//! 每个语义 token 在 dark/light 模式下引用不同的 primitives 值。

use super::primitives::{self, dark, light};
use gpui::Hsla;

pub trait ColorModes {
    fn bg_base(&self) -> Hsla;
    fn bg_elevated(&self) -> Hsla;
    fn bg_surface(&self) -> Hsla;
    fn border_subtle(&self) -> Hsla;
    fn border_default(&self) -> Hsla;
    fn text_primary(&self) -> Hsla;
    fn text_secondary(&self) -> Hsla;
    fn text_muted(&self) -> Hsla;
    fn primary(&self) -> Hsla;
    fn accent(&self) -> Hsla;
    fn success(&self) -> Hsla;
    fn warning(&self) -> Hsla;
    fn danger(&self) -> Hsla;
    fn info(&self) -> Hsla;
}

impl ColorModes for light::Module {
    fn bg_base(&self) -> Hsla { light::BG_BASE }
    fn bg_elevated(&self) -> Hsla { light::BG_ELEVATED }
    fn bg_surface(&self) -> Hsla { light::BG_SURFACE }
    fn border_subtle(&self) -> Hsla { light::BORDER_SUBTLE }
    fn border_default(&self) -> Hsla { light::BORDER_DEFAULT }
    fn text_primary(&self) -> Hsla { light::TEXT_PRIMARY }
    fn text_secondary(&self) -> Hsla { light::TEXT_SECONDARY }
    fn text_muted(&self) -> Hsla { light::TEXT_MUTED }
    fn primary(&self) -> Hsla { light::PRIMARY }
    fn accent(&self) -> Hsla { light::ACCENT }
    fn success(&self) -> Hsla { light::SUCCESS }
    fn warning(&self) -> Hsla { light::WARNING }
    fn danger(&self) -> Hsla { light::DANGER }
    fn info(&self) -> Hsla { light::INFO }
}

impl ColorModes for dark::Module {
    fn bg_base(&self) -> Hsla { dark::BG_BASE }
    fn bg_elevated(&self) -> Hsla { dark::BG_ELEVATED }
    fn bg_surface(&self) -> Hsla { dark::BG_SURFACE }
    fn border_subtle(&self) -> Hsla { dark::BORDER_SUBTLE }
    fn border_default(&self) -> Hsla { dark::BORDER_DEFAULT }
    fn text_primary(&self) -> Hsla { dark::TEXT_PRIMARY }
    fn text_secondary(&self) -> Hsla { dark::TEXT_SECONDARY }
    fn text_muted(&self) -> Hsla { dark::TEXT_MUTED }
    fn primary(&self) -> Hsla { dark::PRIMARY }
    fn accent(&self) -> Hsla { dark::ACCENT }
    fn success(&self) -> Hsla { dark::SUCCESS }
    fn warning(&self) -> Hsla { dark::WARNING }
    fn danger(&self) -> Hsla { dark::DANGER }
    fn info(&self) -> Hsla { dark::INFO }
}
```

- [ ] **Step 4: 创建 color/semantic.rs — 语义化颜色 token**

```rust
//! 语义化颜色 token（Semantic Colors）
//!
//! 供组件使用。名称表达用途而非具体色值。
//! 内部由 glass 系统（apply_glass_tuning）调整 alpha 值。

use crate::theme::ThemeMode;
use super::primitives::{dark, light};

/// 语义化颜色结构体 — dark 模式
#[derive(Debug, Clone, Copy)]
pub struct SemanticColorsDark;

impl SemanticColorsDark {
    // === 背景层 ===
    pub const GLASS_BASE: Hsla = dark::BG_BASE;
    pub const GLASS_ELEVATED: Hsla = dark::BG_ELEVATED;
    pub const GLASS_CONTENT: Hsla = dark::BG_SURFACE;
    pub const GLASS_SURFACE: Hsla = dark::BG_SURFACE;
    pub const GLASS_SIDEBAR: Hsla = dark::BG_ELEVATED;

    // === 边框 ===
    pub const BORDER_SUBTLE: Hsla = dark::BORDER_SUBTLE;
    pub const BORDER_DEFAULT: Hsla = dark::BORDER_DEFAULT;
    pub const BORDER_STRONG: Hsla = dark::BORDER_DEFAULT;

    // === 文字 ===
    pub const TEXT_PRIMARY: Hsla = dark::TEXT_PRIMARY;
    pub const TEXT_SECONDARY: Hsla = dark::TEXT_SECONDARY;
    pub const TEXT_MUTED: Hsla = dark::TEXT_MUTED;
    pub const TEXT_LINK: Hsla = dark::PRIMARY;
    pub const TEXT_LINK_HOVER: Hsla = dark::ACCENT;

    // === 交互状态 ===
    pub const INTERACTIVE_PRIMARY: Hsla = dark::PRIMARY;
    pub const INTERACTIVE_HOVER: Hsla = dark::ACCENT;
    pub const INTERACTIVE_ACTIVE: Hsla = dark::ACCENT;
    pub const INTERACTIVE_DISABLED: Hsla = dark::TEXT_MUTED;

    // === 语义色 ===
    pub const SEMANTIC_SUCCESS: Hsla = dark::SUCCESS;
    pub const SEMANTIC_WARNING: Hsla = dark::WARNING;
    pub const SEMANTIC_DANGER: Hsla = dark::DANGER;
    pub const SEMANTIC_INFO: Hsla = dark::INFO;

    // === 特殊区域 ===
    pub const TITLE_BAR: Hsla = dark::BG_BASE;
    pub const TAB_BAR: Hsla = dark::BG_BASE;
    pub const TOOLBAR: Hsla = dark::BG_ELEVATED;
    pub const SCROLLBAR: Hsla = dark::BORDER_SUBTLE;
}

/// 语义化颜色结构体 — light 模式
#[derive(Debug, Clone, Copy)]
pub struct SemanticColorsLight;

impl SemanticColorsLight {
    pub const GLASS_BASE: Hsla = light::BG_BASE;
    pub const GLASS_ELEVATED: Hsla = light::BG_ELEVATED;
    pub const GLASS_CONTENT: Hsla = light::BG_SURFACE;
    pub const GLASS_SURFACE: Hsla = light::BG_SURFACE;
    pub const GLASS_SIDEBAR: Hsla = light::BG_ELEVATED;

    pub const BORDER_SUBTLE: Hsla = light::BORDER_SUBTLE;
    pub const BORDER_DEFAULT: Hsla = light::BORDER_DEFAULT;
    pub const BORDER_STRONG: Hsla = light::BORDER_DEFAULT;

    pub const TEXT_PRIMARY: Hsla = light::TEXT_PRIMARY;
    pub const TEXT_SECONDARY: Hsla = light::TEXT_SECONDARY;
    pub const TEXT_MUTED: Hsla = light::TEXT_MUTED;
    pub const TEXT_LINK: Hsla = light::PRIMARY;
    pub const TEXT_LINK_HOVER: Hsla = light::ACCENT;

    pub const INTERACTIVE_PRIMARY: Hsla = light::PRIMARY;
    pub const INTERACTIVE_HOVER: Hsla = light::ACCENT;
    pub const INTERACTIVE_ACTIVE: Hsla = light::ACCENT;
    pub const INTERACTIVE_DISABLED: Hsla = light::TEXT_MUTED;

    pub const SEMANTIC_SUCCESS: Hsla = light::SUCCESS;
    pub const SEMANTIC_WARNING: Hsla = light::WARNING;
    pub const SEMANTIC_DANGER: Hsla = light::DANGER;
    pub const SEMANTIC_INFO: Hsla = light::INFO;

    pub const TITLE_BAR: Hsla = light::BG_BASE;
    pub const TAB_BAR: Hsla = light::BG_BASE;
    pub const TOOLBAR: Hsla = light::BG_ELEVATED;
    pub const SCROLLBAR: Hsla = light::BORDER_SUBTLE;
}

/// 根据模式返回语义色
pub struct SemanticColors;

impl SemanticColors {
    pub fn get(mode: ThemeMode) -> impl SemanticColorsTrait {
        if mode.is_dark() {
            SemanticColorsDark
        } else {
            SemanticColorsLight
        }
    }
}
```

- [ ] **Step 5: 创建 spacing.rs — 间距 token**

```rust
//! 间距 Token
//!
//! 统一管理所有间距值。组件应使用此 enum 而非硬编码像素值。

use gpui::Pixels;

/// 间距 enum — 基于 4px 基准网格
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spacing {
    /// 4px
    Unit1,
    /// 8px
    Unit2,
    /// 12px
    Unit3,
    /// 16px
    Unit4,
    /// 20px
    Unit5,
    /// 24px
    Unit6,
    /// 32px
    Unit8,
    /// 40px
    Unit10,
    /// 48px
    Unit12,
    /// 64px
    Unit16,
}

impl Spacing {
    /// 转换为 Pixels
    pub fn px(self) -> Pixels {
        Pixels(self as f32 * 4.0)
    }

    /// 直接数值（用于需要 f32 的场景）
    pub fn value(self) -> f32 {
        self as f32 * 4.0
    }
}

// === 布局间距常量 ===

/// 面板间隙
pub const PANEL_GAP: f32 = 1.0;
/// 内容区块间隙
pub const SECTION_GAP: f32 = 16.0;
/// 页面边距
pub const PAGE_PADDING: f32 = 16.0;

/// 侧边栏宽度
pub const SIDEBAR_WIDTH: f32 = 255.0;
pub const SIDEBAR_MIN_WIDTH: f32 = 200.0;
pub const SIDEBAR_MAX_WIDTH: f32 = 400.0;

/// 数据库对象树宽度
pub const TREE_PANEL_WIDTH: f32 = 250.0;
pub const TREE_PANEL_MIN_WIDTH: f32 = 150.0;
pub const TREE_PANEL_MAX_WIDTH: f32 = 500.0;

/// 聊天侧边栏宽度
pub const CHAT_SIDEBAR_WIDTH: f32 = 360.0;
pub const CHAT_SIDEBAR_MIN_WIDTH: f32 = 320.0;
pub const CHAT_SIDEBAR_MAX_WIDTH: f32 = 600.0;

/// 工具栏高度
pub const TOOLBAR_HEIGHT: f32 = 36.0;
/// 工具栏按钮尺寸
pub const TOOLBAR_BUTTON_SIZE: f32 = 28.0;

/// 标题栏高度
pub const TITLE_BAR_HEIGHT: f32 = 38.0;

/// 面板最小尺寸
pub const PANEL_MIN_SIZE: f32 = 100.0;
```

- [ ] **Step 6: 创建 typography.rs — 字体 token**

```rust
//! 字体 Token
//!
//! 统一管理字体 family、size、weight。

use gpui::{Pixels, SharedString};

/// 字体 token 结构体
#[derive(Debug, Clone)]
pub struct TypographyTokens {
    pub font_family: SharedString,
    pub mono_font_family: SharedString,
    pub font_size_2xs: Pixels,
    pub font_size_xs: Pixels,
    pub font_size_sm: Pixels,
    pub font_size_md: Pixels,
    pub font_size_lg: Pixels,
    pub font_size_xl: Pixels,
    pub font_size_2xl: Pixels,
    pub line_height_tight: f32,
    pub line_height_normal: f32,
    pub line_height_relaxed: f32,
    pub font_weight_normal: u32,
    pub font_weight_medium: u32,
    pub font_weight_semibold: u32,
    pub font_weight_bold: u32,
}

impl Default for TypographyTokens {
    fn default() -> Self {
        Self {
            font_family: ".SystemUIFont".into(),
            mono_font_family: Self::default_mono(),
            font_size_2xs: Pixels(10.0),
            font_size_xs: Pixels(11.0),
            font_size_sm: Pixels(12.0),
            font_size_md: Pixels(13.0),
            font_size_lg: Pixels(14.0),
            font_size_xl: Pixels(16.0),
            font_size_2xl: Pixels(18.0),
            line_height_tight: 1.2,
            line_height_normal: 1.5,
            line_height_relaxed: 1.75,
            font_weight_normal: 400,
            font_weight_medium: 500,
            font_weight_semibold: 600,
            font_weight_bold: 700,
        }
    }
}

impl TypographyTokens {
    fn default_mono() -> SharedString {
        if cfg!(target_os = "macos") {
            "Menlo".into()
        } else if cfg!(target_os = "windows") {
            "Consolas".into()
        } else {
            "DejaVu Sans Mono".into()
        }
    }
}
```

- [ ] **Step 7: 创建 radius.rs — 圆角 token**

```rust
//! 圆角 Token
//!
//! 统一管理 border-radius 值。

use gpui::Pixels;

/// 圆角 enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radius {
    /// 0px — 无圆角
    None,
    /// 4px — 小按钮、标签
    Sm,
    /// 6px — 输入框、下拉框（默认）
    Md,
    /// 8px — 卡片、面板
    Lg,
    /// 12px — 模态框
    Xl,
    /// 全圆角 — 头像、徽章
    Full,
}

impl Radius {
    pub fn px(self) -> Pixels {
        Pixels(match self {
            Self::None => 0.0,
            Self::Sm => 4.0,
            Self::Md => 6.0,
            Self::Lg => 8.0,
            Self::Xl => 12.0,
            Self::Full => 9999.0,
        })
    }
}
```

- [ ] **Step 8: 创建 shadow.rs — 阴影 token**

```rust
//! 阴影 Token
//!
//! 统一管理阴影效果值。

use gpui::Pixels;

/// 阴影 token — 表示阴影级别的枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowToken {
    /// 无阴影
    None,
    /// 小阴影 — 用于轻微浮起效果
    Sm,
    /// 中阴影 — 用于卡片、面板
    Md,
    /// 大阴影 — 用于弹窗、浮层
    Lg,
    /// 超大阴影 — 用于模态框
    Xl,
}

impl ShadowToken {
    /// 返回 (blur, offset_y, spread, alpha) 元组
    pub fn values(self) -> (Pixels, Pixels, Pixels, f32) {
        match self {
            Self::None => (Pixels(0.0), Pixels(0.0), Pixels(0.0), 0.0),
            Self::Sm => (Pixels(4.0), Pixels(1.0), Pixels(0.0), 0.08),
            Self::Md => (Pixels(8.0), Pixels(2.0), Pixels(0.0), 0.12),
            Self::Lg => (Pixels(16.0), Pixels(4.0), Pixels(0.0), 0.16),
            Self::Xl => (Pixels(24.0), Pixels(8.0), Pixels(0.0), 0.20),
        }
    }
}
```

- [ ] **Step 9: 创建 motion.rs — 动效 token**

```rust
//! 动效 Token
//!
//! 统一管理动画时长和缓动函数。

use std::time::Duration;

/// 动效 token 结构体
#[derive(Debug, Clone, Copy)]
pub struct MotionTokens {
    /// 即时反馈 — 50ms
    pub duration_instant: Duration,
    /// 快速过渡 — 150ms
    pub duration_fast: Duration,
    /// 正常过渡 — 250ms
    pub duration_normal: Duration,
    /// 慢速过渡 — 400ms
    pub duration_slow: Duration,
}

impl Default for MotionTokens {
    fn default() -> Self {
        Self {
            duration_instant: Duration::from_millis(50),
            duration_fast: Duration::from_millis(150),
            duration_normal: Duration::from_millis(250),
            duration_slow: Duration::from_millis(400),
        }
    }
}

/// 缓动函数类型
#[derive(Debug, Clone, Copy)]
pub enum Easing {
    /// 默认 — ease-in-out
    Default,
    /// 减速曲线 — 缓入
    Decelerate,
    /// 加速曲线 — 缓出
    Accelerate,
}

impl Easing {
    /// 返回 cubic-bezier 参数 (x1, y1, x2, y2)
    pub fn bezier(self) -> (f64, f64, f64, f64) {
        match self {
            Self::Default => (0.4, 0.0, 0.2, 1.0),
            Self::Decelerate => (0.0, 0.0, 0.2, 1.0),
            Self::Accelerate => (0.4, 0.0, 1.0, 1.0),
        }
    }
}
```

- [ ] **Step 10: 创建 color/mod.rs — 导出 color 子模块**

```rust
//! 颜色 Token 子模块

pub mod modes;
pub mod primitives;
pub mod semantic;

pub use modes::ColorModes;
pub use semantic::{SemanticColors, SemanticColorsDark, SemanticColorsLight};
```

- [ ] **Step 11: 修改 crates/ui/src/lib.rs — 添加 tokens 模块导出**

在 `lib.rs` 中找到适当位置，添加：
```rust
pub mod tokens;
```

- [ ] **Step 12: 验证编译**

Run: `cargo build -p gpui-component`
Expected: 编译成功，无错误

- [ ] **Step 13: 提交**

```bash
git add crates/ui/src/tokens/ crates/ui/src/lib.rs
git commit -m "feat(tokens): 创建 tokens 模块基础框架

- 创建 tokens/color/ 颜色 token 子模块
  - primitives.rs: 原子化 dark/light 颜色常量
  - modes.rs: 颜色模式的 trait 实现
  - semantic.rs: 语义化颜色 token
- 创建 spacing.rs: 间距 token enum
- 创建 typography.rs: 字体 token
- 创建 radius.rs: 圆角 token
- 创建 shadow.rs: 阴影 token
- 创建 motion.rs: 动效 token

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 2: 创建 Theme 语义层集成 — 在 Theme 中添加 semantic_colors 字段

**Files:**
- Modify: `crates/ui/src/theme/mod.rs` — 在 Theme 结构体中添加 semantic_colors 字段
- Create: `crates/ui/src/theme/semantic.rs` — 语义层实现
- Modify: `crates/ui/src/theme/theme_color.rs` — 添加 DEFAULT_THEME_COLORS 中语义色字段的默认值

- [ ] **Step 1: 创建 crates/ui/src/theme/semantic.rs**

```rust
//! Theme 语义层 — 桥接设计 Token 和 ThemeColor
//!
//! 提供从 SemanticColors 到 ThemeColor 字段的映射，
//! 同时处理 glass 效果（alpha 调整）。

use crate::tokens::{
    color::semantic::{SemanticColorsDark, SemanticColorsLight},
    Spacing,
};
use crate::ThemeColor;
use crate::theme::{apply_glass_tuning, ThemeMode, DEFAULT_GLASS_OPACITY};

/// 从语义色构建 ThemeColor（不修改现有字段，仅新增语义化访问路径）
pub fn build_semantic_colors(mode: ThemeMode, blur_enabled: bool) -> ThemeColor {
    let mut colors = ThemeColor::default();

    // 背景层
    colors.background = if blur_enabled {
        adjust_glass_alpha(SemanticColorsDark::GLASS_BASE, mode)
    } else {
        SemanticColorsDark::GLASS_BASE
    };

    // 其余字段由 apply_glass_tuning 处理
    let mut color_seed = ThemeColor::default();
    // 填入基础 seed 值供 glass 系统使用
    apply_glass_tuning(&mut color_seed, mode, blur_enabled, DEFAULT_GLASS_OPACITY);

    // 使用 seed 中的 glass 调整后的值
    colors
}

fn adjust_glass_alpha(color: gpui::Hsla, mode: ThemeMode) -> gpui::Hsla {
    let alpha_offset = if mode.is_dark() { 0.84 } else { 0.95 };
    let mut c = color;
    c.a = alpha_offset;
    c
}
```

> **注意:** 此文件需要根据实际 `apply_glass_tuning` 函数签名调整。Step 1 仅为框架，具体实现在 Task 3 中完成。

- [ ] **Step 2: 在 Theme 结构体中添加 semantic_colors 方法**

修改 `crates/ui/src/theme/mod.rs`，在 `Theme` impl 块中添加：

```rust
impl Theme {
    // ... 现有方法 ...

    /// 获取语义化颜色（基于当前模式）
    pub fn semantic(&self) -> SemanticColorsRef {
        if self.mode.is_dark() {
            SemanticColorsRef::Dark(&SEMANTIC_DARK)
        } else {
            SemanticColorsRef::Light(&SEMANTIC_LIGHT)
        }
    }
}

/// 语义色引用类型
pub enum SemanticColorsRef<'a> {
    Dark(&'static SemanticColorsDark),
    Light(&'static SemanticColorsLight),
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo build -p gpui-component 2>&1`
Expected: 编译错误（预期），因为 `semantic.rs` 引用了尚未导入的模块

- [ ] **Step 4: 修复编译错误，调整实现**

根据实际编译错误调整 `crates/ui/src/theme/semantic.rs` 的实现，确保：
1. 正确导入 `apply_glass_tuning`
2. 与现有 `ThemeColor` 和 `ThemeMode` 类型兼容
3. 静态常量 `SEMANTIC_DARK` 和 `SEMANTIC_LIGHT` 正确导出

- [ ] **Step 5: 再次验证编译**

Run: `cargo build -p gpui-component`
Expected: 编译成功

- [ ] **Step 6: 提交**

```bash
git add crates/ui/src/theme/
git commit -m "feat(theme): 添加 Theme 语义层集成

- 创建 crates/ui/src/theme/semantic.rs 语义层实现
- 在 Theme 中添加 semantic() 方法，提供语义化颜色访问

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 3: 统一布局常量 — 替换 `crates/core/src/layout.rs` 中的硬编码值

**Files:**
- Modify: `crates/core/src/layout.rs` — 使用 tokens 模块的常量替换硬编码值
- Modify: `crates/ui/src/tokens/spacing.rs` — 确保布局常量与现有 layout.rs 保持一致
- Modify: 引用了 layout.rs 中硬编码值的文件

- [ ] **Step 1: 梳理 layout.rs 中所有硬编码像素值的使用位置**

Run: `grep -rn "px(255\|px(400\|px(250\|px(420\|px(36\|px(44\|px(38" crates/ --include="*.rs" | head -40`
Expected: 列出所有使用这些像素值的位置

- [ ] **Step 2: 修改 crates/core/src/layout.rs 使用 tokens 常量**

```rust
// 在 layout.rs 顶部添加导入
use gpui_component::tokens::spacing::{
    self,
    SIDEBAR_WIDTH, SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH,
    TOOLBAR_HEIGHT, TITLE_BAR_HEIGHT,
    TREE_PANEL_WIDTH, TREE_PANEL_MIN_WIDTH, TREE_PANEL_MAX_WIDTH,
    CHAT_SIDEBAR_WIDTH, CHAT_SIDEBAR_MIN_WIDTH, CHAT_SIDEBAR_MAX_WIDTH,
};

/// 侧边栏默认宽度
pub const SIDEBAR_DEFAULT_WIDTH: Pixels = px(SIDEBAR_WIDTH);
/// 侧边栏最小宽度
pub const SIDEBAR_MIN_WIDTH: Pixels = px(SIDEBAR_MIN_WIDTH);
/// 侧边栏最大宽度
pub const SIDEBAR_MAX_WIDTH: Pixels = px(SIDEBAR_MAX_WIDTH);
/// 工具栏宽度
pub const TOOLBAR_WIDTH: Pixels = px(TOOLBAR_HEIGHT);
```

> 注：保留原有 `SIDEBAR_*` 命名常量以保持 API 兼容性，但值来源于 tokens。

- [ ] **Step 3: 验证编译**

Run: `cargo build -p one-core && cargo build`
Expected: 编译成功

- [ ] **Step 4: 提交**

```bash
git add crates/core/src/layout.rs
git commit -m "refactor(layout): 统一布局常量使用 tokens 模块

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 4: 向后兼容层 — 扩展 app_style.rs 辅助函数

**Files:**
- Modify: `crates/ui/src/app_style.rs` — 添加新的语义化辅助函数，映射到现有 ThemeColor 字段

- [ ] **Step 1: 在 app_style.rs 中添加语义化辅助函数**

在现有函数之后添加：

```rust
// === 语义化背景色 ===

/// 毛玻璃基础层背景（窗口最底层）
pub fn glass_base() -> Hsla {
    active_theme().background
}

/// 毛玻璃浮层背景（Popover 等）
pub fn glass_elevated() -> Hsla {
    active_theme().popover
}

/// 内容区背景
pub fn glass_content() -> Hsla {
    active_theme().secondary
}

/// 卡片/面板背景
pub fn glass_surface() -> Hsla {
    active_theme().group_box
}

/// 侧边栏背景
pub fn glass_sidebar() -> Hsla {
    active_theme().sidebar
}

// === 语义化边框色 ===

/// 细线分隔边框
pub fn border_subtle() -> Hsla {
    active_theme().border
}

/// 标准边框
pub fn border_default() -> Hsla {
    active_theme().border
}

// === 语义化文字色 ===

/// 主要文字
pub fn text_primary() -> Hsla {
    active_theme().foreground
}

/// 次要文字
pub fn text_secondary() -> Hsla {
    active_theme().muted_foreground
}

// === 语义化交互色 ===

/// 主要交互色（按钮背景等）
pub fn interactive_primary() -> Hsla {
    active_theme().primary
}

pub fn interactive_hover() -> Hsla {
    active_theme().primary_hover
}

// === 语义化区域色 ===

/// 标题栏背景
pub fn title_bar_bg() -> Hsla {
    active_theme().title_bar
}

/// 工具栏背景
pub fn toolbar_bg() -> Hsla {
    active_theme().sidebar
}

/// Tab 栏背景
pub fn tab_bar_bg() -> Hsla {
    active_theme().tab_bar
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo build -p gpui-component`
Expected: 编译成功

- [ ] **Step 3: 提交**

```bash
git add crates/ui/src/app_style.rs
git commit -m "feat(app_style): 添加语义化辅助函数

- 添加 glass_* 系列: glass_base, glass_elevated, glass_content, glass_surface, glass_sidebar
- 添加 border_* 系列: border_subtle, border_default
- 添加 text_secondary
- 添加 interactive_* 系列: interactive_primary, interactive_hover
- 添加区域色: title_bar_bg, toolbar_bg, tab_bar_bg

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Task 5: 单元测试覆盖

**Files:**
- Modify: `crates/ui/src/tokens/color/primitives.rs` — 添加自测断言
- Create: `crates/ui/src/tokens/tests.rs` — Token 值单元测试

- [ ] **Step 1: 创建 crates/ui/src/tokens/tests.rs**

```rust
//! Token 系统单元测试

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spacing_values() {
        assert_eq!(Spacing::Unit1.px().0, 4.0);
        assert_eq!(Spacing::Unit2.px().0, 8.0);
        assert_eq!(Spacing::Unit3.px().0, 12.0);
        assert_eq!(Spacing::Unit4.px().0, 16.0);
        assert_eq!(Spacing::Unit8.px().0, 32.0);
    }

    #[test]
    fn test_radius_values() {
        assert_eq!(Radius::None.px().0, 0.0);
        assert_eq!(Radius::Sm.px().0, 4.0);
        assert_eq!(Radius::Md.px().0, 6.0);
        assert_eq!(Radius::Lg.px().0, 8.0);
        assert_eq!(Radius::Xl.px().0, 12.0);
    }

    #[test]
    fn test_shadow_token_values() {
        let (blur, _, _, _) = ShadowToken::None.values();
        assert_eq!(blur.0, 0.0);

        let (blur, offset, _, alpha) = ShadowToken::Md.values();
        assert_eq!(blur.0, 8.0);
        assert_eq!(offset.0, 2.0);
        assert!(alpha > 0.0);
    }

    #[test]
    fn test_motion_tokens_defaults() {
        let motion = MotionTokens::default();
        assert_eq!(motion.duration_instant, Duration::from_millis(50));
        assert_eq!(motion.duration_fast, Duration::from_millis(150));
        assert_eq!(motion.duration_normal, Duration::from_millis(250));
        assert_eq!(motion.duration_slow, Duration::from_millis(400));
    }

    #[test]
    fn test_easing_bezier() {
        let (x1, y1, x2, y2) = Easing::Default.bezier();
        assert!((x1 - 0.4).abs() < 0.01);
        assert!((y2 - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_typography_defaults() {
        let typo = TypographyTokens::default();
        assert_eq!(typo.font_size_md.0, 13.0);
        assert_eq!(typo.font_size_xs.0, 11.0);
        assert_eq!(typo.font_weight_normal, 400);
        assert_eq!(typo.font_weight_bold, 700);
    }

    #[test]
    fn test_primitives_dark_light_differ() {
        // 确保 dark 和 light 模式的颜色不同
        use crate::tokens::color::primitives::{dark, light};
        assert_ne!(dark::BG_BASE, light::BG_BASE);
        assert_ne!(dark::TEXT_PRIMARY, light::TEXT_PRIMARY);
        assert_ne!(dark::PRIMARY, light::PRIMARY);
    }

    #[test]
    fn test_layout_constants() {
        use crate::tokens::spacing::*;

        assert_eq!(SIDEBAR_WIDTH, 255.0);
        assert_eq!(SIDEBAR_MIN_WIDTH, 200.0);
        assert_eq!(TOOLBAR_HEIGHT, 36.0);
        assert_eq!(TITLE_BAR_HEIGHT, 38.0);
        assert_eq!(TREE_PANEL_WIDTH, 250.0);
        assert_eq!(CHAT_SIDEBAR_WIDTH, 360.0);
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p gpui-component tokens::tests --no-fail-fast`
Expected: 所有测试通过

- [ ] **Step 3: 提交**

```bash
git add crates/ui/src/tokens/tests.rs
git commit -m "test(tokens): 添加 Token 系统单元测试

- 测试 Spacing enum 值转换
- 测试 Radius enum 值转换
- 测试 ShadowToken 值
- 测试 MotionTokens 默认值
- 测试 Easing bezier 参数
- 测试 TypographyTokens 默认值
- 测试 dark/light 模式颜色差异
- 测试布局常量值

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## 实施总结

完成所有任务后，`crates/ui/src/tokens/` 模块将提供：

| 模块 | 内容 |
|------|------|
| `color/primitives.rs` | dark/light 原子化颜色常量 |
| `color/modes.rs` | 颜色模式的 trait 实现 |
| `color/semantic.rs` | 语义化颜色 token |
| `spacing.rs` | 间距 enum + 布局常量 |
| `typography.rs` | 字体 family/size/weight token |
| `radius.rs` | 圆角 enum |
| `shadow.rs` | 阴影 token enum |
| `motion.rs` | 动效时长/缓动函数 |

同时 `crates/ui/src/theme/semantic.rs` 提供 Theme 集成，`crates/ui/src/app_style.rs` 提供向后兼容的辅助函数。

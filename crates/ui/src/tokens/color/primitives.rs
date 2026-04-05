//! 原子化颜色常量（Primitives）
//!
//! 仅作为语义 token 的基础值。不直接被组件使用。

use gpui::Hsla;

/// Dark 模式原始色值
pub mod dark {
    use super::*;

    pub const BG_BASE: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.10, a: 1.0 };       // #1a1a1a
    pub const BG_ELEVATED: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.14, a: 1.0 };   // #242424
    pub const BG_SURFACE: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.18, a: 1.0 };    // #2d2d2d

    pub const BORDER_SUBTLE: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.23, a: 1.0 };  // #3a3a3a
    pub const BORDER_DEFAULT: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.29, a: 1.0 }; // #4a4a4a

    pub const TEXT_PRIMARY: Hsla = Hsla { h: 0.0, s: 0.0, l: 1.00, a: 1.0 };   // #ffffff
    pub const TEXT_SECONDARY: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.63, a: 1.0 }; // #a0a0a0
    pub const TEXT_MUTED: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.44, a: 1.0 };    // #707070

    pub const PRIMARY: Hsla = Hsla { h: 0.60, s: 0.80, l: 0.55, a: 1.0 };     // #3b82f6
    pub const ACCENT: Hsla = Hsla { h: 0.75, s: 0.75, l: 0.90, a: 1.0 };      // #8b5cf6
    pub const SUCCESS: Hsla = Hsla { h: 0.35, s: 0.75, l: 0.50, a: 1.0 };      // #22c55e
    pub const WARNING: Hsla = Hsla { h: 0.08, s: 0.90, l: 0.60, a: 1.0 };     // #f59e0b
    pub const DANGER: Hsla = Hsla { h: 0.00, s: 0.80, l: 0.60, a: 1.0 };       // #ef4444
    pub const INFO: Hsla = Hsla { h: 0.50, s: 0.90, l: 0.55, a: 1.0 };       // #06b6d4
}

/// Light 模式原始色值
pub mod light {
    use super::*;

    pub const BG_BASE: Hsla = Hsla { h: 0.0, s: 0.0, l: 1.00, a: 1.0 };       // #ffffff
    pub const BG_ELEVATED: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.96, a: 1.0 };   // #f5f5f5
    pub const BG_SURFACE: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.92, a: 1.0 };    // #ebebeb

    pub const BORDER_SUBTLE: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.88, a: 1.0 };  // #e0e0e0
    pub const BORDER_DEFAULT: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.82, a: 1.0 };  // #d0d0d0

    pub const TEXT_PRIMARY: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.10, a: 1.0 };   // #1a1a1a
    pub const TEXT_SECONDARY: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.40, a: 1.0 }; // #666666
    pub const TEXT_MUTED: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.60, a: 1.0 };    // #999999

    pub const PRIMARY: Hsla = Hsla { h: 0.60, s: 0.75, l: 0.45, a: 1.0 };     // #2563eb
    pub const ACCENT: Hsla = Hsla { h: 0.75, s: 0.70, l: 0.55, a: 1.0 };      // #7c3aed
    pub const SUCCESS: Hsla = Hsla { h: 0.35, s: 0.70, l: 0.35, a: 1.0 };     // #16a34a
    pub const WARNING: Hsla = Hsla { h: 0.08, s: 0.85, l: 0.45, a: 1.0 };     // #d97706
    pub const DANGER: Hsla = Hsla { h: 0.00, s: 0.75, l: 0.50, a: 1.0 };      // #dc2626
    pub const INFO: Hsla = Hsla { h: 0.50, s: 0.85, l: 0.40, a: 1.0 };       // #0891b2
}

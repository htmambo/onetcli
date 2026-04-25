//! Theme 语义层 — 桥接设计 Token 和 ThemeColor
//!
//! 提供从 ThemeColor 到语义化颜色的映射，
//! 让组件可以通过 `theme.semantic()` 获取语义化颜色。

use crate::{Colorize, ThemeColor};

/// 语义色引用类型 — 基于当前主题颜色返回对应的语义色
#[derive(Debug, Clone, Copy)]
pub enum SemanticColorsRef<'a> {
    Dark(&'a ThemeColor),
    Light(&'a ThemeColor),
}

impl<'a> SemanticColorsRef<'a> {
    pub fn bg_base(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.background,
        }
    }

    pub fn bg_elevated(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.popover,
        }
    }

    pub fn bg_surface(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.group,
        }
    }

    pub fn bg_sidebar(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.sidebar,
        }
    }

    pub fn border_subtle(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.border.mix(colors.background, 0.5),
        }
    }

    pub fn border_default(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.border,
        }
    }

    pub fn text_primary(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.foreground,
        }
    }

    pub fn text_secondary(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.muted_foreground,
        }
    }

    pub fn text_muted(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.muted_foreground.opacity(0.7),
        }
    }

    pub fn interactive_primary(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.primary,
        }
    }

    pub fn interactive_hover(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.primary_hover,
        }
    }

    pub fn semantic_success(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.success,
        }
    }

    pub fn semantic_warning(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.warning,
        }
    }

    pub fn semantic_danger(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.danger,
        }
    }

    pub fn semantic_info(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.info,
        }
    }

    pub fn title_bar(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.title_bar,
        }
    }

    pub fn tab_bar(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.tab_bar,
        }
    }

    pub fn toolbar(&self) -> gpui::Hsla {
        match self {
            Self::Dark(colors) | Self::Light(colors) => colors.sidebar,
        }
    }
}

//! Theme 语义层 — 桥接设计 Token 和 ThemeColor
//!
//! 提供从 SemanticColors 到 ThemeColor 字段的映射，
//! 让组件可以通过 `theme.semantic()` 获取语义化颜色。

use crate::tokens::color::semantic::{SemanticColorsDark, SemanticColorsLight};

/// 语义色引用类型 — 基于当前主题模式返回对应的语义色
#[derive(Debug, Clone, Copy)]
pub enum SemanticColorsRef<'a> {
    Dark(&'a SemanticColorsDark),
    Light(&'a SemanticColorsLight),
}

impl<'a> SemanticColorsRef<'a> {
    pub fn bg_base(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::GLASS_BASE,
            Self::Light(_) => SemanticColorsLight::GLASS_BASE,
        }
    }

    pub fn bg_elevated(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::GLASS_ELEVATED,
            Self::Light(_) => SemanticColorsLight::GLASS_ELEVATED,
        }
    }

    pub fn bg_surface(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::GLASS_SURFACE,
            Self::Light(_) => SemanticColorsLight::GLASS_SURFACE,
        }
    }

    pub fn bg_sidebar(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::GLASS_SIDEBAR,
            Self::Light(_) => SemanticColorsLight::GLASS_SIDEBAR,
        }
    }

    pub fn border_subtle(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::BORDER_SUBTLE,
            Self::Light(_) => SemanticColorsLight::BORDER_SUBTLE,
        }
    }

    pub fn border_default(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::BORDER_DEFAULT,
            Self::Light(_) => SemanticColorsLight::BORDER_DEFAULT,
        }
    }

    pub fn text_primary(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::TEXT_PRIMARY,
            Self::Light(_) => SemanticColorsLight::TEXT_PRIMARY,
        }
    }

    pub fn text_secondary(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::TEXT_SECONDARY,
            Self::Light(_) => SemanticColorsLight::TEXT_SECONDARY,
        }
    }

    pub fn text_muted(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::TEXT_MUTED,
            Self::Light(_) => SemanticColorsLight::TEXT_MUTED,
        }
    }

    pub fn interactive_primary(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::INTERACTIVE_PRIMARY,
            Self::Light(_) => SemanticColorsLight::INTERACTIVE_PRIMARY,
        }
    }

    pub fn interactive_hover(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::INTERACTIVE_HOVER,
            Self::Light(_) => SemanticColorsLight::INTERACTIVE_HOVER,
        }
    }

    pub fn semantic_success(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::SEMANTIC_SUCCESS,
            Self::Light(_) => SemanticColorsLight::SEMANTIC_SUCCESS,
        }
    }

    pub fn semantic_warning(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::SEMANTIC_WARNING,
            Self::Light(_) => SemanticColorsLight::SEMANTIC_WARNING,
        }
    }

    pub fn semantic_danger(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::SEMANTIC_DANGER,
            Self::Light(_) => SemanticColorsLight::SEMANTIC_DANGER,
        }
    }

    pub fn semantic_info(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::SEMANTIC_INFO,
            Self::Light(_) => SemanticColorsLight::SEMANTIC_INFO,
        }
    }

    pub fn title_bar(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::TITLE_BAR,
            Self::Light(_) => SemanticColorsLight::TITLE_BAR,
        }
    }

    pub fn tab_bar(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::TAB_BAR,
            Self::Light(_) => SemanticColorsLight::TAB_BAR,
        }
    }

    pub fn toolbar(&self) -> gpui::Hsla {
        match self {
            Self::Dark(_) => SemanticColorsDark::TOOLBAR,
            Self::Light(_) => SemanticColorsLight::TOOLBAR,
        }
    }
}

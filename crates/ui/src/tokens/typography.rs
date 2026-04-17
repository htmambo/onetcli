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
            font_size_2xs: Pixels::from(10.0),
            font_size_xs: Pixels::from(11.0),
            font_size_sm: Pixels::from(12.0),
            font_size_md: Pixels::from(13.0),
            font_size_lg: Pixels::from(14.0),
            font_size_xl: Pixels::from(16.0),
            font_size_2xl: Pixels::from(18.0),
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

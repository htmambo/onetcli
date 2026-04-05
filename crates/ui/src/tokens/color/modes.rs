//! 语义色的 dark/light 模式变体
//!
//! 每个语义 token 在 dark/light 模式下引用不同的 primitives 值。

use super::primitives::{dark, light};
use gpui::Hsla;

/// 颜色模式的 trait — 用于动态模式切换
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

/// Light 模式的颜色实现
pub struct LightMode;

impl ColorModes for LightMode {
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

/// Dark 模式的颜色实现
pub struct DarkMode;

impl ColorModes for DarkMode {
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

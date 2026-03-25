use gpui::{App, Hsla, rgb, rgba, white};
use gpui_component::button::{ButtonCustomVariant, ButtonVariant};

pub(crate) fn page_bg() -> Hsla {
    rgb(0x0a0a0a).into()
}

pub(crate) fn panel_bg() -> Hsla {
    rgb(0x12121a).into()
}

pub(crate) fn panel_alt_bg() -> Hsla {
    rgb(0x101114).into()
}

pub(crate) fn panel_hover_bg() -> Hsla {
    rgb(0x15171d).into()
}

pub(crate) fn border() -> Hsla {
    rgba(0xffffff14).into()
}

pub(crate) fn border_strong() -> Hsla {
    rgba(0xffffff1f).into()
}

pub(crate) fn text_primary() -> Hsla {
    rgb(0xeaeaf0).into()
}

pub(crate) fn text_muted() -> Hsla {
    rgb(0xb7b7c6).into()
}

pub(crate) fn text_soft() -> Hsla {
    rgb(0x6b7280).into()
}

pub(crate) fn accent() -> Hsla {
    rgb(0x00d9a3).into()
}

pub(crate) fn accent_hover() -> Hsla {
    rgb(0x10b981).into()
}

pub(crate) fn accent_active() -> Hsla {
    rgb(0x059669).into()
}

pub(crate) fn accent_dim() -> Hsla {
    accent().opacity(0.10)
}

pub(crate) fn accent_dim_strong() -> Hsla {
    accent().opacity(0.16)
}

pub(crate) fn danger() -> Hsla {
    rgb(0xef4444).into()
}

pub(crate) fn danger_dim() -> Hsla {
    danger().opacity(0.12)
}

pub(crate) fn primary_button_variant(cx: &App) -> ButtonVariant {
    ButtonVariant::Custom(
        ButtonCustomVariant::new(cx)
            .color(accent())
            .foreground(white())
            .border(accent().opacity(0.4))
            .hover(accent_hover())
            .active(accent_active())
            .shadow(true),
    )
}

pub(crate) fn secondary_button_variant(cx: &App) -> ButtonVariant {
    ButtonVariant::Custom(
        ButtonCustomVariant::new(cx)
            .color(panel_alt_bg())
            .foreground(text_primary())
            .border(border_strong())
            .hover(panel_hover_bg())
            .active(panel_bg()),
    )
}

pub(crate) fn danger_button_variant(cx: &App) -> ButtonVariant {
    ButtonVariant::Custom(
        ButtonCustomVariant::new(cx)
            .color(panel_alt_bg())
            .foreground(danger())
            .border(danger().opacity(0.35))
            .hover(danger_dim())
            .active(danger().opacity(0.18)),
    )
}

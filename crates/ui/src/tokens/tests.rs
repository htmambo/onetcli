//! Token 系统单元测试

#[cfg(test)]
mod tests {
    use crate::tokens::{
        motion::{Easing, MotionTokens},
        radius::Radius,
        shadow::ShadowToken,
        spacing::Spacing,
        typography::TypographyTokens,
    };

    #[test]
    fn test_spacing_values() {
        assert!((f32::from(Spacing::Unit1.px()) - 4.0).abs() < 0.001);
        assert!((f32::from(Spacing::Unit2.px()) - 8.0).abs() < 0.001);
        assert!((f32::from(Spacing::Unit3.px()) - 12.0).abs() < 0.001);
        assert!((f32::from(Spacing::Unit4.px()) - 16.0).abs() < 0.001);
        assert!((f32::from(Spacing::Unit8.px()) - 32.0).abs() < 0.001);
        assert!((f32::from(Spacing::Unit10.px()) - 40.0).abs() < 0.001);
    }

    #[test]
    fn test_spacing_values_as_f32() {
        assert!((Spacing::Unit1.value() - 4.0).abs() < 0.001);
        assert!((Spacing::Unit4.value() - 16.0).abs() < 0.001);
        assert!((Spacing::Unit16.value() - 64.0).abs() < 0.001);
    }

    #[test]
    fn test_radius_values() {
        assert!((f32::from(Radius::None.px()) - 0.0).abs() < 0.001);
        assert!((f32::from(Radius::Sm.px()) - 4.0).abs() < 0.001);
        assert!((f32::from(Radius::Md.px()) - 6.0).abs() < 0.001);
        assert!((f32::from(Radius::Lg.px()) - 8.0).abs() < 0.001);
        assert!((f32::from(Radius::Xl.px()) - 12.0).abs() < 0.001);
        // Full should be a very large number
        assert!(f32::from(Radius::Full.px()) > 1000.0);
    }

    #[test]
    fn test_shadow_token_values() {
        let (blur, offset, _spread, alpha) = ShadowToken::None.values();
        assert!((f32::from(blur) - 0.0).abs() < 0.001);

        let (blur, offset, _spread, _alpha) = ShadowToken::Sm.values();
        assert!((f32::from(blur) - 4.0).abs() < 0.001);
        assert!((f32::from(offset) - 1.0).abs() < 0.001);

        let (blur, offset, _spread, alpha) = ShadowToken::Md.values();
        assert!((f32::from(blur) - 8.0).abs() < 0.001);
        assert!((f32::from(offset) - 2.0).abs() < 0.001);
        assert!(alpha > 0.0);

        let (blur, _offset, _spread, _alpha) = ShadowToken::Lg.values();
        assert!((f32::from(blur) - 16.0).abs() < 0.001);

        let (blur, offset, _spread, _alpha) = ShadowToken::Xl.values();
        assert!((f32::from(blur) - 24.0).abs() < 0.001);
        assert!((f32::from(offset) - 8.0).abs() < 0.001);
    }

    #[test]
    fn test_motion_tokens_defaults() {
        let motion = MotionTokens::default();
        use std::time::Duration;
        assert_eq!(motion.duration_instant, Duration::from_millis(50));
        assert_eq!(motion.duration_fast, Duration::from_millis(150));
        assert_eq!(motion.duration_normal, Duration::from_millis(250));
        assert_eq!(motion.duration_slow, Duration::from_millis(400));
    }

    #[test]
    fn test_easing_bezier() {
        let (x1, y1, x2, y2) = Easing::Default.bezier();
        assert!((x1 - 0.4).abs() < 0.01);
        assert!((y1 - 0.0).abs() < 0.01);
        assert!((x2 - 0.2).abs() < 0.01);
        assert!((y2 - 1.0).abs() < 0.01);

        let (x1, _y1, x2, y2) = Easing::Decelerate.bezier();
        assert!((x1 - 0.0).abs() < 0.01);
        assert!((x2 - 0.2).abs() < 0.01);
        assert!((y2 - 1.0).abs() < 0.001);

        let (x1, y1, x2, _y2) = Easing::Accelerate.bezier();
        assert!((x1 - 0.4).abs() < 0.01);
        assert!((x2 - 1.0).abs() < 0.01);
        assert!((y1 - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_typography_defaults() {
        let typo = TypographyTokens::default();
        assert!((f32::from(typo.font_size_md) - 13.0).abs() < 0.001);
        assert!((f32::from(typo.font_size_xs) - 11.0).abs() < 0.001);
        assert!((f32::from(typo.font_size_2xs) - 10.0).abs() < 0.001);
        assert!((f32::from(typo.font_size_lg) - 14.0).abs() < 0.001);
        assert!((f32::from(typo.font_size_xl) - 16.0).abs() < 0.001);
        assert!((f32::from(typo.font_size_2xl) - 18.0).abs() < 0.001);
        assert_eq!(typo.font_weight_normal, 400);
        assert_eq!(typo.font_weight_bold, 700);
        assert_eq!(typo.font_weight_medium, 500);
        assert_eq!(typo.font_weight_semibold, 600);
    }

    #[test]
    fn test_primitives_dark_light_differ() {
        use crate::tokens::color::primitives::{dark, light};
        assert_ne!(dark::BG_BASE, light::BG_BASE);
        assert_ne!(dark::TEXT_PRIMARY, light::TEXT_PRIMARY);
        assert_ne!(dark::PRIMARY, light::PRIMARY);
        assert_ne!(dark::SUCCESS, light::SUCCESS);
    }

    #[test]
    fn test_layout_constants() {
        use crate::tokens::spacing::*;

        assert!((SIDEBAR_WIDTH - 255.0).abs() < 0.001);
        assert!((SIDEBAR_MIN_WIDTH - 200.0).abs() < 0.001);
        assert!((TOOLBAR_HEIGHT - 36.0).abs() < 0.001);
        assert!((TITLE_BAR_HEIGHT - 38.0).abs() < 0.001);
        assert!((TREE_PANEL_WIDTH - 250.0).abs() < 0.001);
        assert!((CHAT_SIDEBAR_WIDTH - 360.0).abs() < 0.001);
    }
}

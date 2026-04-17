use super::{DEFAULT_GLASS_OPACITY, WindowsSurfaceLayer, windows_surface_opacity};

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.0001,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn windows_blurred_surface_layers_are_thicker() {
    let opacity = DEFAULT_GLASS_OPACITY;

    if cfg!(target_os = "windows") {
        assert_close(
            windows_surface_opacity(opacity, true, WindowsSurfaceLayer::ContentBase),
            opacity * 0.68,
        );
        assert_close(
            windows_surface_opacity(opacity, true, WindowsSurfaceLayer::ContentSection),
            opacity * 0.48,
        );
        assert_close(
            windows_surface_opacity(opacity, true, WindowsSurfaceLayer::ContentCard),
            opacity * 0.34,
        );
        assert_close(
            windows_surface_opacity(opacity, true, WindowsSurfaceLayer::TerminalFallback),
            opacity * 0.36,
        );
        assert_close(
            windows_surface_opacity(opacity, true, WindowsSurfaceLayer::TerminalCanvas),
            opacity * 0.52,
        );
    } else {
        assert_close(
            windows_surface_opacity(opacity, true, WindowsSurfaceLayer::ContentBase),
            opacity,
        );
    }
}

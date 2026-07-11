use db::GlobalDbState;
use gpui::{
    App, AppContext, Bounds, Pixels, Size, WindowDecorations, WindowKind, WindowOptions, px, size,
};

use crate::setting_tab::AppSettings;

pub fn init_global_runtime_state(cx: &mut App) {
    let db_state = GlobalDbState::new();
    db_state.start_cleanup_task(cx);
    cx.set_global(db_state);

    db_view::init_ask_ai_notifier(cx);
}

pub fn main_window_options(cx: &App) -> WindowOptions {
    let requested_size = preferred_window_size(cx);
    let window_bounds = AppSettings::global(cx).restored_main_window_bounds(requested_size, cx);

    WindowOptions {
        window_bounds: Some(window_bounds),
        #[cfg(not(target_os = "linux"))]
        titlebar: Some(gpui_component::TitleBar::title_bar_options()),
        window_min_size: Some(Size {
            width: px(640.),
            height: px(480.),
        }),
        window_background: AppSettings::global(cx).preferred_window_background(),
        #[cfg(target_os = "linux")]
        app_id: Some("omnihub".to_string()),
        #[cfg(target_os = "linux")]
        window_decorations: Some(WindowDecorations::Client),
        kind: WindowKind::Normal,
        ..Default::default()
    }
}

fn preferred_window_size(cx: &App) -> gpui::Size<Pixels> {
    let mut window_size = size(px(1600.0), px(1200.0));
    if let Some(display) = cx.primary_display() {
        let display_size = display.visible_bounds().size;
        window_size.width = window_size.width.min(display_size.width * 0.85);
        window_size.height = window_size.height.min(display_size.height * 0.85);
    }
    window_size
}

#[allow(dead_code)]
fn _bounds_size(bounds: Bounds<Pixels>) -> gpui::Size<Pixels> {
    bounds.size
}

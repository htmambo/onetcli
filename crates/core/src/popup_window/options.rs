use gpui::{
    AnyWindowHandle, Bounds, Size, WindowBackgroundAppearance, WindowBounds, WindowKind,
    WindowOptions,
};
use gpui_component::TitleBar;

/// 弹窗窗口选项的构造参数。
pub(super) struct PopupWindowConfig {
    pub(super) kind: WindowKind,
    pub(super) bounds: Bounds<gpui::Pixels>,
    pub(super) min_size: Size<gpui::Pixels>,
    pub(super) window_background: WindowBackgroundAppearance,
    pub(super) parent_window: AnyWindowHandle,
}

/// 构造弹窗的窗口选项。
///
/// 父窗口必须显式传入：Linux（X11/Wayland）平台若依赖「键盘焦点窗口」推断父窗口，
/// 应用启动期主窗口尚未获得键盘焦点，弹窗会退化成独立顶层窗口并被主窗口覆盖。
pub(super) fn popup_window_options(config: PopupWindowConfig) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(config.bounds)),
        // Dialog windows on macOS are created as sheets (beginSheet) which
        // don't have standard window buttons — passing a titlebar with
        // traffic_light_position causes a nil pointer dereference in
        // MacWindowState::move_traffic_light.
        titlebar: if matches!(config.kind, WindowKind::Dialog) {
            None
        } else {
            Some(TitleBar::title_bar_options())
        },
        window_min_size: Some(config.min_size),
        kind: config.kind,
        window_background: config.window_background,
        #[cfg(target_os = "linux")]
        window_decorations: Some(gpui::WindowDecorations::Client),
        parent_window: Some(config.parent_window),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    /// 回归守护：弹窗必须携带显式父窗口。
    ///
    /// Linux 平台若依赖「键盘焦点窗口」推断父窗口，应用启动期主窗口尚未获得
    /// 键盘焦点，弹窗会退化成独立顶层窗口并被主窗口覆盖。
    #[gpui::test]
    fn popup_window_options_carry_explicit_parent_window(cx: &mut gpui::TestAppContext) {
        let parent: AnyWindowHandle = cx.add_window(|_, _| gpui::Empty).into();

        let options = popup_window_options(PopupWindowConfig {
            kind: WindowKind::Dialog,
            bounds: Bounds::centered_at(point(px(0.0), px(0.0)), size(px(100.0), px(100.0))),
            min_size: size(px(10.0), px(10.0)),
            window_background: WindowBackgroundAppearance::Opaque,
            parent_window: parent,
        });

        assert_eq!(options.parent_window, Some(parent));
    }

    /// 回归守护：Dialog 不携带标题栏。
    ///
    /// macOS 的 Dialog 以 sheet 形式挂载，带 `traffic_light_position` 的标题栏会触发
    /// `MacWindowState::move_traffic_light` 空指针。
    #[gpui::test]
    fn popup_window_options_dialog_has_no_titlebar(cx: &mut gpui::TestAppContext) {
        let parent: AnyWindowHandle = cx.add_window(|_, _| gpui::Empty).into();

        let options = popup_window_options(PopupWindowConfig {
            kind: WindowKind::Dialog,
            bounds: Bounds::centered_at(point(px(0.0), px(0.0)), size(px(100.0), px(100.0))),
            min_size: size(px(10.0), px(10.0)),
            window_background: WindowBackgroundAppearance::Opaque,
            parent_window: parent,
        });

        assert!(options.titlebar.is_none());
        assert_eq!(options.kind, WindowKind::Dialog);
    }
}

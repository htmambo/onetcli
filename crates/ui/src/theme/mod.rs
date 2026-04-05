use crate::{
    highlighter::HighlightTheme, list::ListSettings, notification::NotificationSettings,
    scroll::ScrollbarShow, sheet::SheetSettings, tokens::color::semantic::{SemanticColorsDark, SemanticColorsLight},
};
use gpui::{App, Global, Hsla, Pixels, SharedString, Window, WindowAppearance, px};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::Arc,
};

mod color;
mod glass;
mod registry;
mod schema;
mod semantic;
mod theme_color;

pub use color::*;
pub(crate) use glass::{apply_glass_highlight_tuning, apply_glass_tuning};
pub use registry::*;
pub use schema::*;
pub use semantic::SemanticColorsRef;
pub use theme_color::*;

pub const DEFAULT_GLASS_OPACITY: f32 = 0.84;
pub const MIN_GLASS_OPACITY: f32 = 0.40;
pub const MAX_GLASS_OPACITY: f32 = 1.00;
/// 左侧面板毛玻璃透明度偏移量
pub const LEFT_PANEL_ALPHA_OFFSET: f32 = 0.20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsSurfaceLayer {
    ContentBase,
    ContentSection,
    ContentCard,
    TerminalFallback,
    TerminalCanvas,
}

pub fn windows_surface_opacity(
    opacity: f32,
    blur_enabled: bool,
    layer: WindowsSurfaceLayer,
) -> f32 {
    let opacity = clamp_surface_opacity(opacity as f64);
    if !cfg!(target_os = "windows") {
        return opacity;
    }

    let factor = match (blur_enabled, layer) {
        (true, WindowsSurfaceLayer::ContentBase) => 0.58,
        (true, WindowsSurfaceLayer::ContentSection) => 0.38,
        (true, WindowsSurfaceLayer::ContentCard) => 0.24,
        (true, WindowsSurfaceLayer::TerminalFallback) => 0.26,
        (true, WindowsSurfaceLayer::TerminalCanvas) => 0.42,
        (false, WindowsSurfaceLayer::ContentBase) => 0.45,
        (false, WindowsSurfaceLayer::ContentSection) => 0.24,
        (false, WindowsSurfaceLayer::ContentCard) => 0.14,
        (false, WindowsSurfaceLayer::TerminalFallback) => 0.16,
        (false, WindowsSurfaceLayer::TerminalCanvas) => 0.34,
    };

    (opacity * factor).clamp(0.0, 1.0)
}

pub fn windows_surface_color(
    mut color: Hsla,
    blur_enabled: bool,
    opacity: f32,
    layer: WindowsSurfaceLayer,
) -> Hsla {
    if cfg!(target_os = "windows") {
        color.a = windows_surface_opacity(opacity, blur_enabled, layer);
    }

    color
}

pub fn init(cx: &mut App) {
    registry::init(cx);

    Theme::sync_system_appearance(None, cx);
    Theme::sync_scrollbar_appearance(cx);
}

pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    #[inline(always)]
    fn theme(&self) -> &Theme {
        Theme::global(self)
    }
}

/// The global theme configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Theme {
    pub colors: ThemeColor,
    pub highlight_theme: Arc<HighlightTheme>,
    pub light_theme: Rc<ThemeConfig>,
    pub dark_theme: Rc<ThemeConfig>,

    pub mode: ThemeMode,
    /// The font family for the application, default is `.SystemUIFont`.
    pub font_family: SharedString,
    /// The base font size for the application, default is 16px.
    pub font_size: Pixels,
    /// The monospace font family for the application.
    ///
    /// Defaults to:
    ///
    /// - macOS: `Menlo`
    /// - Windows: `Consolas`
    /// - Linux: `DejaVu Sans Mono`
    pub mono_font_family: SharedString,
    /// The monospace font size for the application, default is 13px.
    pub mono_font_size: Pixels,
    /// Radius for the general elements.
    pub radius: Pixels,
    /// Radius for the large elements, e.g.: Dialog, Notification border radius.
    pub radius_lg: Pixels,
    pub shadow: bool,
    pub transparent: Hsla,
    pub window_blur_enabled: bool,
    pub surface_opacity: f32,
    /// Show the scrollbar mode, default: Scrolling
    pub scrollbar_show: ScrollbarShow,
    /// The notification setting.
    pub notification: NotificationSettings,
    /// Tile grid size, default is 4px.
    pub tile_grid_size: Pixels,
    /// The shadow of the tile panel.
    pub tile_shadow: bool,
    /// The border radius of the tile panel, default is 0px.
    pub tile_radius: Pixels,
    /// The list settings.
    pub list: ListSettings,
    /// The sheet settings.
    pub sheet: SheetSettings,
}

/// Dark 模式语义色静态实例
static SEMANTIC_DARK: SemanticColorsDark = SemanticColorsDark;

/// Light 模式语义色静态实例
static SEMANTIC_LIGHT: SemanticColorsLight = SemanticColorsLight;

impl Default for Theme {
    fn default() -> Self {
        Self::from(&ThemeColor::default())
    }
}

impl Deref for Theme {
    type Target = ThemeColor;

    fn deref(&self) -> &Self::Target {
        &self.colors
    }
}

impl DerefMut for Theme {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.colors
    }
}

impl Global for Theme {}

impl Theme {
    /// Returns the global theme reference
    #[inline(always)]
    pub fn global(cx: &App) -> &Theme {
        cx.global::<Theme>()
    }

    /// Returns the global theme mutable reference
    #[inline(always)]
    pub fn global_mut(cx: &mut App) -> &mut Theme {
        cx.global_mut::<Theme>()
    }

    /// Returns true if the theme is dark.
    #[inline(always)]
    pub fn is_dark(&self) -> bool {
        self.mode.is_dark()
    }

    /// Returns the current theme name.
    pub fn theme_name(&self) -> &SharedString {
        if self.is_dark() {
            &self.dark_theme.name
        } else {
            &self.light_theme.name
        }
    }

    /// Sync the theme with the system appearance
    pub fn sync_system_appearance(window: Option<&mut Window>, cx: &mut App) {
        // Better use window.appearance() for avoid error on Linux.
        // https://github.com/longbridge/gpui-component/issues/104
        let appearance = window
            .as_ref()
            .map(|window| window.appearance())
            .unwrap_or_else(|| cx.window_appearance());

        Self::change(appearance, window, cx);
    }

    /// Sync the Scrollbar showing behavior with the system
    pub fn sync_scrollbar_appearance(cx: &mut App) {
        Theme::global_mut(cx).scrollbar_show = if cx.should_auto_hide_scrollbars() {
            ScrollbarShow::Scrolling
        } else {
            ScrollbarShow::Hover
        };
    }

    /// Change the theme mode.
    pub fn change(mode: impl Into<ThemeMode>, window: Option<&mut Window>, cx: &mut App) {
        let mode = mode.into();
        Self::ensure_global(cx);

        let theme = cx.global_mut::<Theme>();
        theme.mode = mode;
        if mode.is_dark() {
            theme.apply_config(&theme.dark_theme.clone());
        } else {
            theme.apply_config(&theme.light_theme.clone());
        }
        crate::app_style::sync_theme(theme);

        if let Some(window) = window {
            window.refresh();
        }
    }

    pub fn set_window_surface_preferences(blur_enabled: bool, opacity: f64, cx: &mut App) {
        Self::ensure_global(cx);

        let theme = cx.global_mut::<Theme>();
        let mode = theme.mode;
        theme.window_blur_enabled = blur_enabled;
        theme.surface_opacity = clamp_surface_opacity(opacity);

        // 重新应用毛玻璃调整到主题颜色
        crate::theme::apply_glass_tuning(
            &mut theme.colors,
            mode,
            blur_enabled,
            theme.surface_opacity,
        );

        // 刷新所有窗口以应用新颜色
        cx.refresh_windows();
    }

    /// Get the input background color.
    #[inline]
    pub fn input_background(&self) -> Hsla {
        if self.is_dark() {
            self.input.mix(self.transparent, 0.1)
        } else {
            self.background
        }
    }

    /// Get the editor background color, if not set, use the input background color.
    #[inline]
    pub(crate) fn editor_background(&self) -> Hsla {
        self.highlight_theme
            .style
            .editor_background
            .unwrap_or_else(|| self.input_background())
    }

    /// 获取语义化颜色（基于当前模式）
    #[inline(always)]
    pub fn semantic(&self) -> SemanticColorsRef<'_> {
        if self.is_dark() {
            SemanticColorsRef::Dark(&SEMANTIC_DARK)
        } else {
            SemanticColorsRef::Light(&SEMANTIC_LIGHT)
        }
    }
}

impl From<&ThemeColor> for Theme {
    fn from(colors: &ThemeColor) -> Self {
        Theme {
            mode: ThemeMode::default(),
            transparent: Hsla::transparent_black(),
            window_blur_enabled: true,
            surface_opacity: DEFAULT_GLASS_OPACITY,
            font_family: ".SystemUIFont".into(),
            font_size: px(16.),
            mono_font_family: if cfg!(target_os = "macos") {
                // https://en.wikipedia.org/wiki/Menlo_(typeface)
                "Menlo".into()
            } else if cfg!(target_os = "windows") {
                "Consolas".into()
            } else {
                "DejaVu Sans Mono".into()
            },
            mono_font_size: px(13.),
            radius: px(6.),
            radius_lg: px(8.),
            shadow: true,
            scrollbar_show: ScrollbarShow::default(),
            notification: NotificationSettings::default(),
            tile_grid_size: px(8.),
            tile_shadow: true,
            tile_radius: px(0.),
            list: ListSettings::default(),
            colors: *colors,
            light_theme: Rc::new(ThemeConfig::default()),
            dark_theme: Rc::new(ThemeConfig::default()),
            highlight_theme: HighlightTheme::default_light(),
            sheet: SheetSettings::default(),
        }
    }
}

fn clamp_surface_opacity(opacity: f64) -> f32 {
    (opacity as f32).clamp(MIN_GLASS_OPACITY, MAX_GLASS_OPACITY)
}

impl Theme {
    fn ensure_global(cx: &mut App) {
        if !cx.has_global::<Theme>() {
            let mut theme = Theme::default();
            theme.light_theme = ThemeRegistry::global(cx).default_light_theme().clone();
            theme.dark_theme = ThemeRegistry::global(cx).default_dark_theme().clone();
            cx.set_global(theme);
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    PartialOrd,
    Eq,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    #[default]
    Light,
    Dark,
}

impl ThemeMode {
    #[inline(always)]
    pub fn is_dark(&self) -> bool {
        matches!(self, Self::Dark)
    }

    /// Return lower_case theme name: `light`, `dark`.
    pub fn name(&self) -> &'static str {
        match self {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }
}

impl From<WindowAppearance> for ThemeMode {
    fn from(appearance: WindowAppearance) -> Self {
        match appearance {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => Self::Light,
        }
    }
}

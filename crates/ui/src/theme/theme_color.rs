use std::sync::Arc;

use crate::{ThemeMode, theme::DEFAULT_THEME_COLORS};

use gpui::Hsla;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Base color palette for theme colors.
///
/// This struct groups the 12 ANSI-style base colors used throughout the UI,
/// providing a more organized access pattern via `ThemeColor.base.*`.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct ThemeBaseColors {
    /// The base red color.
    pub red: Hsla,
    /// The base red light color.
    pub red_light: Hsla,
    /// The base green color.
    pub green: Hsla,
    /// The base green light color.
    pub green_light: Hsla,
    /// The base blue color.
    pub blue: Hsla,
    /// The base blue light color.
    pub blue_light: Hsla,
    /// The base yellow color.
    pub yellow: Hsla,
    /// The base yellow light color.
    pub yellow_light: Hsla,
    /// The base magenta color.
    pub magenta: Hsla,
    /// The base magenta light color.
    pub magenta_light: Hsla,
    /// The base cyan color.
    pub cyan: Hsla,
    /// The base cyan light color.
    pub cyan_light: Hsla,
}

/// Terminal UI colors for embedded terminal elements.
///
/// These colors are used for terminal-specific UI components like scrollbars,
/// search highlighting, status indicators, and command displays.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct TerminalUiColors {
    /// Scrollbar track background color.
    pub scrollbar_track: Hsla,
    /// Scrollbar thumb background color.
    pub scrollbar_thumb: Hsla,
    /// Search match highlight background (e.g., find result).
    pub search_match_bg: Hsla,
    /// Search current match highlight background.
    pub search_match_current_bg: Hsla,
    /// Status connected indicator color.
    pub status_connected: Hsla,
    /// Status disconnected/error indicator color.
    pub status_disconnected: Hsla,
    /// Primary text color for terminal UI elements.
    pub text_primary: Hsla,
    /// Muted text color for secondary terminal UI elements.
    pub text_muted: Hsla,
    /// Accent color for interactive terminal elements.
    pub accent: Hsla,
}

/// Theme colors used throughout the UI components.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, JsonSchema)]
pub struct ThemeColor {
    /// Used for accents such as hover background on MenuItem, ListItem, etc.
    pub accent: Hsla,
    /// Used for accent text color.
    pub accent_foreground: Hsla,
    /// Accordion background color.
    pub accordion: Hsla,
    /// Accordion hover background color.
    pub accordion_hover: Hsla,
    /// Default background color.
    pub background: Hsla,
    /// Default border color
    pub border: Hsla,
    /// Background color for GroupBox / Panel.
    pub group: Hsla,
    /// Text color for GroupBox.
    pub group_foreground: Hsla,
    /// Input caret color (Blinking cursor).
    pub caret: Hsla,
    /// Chart 1 color.
    pub chart_1: Hsla,
    /// Chart 2 color.
    pub chart_2: Hsla,
    /// Chart 3 color.
    pub chart_3: Hsla,
    /// Chart 4 color.
    pub chart_4: Hsla,
    /// Chart 5 color.
    pub chart_5: Hsla,
    /// Danger background color.
    pub danger: Hsla,
    /// Danger active background color.
    pub danger_active: Hsla,
    /// Danger text color.
    pub danger_foreground: Hsla,
    /// Danger hover background color.
    pub danger_hover: Hsla,
    /// Description List label background color.
    pub description_list_label: Hsla,
    /// Description List label foreground color.
    pub description_list_label_foreground: Hsla,
    /// Drag border color.
    pub drag_border: Hsla,
    /// Drop target background color.
    pub drop_target: Hsla,
    /// Default text color.
    pub foreground: Hsla,
    /// Info background color.
    pub info: Hsla,
    /// Info active background color.
    pub info_active: Hsla,
    /// Info text color.
    pub info_foreground: Hsla,
    /// Info hover background color.
    pub info_hover: Hsla,
    /// Border color for inputs such as Input, Select, etc.
    pub input: Hsla,
    /// Link text color.
    pub link: Hsla,
    /// Active link text color.
    pub link_active: Hsla,
    /// Hover link text color.
    pub link_hover: Hsla,
    /// Background color for List and ListItem.
    pub list: Hsla,
    /// Background color for active ListItem.
    pub list_active: Hsla,
    /// Border color for active ListItem.
    pub list_active_border: Hsla,
    /// Stripe background color for even ListItem.
    pub list_even: Hsla,
    /// Background color for List header.
    pub list_head: Hsla,
    /// Hover background color for ListItem.
    pub list_hover: Hsla,
    /// Muted backgrounds such as Skeleton and Switch.
    pub muted: Hsla,
    /// Muted text color, as used in disabled text.
    pub muted_foreground: Hsla,
    /// Background color for Popover.
    pub popover: Hsla,
    /// Text color for Popover.
    pub popover_foreground: Hsla,
    /// Primary background color.
    pub primary: Hsla,
    /// Active primary background color.
    pub primary_active: Hsla,
    /// Primary text color.
    pub primary_foreground: Hsla,
    /// Hover primary background color.
    pub primary_hover: Hsla,
    /// Progress bar background color.
    pub progress_bar: Hsla,
    /// Used for focus ring.
    pub ring: Hsla,
    /// Scrollbar background color.
    pub scrollbar: Hsla,
    /// Scrollbar thumb background color.
    pub scrollbar_thumb: Hsla,
    /// Scrollbar thumb hover background color.
    pub scrollbar_thumb_hover: Hsla,
    /// Secondary background color.
    pub secondary: Hsla,
    /// Active secondary background color.
    pub secondary_active: Hsla,
    /// Secondary text color, used for secondary Button text color or secondary text.
    pub secondary_foreground: Hsla,
    /// Hover secondary background color.
    pub secondary_hover: Hsla,
    /// Input selection background color.
    pub selection: Hsla,
    /// Sidebar background color.
    pub sidebar: Hsla,
    /// Sidebar accent background color.
    pub sidebar_accent: Hsla,
    /// Sidebar accent text color.
    pub sidebar_accent_foreground: Hsla,
    /// Sidebar border color.
    pub sidebar_border: Hsla,
    /// Sidebar text color.
    pub sidebar_foreground: Hsla,
    /// Sidebar primary background color.
    pub sidebar_primary: Hsla,
    /// Sidebar primary text color.
    pub sidebar_primary_foreground: Hsla,
    /// Skeleton background color.
    pub skeleton: Hsla,
    /// Slider bar background color.
    pub slider_bar: Hsla,
    /// Slider thumb background color.
    pub slider_thumb: Hsla,
    /// Success background color.
    pub success: Hsla,
    /// Success text color.
    pub success_foreground: Hsla,
    /// Success hover background color.
    pub success_hover: Hsla,
    /// Success active background color.
    pub success_active: Hsla,
    /// Bullish color for candlestick charts (upward price movement).
    pub bullish: Hsla,
    /// Bearish color for candlestick charts (downward price movement).
    pub bearish: Hsla,
    /// Switch background color.
    pub switch: Hsla,
    /// Switch thumb background color.
    pub switch_thumb: Hsla,
    /// Tab background color.
    pub tab: Hsla,
    /// Tab active background color.
    pub tab_active: Hsla,
    /// Tab active text color.
    pub tab_active_foreground: Hsla,
    /// TabBar background color.
    pub tab_bar: Hsla,
    /// TabBar segmented background color.
    pub tab_bar_segmented: Hsla,
    /// Tab text color.
    pub tab_foreground: Hsla,
    /// Table background color.
    pub table: Hsla,
    /// Table active item background color.
    pub table_active: Hsla,
    /// Table active item border color.
    pub table_active_border: Hsla,
    /// Stripe background color for even TableRow.
    pub table_even: Hsla,
    /// Table head background color.
    pub table_head: Hsla,
    /// Table head text color.
    pub table_head_foreground: Hsla,
    /// Table item hover background color.
    pub table_hover: Hsla,
    /// Table row border color.
    pub table_row_border: Hsla,
    /// TitleBar background color, use for Window title bar.
    pub title_bar: Hsla,
    /// TitleBar border color.
    pub title_bar_border: Hsla,
    /// Background color for Tiles.
    pub tiles: Hsla,
    /// Warning background color.
    pub warning: Hsla,
    /// Warning active background color.
    pub warning_active: Hsla,
    /// Warning hover background color.
    pub warning_hover: Hsla,
    /// Warning foreground color.
    pub warning_foreground: Hsla,
    /// Overlay background color.
    pub overlay: Hsla,
    /// Window border color.
    ///
    /// # Platform specific:
    ///
    /// This is only works on Linux, other platforms we can't change the window border color.
    pub window_border: Hsla,

    /// The base red color.
    pub red: Hsla,
    /// The base red light color.
    pub red_light: Hsla,
    /// The base green color.
    pub green: Hsla,
    /// The base green light color.
    pub green_light: Hsla,
    /// The base blue color.
    pub blue: Hsla,
    /// The base blue light color.
    pub blue_light: Hsla,
    /// The base yellow color.
    pub yellow: Hsla,
    /// The base yellow light color.
    pub yellow_light: Hsla,
    /// The base magenta color.
    pub magenta: Hsla,
    /// The base magenta light color.
    pub magenta_light: Hsla,
    /// The base cyan color.
    pub cyan: Hsla,
    /// The base cyan light color.
    pub cyan_light: Hsla,

    /// Base color palette grouping.
    ///
    /// This field provides organized access to the 12 base colors via `theme.base.*`.
    /// It is populated by `sync_base_palette()` after `apply_config()` completes.
    /// Marked with `#[serde(skip)]` to avoid redundant serialization.
    #[serde(skip)]
    pub base: ThemeBaseColors,

    /// Terminal UI colors for embedded terminal elements.
    ///
    /// This field provides colors for terminal-specific UI components like
    /// scrollbars, search highlighting, and status indicators.
    /// It is populated by `sync_terminal_ui()` after `apply_config()` completes.
    /// Marked with `#[serde(skip)]` to avoid redundant serialization.
    #[serde(skip)]
    pub terminal_ui: TerminalUiColors,
}

impl ThemeColor {
    /// Get the default light theme colors.
    pub fn light() -> Arc<Self> {
        DEFAULT_THEME_COLORS[&ThemeMode::Light].0.clone()
    }

    /// Get the default dark theme colors.
    pub fn dark() -> Arc<Self> {
        DEFAULT_THEME_COLORS[&ThemeMode::Dark].0.clone()
    }

    /// Synchronize the base palette from the individual base color fields.
    ///
    /// This method populates the `base` field with the current values of the
    /// individual base color fields (`red`, `green`, `blue`, etc.).
    /// Call this method after `apply_config()` to ensure `theme.base.*` stays
    /// in sync with the flat fields.
    pub fn sync_base_palette(&mut self) {
        self.base = ThemeBaseColors {
            red: self.red,
            red_light: self.red_light,
            green: self.green,
            green_light: self.green_light,
            blue: self.blue,
            blue_light: self.blue_light,
            yellow: self.yellow,
            yellow_light: self.yellow_light,
            magenta: self.magenta,
            magenta_light: self.magenta_light,
            cyan: self.cyan,
            cyan_light: self.cyan_light,
        };
    }

    /// Synchronize terminal UI colors from existing theme color fields.
    ///
    /// This method populates the `terminal_ui` field with colors derived from
    /// existing theme fields, providing a semantic interface for terminal UI elements.
    /// Call this method after `apply_config()` to ensure `theme.terminal_ui.*` is updated.
    pub fn sync_terminal_ui(&mut self) {
        self.terminal_ui = TerminalUiColors {
            scrollbar_track: self.scrollbar,
            scrollbar_thumb: self.scrollbar_thumb,
            search_match_bg: self.base.yellow,
            search_match_current_bg: self.base.yellow_light,
            status_connected: self.success,
            status_disconnected: self.danger,
            text_primary: self.foreground,
            text_muted: self.muted_foreground,
            accent: self.accent,
        };
    }
}

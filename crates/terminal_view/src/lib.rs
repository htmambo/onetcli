rust_i18n::i18n!("locales", fallback = "en");

pub mod addon;
pub mod agent_bridge;
pub mod agents;
pub mod cd_completion;
pub mod highlight_presets;
pub mod history_prompt;
pub mod keys;
pub mod registry;
pub mod risk;
pub mod settings;
pub mod sidebar;
pub mod ssh_form_window;
pub mod terminal_element;
pub mod theme;
pub mod view;

pub use addon::{AddonManager, HoveredLink, SearchAddon, TerminalAddon, WebLinksAddon};
pub use one_core::layout::{
    SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MAX_WIDTH, SIDEBAR_MIN_WIDTH, TOOLBAR_WIDTH,
};
pub use settings::{
    TerminalHighlightRule, TerminalSettings, current_settings, init_settings, update_settings,
};
pub use sidebar::{SettingsPanel, SidebarPanel, TerminalSidebar, TerminalSidebarEvent};
pub use ssh_form_window::{SshFormWindow, SshFormWindowConfig};
pub use terminal::terminal::{
    ConnectionState, SshTerminalConfig, Terminal, TerminalConnectionKind, TerminalModelEvent,
};
pub use terminal::terminal::{DEFAULT_RECOVERY_SCROLLBACK_LINES, MAX_RECOVERY_SCROLLBACK_LINES};
pub use theme::{
    AnsiPalette, DEFAULT_FONT_SIZE, DEFAULT_LINE_HEIGHT_SCALE, MAX_FONT_SIZE,
    MAX_LINE_HEIGHT_SCALE, MIN_FONT_SIZE, MIN_LINE_HEIGHT_SCALE, TerminalTheme,
    default_font_fallbacks,
};
pub use view::{
    TerminalView, TerminalViewEvent, build_local_terminal, init, set_recovery_scrollback_lines,
    with_recovery_snapshot_overrides,
};

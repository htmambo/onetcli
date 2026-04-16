rust_i18n::i18n!("locales", fallback = "en");

pub mod addon;
pub mod history_prompt;
pub mod keys;
pub mod serial_form_window;
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
pub use serial_form_window::{SerialFormWindow, SerialFormWindowConfig};
pub use settings::{current_settings, init_settings, update_settings, TerminalSettings};
pub use sidebar::{SettingsPanel, SidebarPanel, TerminalSidebar, TerminalSidebarEvent};
pub use ssh_form_window::{SshFormWindow, SshFormWindowConfig};
pub use terminal::terminal::{
    ConnectionState, SshTerminalConfig, Terminal, TerminalConnectionKind, TerminalModelEvent,
};
pub use terminal::terminal::{DEFAULT_RECOVERY_SCROLLBACK_LINES, MAX_RECOVERY_SCROLLBACK_LINES};
pub use theme::{
    default_font_fallbacks, TerminalTheme, DEFAULT_FONT_SIZE, DEFAULT_LINE_HEIGHT_SCALE,
    MAX_FONT_SIZE, MAX_LINE_HEIGHT_SCALE, MIN_FONT_SIZE, MIN_LINE_HEIGHT_SCALE,
};
pub use view::{
    build_local_terminal, init, set_recovery_scrollback_lines, with_recovery_snapshot_overrides,
    TerminalView, TerminalViewEvent,
};

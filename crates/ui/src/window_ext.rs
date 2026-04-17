use crate::{
    Placement, Root, SystemNotificationOptions, dialog::Dialog, input::InputState,
    notification::Notification, sheet::Sheet, show_system_notification,
};
use gpui::{App, Entity, Window};
use std::rc::Rc;

#[cfg(target_os = "macos")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MacTitlebarDoubleClickAction {
    None,
    Minimize,
    Zoom,
}

#[cfg(target_os = "macos")]
fn resolve_macos_titlebar_double_click_action(
    action_on_double_click: Option<&str>,
    miniaturize_on_double_click: Option<&str>,
) -> MacTitlebarDoubleClickAction {
    let action = action_on_double_click
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match action {
        Some(value) if value.eq_ignore_ascii_case("none") => MacTitlebarDoubleClickAction::None,
        Some(value) if value.eq_ignore_ascii_case("minimize") => {
            MacTitlebarDoubleClickAction::Minimize
        }
        Some(value)
            if value.eq_ignore_ascii_case("maximize") || value.eq_ignore_ascii_case("fill") =>
        {
            MacTitlebarDoubleClickAction::Zoom
        }
        Some(_) => MacTitlebarDoubleClickAction::Zoom,
        None => match miniaturize_on_double_click
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES") => {
                MacTitlebarDoubleClickAction::Minimize
            }
            _ => MacTitlebarDoubleClickAction::Zoom,
        },
    }
}

#[cfg(target_os = "macos")]
fn read_global_defaults(key: &str) -> Option<String> {
    use std::process::Command;
    let output = Command::new("defaults")
        .args(["read", "-g", key])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Extension trait for [`Window`] to add dialog, sheet .. functionality.
pub trait WindowExt: Sized {
    /// Opens a Sheet at right placement.
    fn open_sheet<F>(&mut self, cx: &mut App, build: F)
    where
        F: Fn(Sheet, &mut Window, &mut App) -> Sheet + 'static;

    /// Opens a Sheet at the given placement.
    fn open_sheet_at<F>(&mut self, placement: Placement, cx: &mut App, build: F)
    where
        F: Fn(Sheet, &mut Window, &mut App) -> Sheet + 'static;

    /// Return true, if there is an active Sheet.
    fn has_active_sheet(&mut self, cx: &mut App) -> bool;

    /// Closes the active Sheet.
    fn close_sheet(&mut self, cx: &mut App);

    /// Opens a Dialog.
    fn open_dialog<F>(&mut self, cx: &mut App, build: F)
    where
        F: FnMut(Dialog, &mut Window, &mut App) -> Dialog + 'static;

    /// Return true, if there is an active Dialog.
    fn has_active_dialog(&mut self, cx: &mut App) -> bool;

    /// Closes the last active Dialog.
    fn close_dialog(&mut self, cx: &mut App);

    /// Closes all active Dialogs.
    fn close_all_dialogs(&mut self, cx: &mut App);

    /// Pushes a notification to the notification list.
    fn push_notification(&mut self, note: impl Into<Notification>, cx: &mut App);

    /// Removes the notification with the given id.
    fn remove_notification<T: Sized + 'static>(&mut self, cx: &mut App);

    /// Clears all notifications.
    fn clear_notifications(&mut self, cx: &mut App);

    /// Returns number of notifications.
    fn notifications(&mut self, cx: &mut App) -> Rc<Vec<Entity<Notification>>>;

    /// Return current focused Input entity.
    fn focused_input(&mut self, cx: &mut App) -> Option<Entity<InputState>>;
    /// Returns true if there is a focused Input entity.
    fn has_focused_input(&mut self, cx: &mut App) -> bool;

    /// 按照 macOS 系统设置执行标题栏双击动作。
    fn handle_titlebar_double_click(&self);

    /// 发送系统级通知（不依赖窗口可见状态）
    fn show_system_notification(&self, opts: SystemNotificationOptions, cx: &App);
}

impl WindowExt for Window {
    #[inline]
    fn open_sheet<F>(&mut self, cx: &mut App, build: F)
    where
        F: Fn(Sheet, &mut Window, &mut App) -> Sheet + 'static,
    {
        self.open_sheet_at(Placement::Right, cx, build)
    }

    #[inline]
    fn open_sheet_at<F>(&mut self, placement: Placement, cx: &mut App, build: F)
    where
        F: Fn(Sheet, &mut Window, &mut App) -> Sheet + 'static,
    {
        Root::update(self, cx, move |root, window, cx| {
            root.open_sheet_at(placement, build, window, cx);
        })
    }

    #[inline]
    fn has_active_sheet(&mut self, cx: &mut App) -> bool {
        Root::read(self, cx).active_sheet.is_some()
    }

    #[inline]
    fn close_sheet(&mut self, cx: &mut App) {
        Root::update(self, cx, |root, window, cx| {
            root.close_sheet(window, cx);
        })
    }

    #[inline]
    fn open_dialog<F>(&mut self, cx: &mut App, build: F)
    where
        F: FnMut(Dialog, &mut Window, &mut App) -> Dialog + 'static,
    {
        Root::update(self, cx, move |root, window, cx| {
            root.open_dialog(build, window, cx);
        })
    }

    #[inline]
    fn has_active_dialog(&mut self, cx: &mut App) -> bool {
        Root::read(self, cx).active_dialogs.len() > 0
    }

    #[inline]
    fn close_dialog(&mut self, cx: &mut App) {
        Root::update(self, cx, |root, window, cx| {
            root.close_dialog(window, cx);
        })
    }

    #[inline]
    fn close_all_dialogs(&mut self, cx: &mut App) {
        Root::update(self, cx, |root, window, cx| {
            root.close_all_dialogs(window, cx);
        })
    }

    #[inline]
    fn push_notification(&mut self, note: impl Into<Notification>, cx: &mut App) {
        let note = note.into();
        Root::update(self, cx, |root, window, cx| {
            root.push_notification(note, window, cx);
        })
    }

    #[inline]
    fn remove_notification<T: Sized + 'static>(&mut self, cx: &mut App) {
        Root::update(self, cx, |root, window, cx| {
            root.remove_notification::<T>(window, cx);
        })
    }

    #[inline]
    fn clear_notifications(&mut self, cx: &mut App) {
        Root::update(self, cx, |root, window, cx| {
            root.clear_notifications(window, cx);
        })
    }

    #[inline]
    fn notifications(&mut self, cx: &mut App) -> Rc<Vec<Entity<Notification>>> {
        Rc::new(Root::read(self, cx).notification.read(cx).notifications())
    }

    #[inline]
    fn has_focused_input(&mut self, cx: &mut App) -> bool {
        Root::read(self, cx).focused_input.is_some()
    }

    #[inline]
    fn focused_input(&mut self, cx: &mut App) -> Option<Entity<InputState>> {
        Root::read(self, cx).focused_input.clone()
    }

    #[inline]
    fn show_system_notification(&self, opts: SystemNotificationOptions, cx: &App) {
        show_system_notification(opts, cx);
    }

    #[inline]
    fn handle_titlebar_double_click(&self) {
        #[cfg(target_os = "macos")]
        {
            let action = resolve_macos_titlebar_double_click_action(
                read_global_defaults("AppleActionOnDoubleClick").as_deref(),
                read_global_defaults("AppleMiniaturizeOnDoubleClick").as_deref(),
            );

            match action {
                MacTitlebarDoubleClickAction::None => {}
                MacTitlebarDoubleClickAction::Minimize => self.minimize_window(),
                MacTitlebarDoubleClickAction::Zoom => self.zoom_window(),
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            self.zoom_window();
        }
    }
}

#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use super::{MacTitlebarDoubleClickAction, resolve_macos_titlebar_double_click_action};

    #[test]
    fn respects_explicit_none_action() {
        let action = resolve_macos_titlebar_double_click_action(Some("None"), Some("1"));
        assert_eq!(action, MacTitlebarDoubleClickAction::None);
    }

    #[test]
    fn respects_explicit_minimize_action() {
        let action = resolve_macos_titlebar_double_click_action(Some("Minimize"), Some("0"));
        assert_eq!(action, MacTitlebarDoubleClickAction::Minimize);
    }

    #[test]
    fn falls_back_to_legacy_miniaturize_key() {
        let action = resolve_macos_titlebar_double_click_action(None, Some("1"));
        assert_eq!(action, MacTitlebarDoubleClickAction::Minimize);
    }

    #[test]
    fn defaults_to_zoom_when_no_preference_is_available() {
        let action = resolve_macos_titlebar_double_click_action(None, None);
        assert_eq!(action, MacTitlebarDoubleClickAction::Zoom);
    }

    #[test]
    fn treats_fill_as_zoom() {
        let action = resolve_macos_titlebar_double_click_action(Some("Fill"), Some("1"));
        assert_eq!(action, MacTitlebarDoubleClickAction::Zoom);
    }
}

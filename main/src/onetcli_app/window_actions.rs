use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{App, AppContext};

use super::close_guard::{
    GlobalAppCloseState, request_app_close_without_window, request_main_window_close,
};
use super::{GlobalHomePage, GlobalMainWindowHandle, GlobalTabContainer};

pub fn activate_tab_by_number(number: usize, cx: &mut App) {
    let Some(active_window) = cx.active_window() else {
        return;
    };
    let Some(container) = cx.try_global::<GlobalTabContainer>() else {
        return;
    };
    let container = container.tab_container.clone();

    cx.defer(move |cx| {
        _ = active_window.update(cx, |_, window, cx| {
            container.update(cx, |tc, cx| {
                if number == 1 && tc.has_pinned_tab() {
                    tc.activate_pinned_tab(window, cx);
                    return;
                }

                let index = if tc.has_pinned_tab() {
                    number.saturating_sub(2)
                } else {
                    number.saturating_sub(1)
                };

                if index < tc.tabs().len() {
                    tc.set_active_index(index, window, cx);
                }
            });
        });
    });
}

pub fn toggle_fullscreen(cx: &mut App) {
    let Some(active_window) = cx.active_window() else {
        return;
    };

    cx.defer(move |cx| {
        _ = active_window.update(cx, |_, window, _| {
            window.toggle_fullscreen();
        });
    });
}

pub fn duplicate_tab(cx: &mut App) {
    let Some(active_window) = cx.active_window() else {
        return;
    };
    let Some(home) = cx.try_global::<GlobalHomePage>() else {
        return;
    };
    let home_page = home.home_page.clone();

    cx.defer(move |cx| {
        _ = active_window.update(cx, |_, window, cx| {
            home_page.update(cx, |hp, cx| {
                hp.duplicate_active_tab(window, cx);
            });
        });
    });
}

pub fn open_sftp_from_tab(tab_id: String, cx: &mut App) {
    let Some(active_window) = cx.active_window() else {
        return;
    };
    let Some(home) = cx.try_global::<GlobalHomePage>() else {
        return;
    };
    let home_page = home.home_page.clone();

    cx.defer(move |cx| {
        _ = active_window.update(cx, |_, window, cx| {
            home_page.update(cx, |hp, cx| {
                hp.open_sftp_for_tab_id(&tab_id, window, cx);
            });
        });
    });
}

pub fn quit_app(cx: &mut App) {
    // 防止同一轮内重复触发退出动作。
    static QUITTING: AtomicBool = AtomicBool::new(false);
    if QUITTING.swap(true, Ordering::SeqCst) {
        return;
    }

    let Some(main_window_handle) = cx.try_global::<GlobalMainWindowHandle>().copied() else {
        if request_app_close_without_window(cx) {
            cx.quit();
        }
        QUITTING.store(false, Ordering::SeqCst);
        return;
    };

    cx.defer(move |cx| {
        if cx.try_global::<GlobalAppCloseState>().is_none() {
            cx.quit();
            QUITTING.store(false, Ordering::SeqCst);
            return;
        }

        let should_quit = main_window_handle
            .window_handle
            .update(cx, |_, window, cx| request_main_window_close(window, cx))
            .unwrap_or_else(|err| {
                tracing::warn!(
                    "quit_app: window update failed: {:?}, falling back to non-UI close check",
                    err
                );
                request_app_close_without_window(cx)
            });

        if should_quit {
            cx.quit();
        }
        QUITTING.store(false, Ordering::SeqCst);
    });
}

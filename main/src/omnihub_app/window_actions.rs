use std::sync::atomic::{AtomicBool, Ordering};

use gpui::App;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use gpui::Window;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

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

/// 窗口置顶状态（单窗口应用，使用静态原子量足够）。Windows / macOS 实现。
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) static ALWAYS_ON_TOP: AtomicBool = AtomicBool::new(false);

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn toggle_always_on_top(cx: &mut App) {
    let Some(active_window) = cx.active_window() else {
        return;
    };
    let tab_container = cx
        .try_global::<GlobalTabContainer>()
        .map(|global| global.tab_container.clone());
    cx.defer(move |cx| {
        _ = active_window.update(cx, |_, window, cx| {
            let next = !ALWAYS_ON_TOP.load(Ordering::Relaxed);
            if set_window_always_on_top(window, next).is_ok() {
                ALWAYS_ON_TOP.store(next, Ordering::Relaxed);
                if let Some(tab_container) = tab_container {
                    tab_container.update(cx, |_, cx| cx.notify());
                }
            }
        });
    });
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn set_window_always_on_top(window: &Window, always_on_top: bool) -> anyhow::Result<()> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|err| anyhow::anyhow!("获取窗口句柄失败: {err:?}"))?
        .as_raw();
    match handle {
        #[cfg(target_os = "macos")]
        RawWindowHandle::AppKit(handle) => {
            set_macos_always_on_top(handle.ns_view.as_ptr(), always_on_top)
        }
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(handle) => {
            set_windows_always_on_top(handle.hwnd.get(), always_on_top)
        }
        _ => Err(anyhow::anyhow!("当前平台暂不支持窗口置顶")),
    }
}

#[cfg(target_os = "macos")]
fn set_macos_always_on_top(
    ns_view: *mut std::ffi::c_void,
    always_on_top: bool,
) -> anyhow::Result<()> {
    if ns_view.is_null() {
        return Err(anyhow::anyhow!("获取 NSView 失败"));
    }

    type Id = *mut std::ffi::c_void;
    type Sel = *mut std::ffi::c_void;

    #[link(name = "objc")]
    unsafe extern "C" {
        #[link_name = "sel_registerName"]
        fn sel_register_name(name: *const std::ffi::c_char) -> Sel;
        #[link_name = "objc_msgSend"]
        fn objc_msg_send(receiver: Id, selector: Sel, ...) -> Id;
    }

    const NS_NORMAL_WINDOW_LEVEL: isize = 0;
    const NS_FLOATING_WINDOW_LEVEL: isize = 3;
    let level = if always_on_top {
        NS_FLOATING_WINDOW_LEVEL
    } else {
        NS_NORMAL_WINDOW_LEVEL
    };
    let window_selector = std::ffi::CString::new("window")?;
    let set_level_selector = std::ffi::CString::new("setLevel:")?;
    unsafe {
        let ns_window = objc_msg_send(ns_view.cast(), sel_register_name(window_selector.as_ptr()));
        if ns_window.is_null() {
            return Err(anyhow::anyhow!("获取 NSWindow 失败"));
        }
        objc_msg_send(
            ns_window,
            sel_register_name(set_level_selector.as_ptr()),
            level,
        );
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn set_windows_always_on_top(hwnd: isize, always_on_top: bool) -> anyhow::Result<()> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
    };

    let insert_after = if always_on_top {
        HWND_TOPMOST
    } else {
        HWND_NOTOPMOST
    };
    unsafe {
        SetWindowPos(
            HWND(hwnd as *mut _),
            Some(insert_after),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )?;
    }
    Ok(())
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

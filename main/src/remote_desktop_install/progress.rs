use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use gpui::{
    App, AsyncApp, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    WeakEntity, Window, div, px,
};
use gpui_component::{ActiveTheme, Sizable, WindowExt, progress::Progress, v_flex};
use rust_i18n::t;

const START_PROGRESS: f32 = 5.0;
const INSTALLING_PROGRESS: f32 = 95.0;
const FINISHED_PROGRESS: f32 = 100.0;
const PROGRESS_POLL_INTERVAL: Duration = Duration::from_millis(100);
const STATUS_INSTALLING: &str = "正在安装插件。";
const STATUS_FINISHED: &str = "安装完成，正在打开连接。";

#[derive(Debug)]
pub(crate) enum DownloadProgress {
    Started,
    Bytes { downloaded: u64, total: Option<u64> },
    Failed { error: String },
    Finished,
}

pub(crate) struct ProviderInstallProgressView {
    provider_label: SharedString,
    connection_name: SharedString,
    status: SharedString,
    value: f32,
}

impl ProviderInstallProgressView {
    pub(crate) fn new(provider_label: &str, connection_name: &str) -> Self {
        Self {
            provider_label: provider_label.to_string().into(),
            connection_name: connection_name.to_string().into(),
            status: STATUS_INSTALLING.into(),
            value: 0.0,
        }
    }

    fn apply_download_progress(&mut self, progress: DownloadProgress) {
        match progress {
            DownloadProgress::Started => {
                self.value = START_PROGRESS;
                self.status = STATUS_INSTALLING.into();
            }
            DownloadProgress::Bytes { downloaded, total } => {
                self.value = byte_progress_value(downloaded, total);
                self.status = STATUS_INSTALLING.into();
            }
            DownloadProgress::Failed { error } => {
                self.status = error.into();
            }
            DownloadProgress::Finished => {
                self.value = INSTALLING_PROGRESS;
                self.status = STATUS_INSTALLING.into();
            }
        }
    }

    fn mark_finished(&mut self) {
        self.value = FINISHED_PROGRESS;
        self.status = STATUS_FINISHED.into();
    }
}

impl Render for ProviderInstallProgressView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "连接「{}」需要安装「{}」远程桌面插件。",
                        self.connection_name, self.provider_label
                    )),
            )
            .child(
                Progress::new("remote-desktop-provider-install-progress")
                    .value(self.value)
                    .small(),
            )
            .child(div().text_sm().child(self.status.clone()))
    }
}

/// 往进度快照队列写入事件，连续的 Bytes 事件只保留最新一条。
pub(crate) fn push_download_progress(
    queue: &mut VecDeque<DownloadProgress>,
    progress: DownloadProgress,
) {
    let replace_tail = matches!(progress, DownloadProgress::Bytes { .. })
        && matches!(queue.back(), Some(DownloadProgress::Bytes { .. }));
    if replace_tail {
        if let Some(last) = queue.back_mut() {
            *last = progress;
        }
        return;
    }
    queue.push_back(progress);
}

pub(crate) fn open_install_progress_dialog(
    view: Entity<ProviderInstallProgressView>,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(t!("RemoteDesktop.install_progress_title").to_string())
            .child(view.clone())
            .w(px(420.))
            .close_button(false)
            .overlay_closable(false)
            .keyboard(false)
    });
}

/// 定时轮询进度快照并刷新进度视图，finished 置位后做最后一次刷新再退出。
pub(crate) fn watch_install_progress(
    view: WeakEntity<ProviderInstallProgressView>,
    snapshot: Arc<Mutex<VecDeque<DownloadProgress>>>,
    finished: Arc<AtomicBool>,
    cx: &mut App,
) {
    cx.spawn(async move |cx| {
        loop {
            if finished.load(Ordering::Relaxed) {
                break;
            }
            cx.background_executor().timer(PROGRESS_POLL_INTERVAL).await;
            sync_install_progress(&view, &snapshot, cx);
        }
        sync_install_progress(&view, &snapshot, cx);
    })
    .detach();
}

pub(crate) fn mark_install_finished(
    view: &WeakEntity<ProviderInstallProgressView>,
    cx: &mut AsyncApp,
) {
    let _ = view.update(cx, |view, cx| {
        view.mark_finished();
        cx.notify();
    });
}

fn sync_install_progress(
    view: &WeakEntity<ProviderInstallProgressView>,
    snapshot: &Arc<Mutex<VecDeque<DownloadProgress>>>,
    cx: &mut AsyncApp,
) {
    let Some(progress) = snapshot.lock().ok().and_then(|mut queue| queue.pop_front()) else {
        return;
    };
    let _ = view.update(cx, |view, cx| {
        view.apply_download_progress(progress);
        cx.notify();
    });
}

fn byte_progress_value(downloaded: u64, total: Option<u64>) -> f32 {
    let Some(total) = total.filter(|total| *total > 0) else {
        return START_PROGRESS;
    };
    START_PROGRESS + (downloaded as f32 / total as f32) * (INSTALLING_PROGRESS - 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_maps_download_events_to_progress_values() {
        let mut view = ProviderInstallProgressView::new("VNC", "测试连接");

        view.apply_download_progress(DownloadProgress::Started);
        assert_eq!(5.0, view.value);
        assert_eq!(STATUS_INSTALLING, view.status.as_ref());

        view.apply_download_progress(DownloadProgress::Bytes {
            downloaded: 50,
            total: Some(100),
        });
        assert_eq!(47.5, view.value);

        view.apply_download_progress(DownloadProgress::Bytes {
            downloaded: 50,
            total: None,
        });
        assert_eq!(5.0, view.value);

        view.apply_download_progress(DownloadProgress::Failed {
            error: "下载失败".to_string(),
        });
        assert_eq!("下载失败", view.status.as_ref());

        view.apply_download_progress(DownloadProgress::Finished);
        assert_eq!(95.0, view.value);

        view.mark_finished();
        assert_eq!(100.0, view.value);
        assert_eq!(STATUS_FINISHED, view.status.as_ref());
    }

    #[test]
    fn snapshot_queue_coalesces_byte_updates() {
        let mut queue = VecDeque::new();

        push_download_progress(&mut queue, DownloadProgress::Started);
        push_download_progress(
            &mut queue,
            DownloadProgress::Bytes {
                downloaded: 20,
                total: Some(100),
            },
        );
        push_download_progress(
            &mut queue,
            DownloadProgress::Bytes {
                downloaded: 40,
                total: Some(100),
            },
        );
        push_download_progress(&mut queue, DownloadProgress::Finished);

        assert!(matches!(queue.pop_front(), Some(DownloadProgress::Started)));
        assert!(matches!(
            queue.pop_front(),
            Some(DownloadProgress::Bytes {
                downloaded: 40,
                total: Some(100),
            })
        ));
        assert!(matches!(
            queue.pop_front(),
            Some(DownloadProgress::Finished)
        ));
        assert!(queue.is_empty());
    }
}

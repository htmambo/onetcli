//! 远程桌面插件安装引导：插件缺失时弹窗确认、从扩展市场下载安装，成功后自动打开连接。

mod download;
mod install;
mod marketplace;
mod progress;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use gpui::http_client::HttpClient;
use gpui::{
    App, AppContext, Context, ParentElement, SharedString, Styled, Task, WeakEntity, Window, div,
};
use gpui_component::WindowExt;
use gpui_component::dialog::DialogButtonProps;
use gpui_component::notification::Notification;
use one_core::gpui_tokio::{JoinError, Tokio};
use one_core::storage::{RemoteDesktopProtocol as StorageRemoteDesktopProtocol, StoredConnection};
use remote_desktop::{RemoteDesktopProtocol, RemoteDesktopProviderRegistry};
use rust_i18n::t;

use crate::home_tab::HomePage;
use download::download_package;
use install::{default_providers_root, install_provider_from_archive};
use marketplace::fetch_provider_package;
use progress::{
    DownloadProgress, ProviderInstallProgressView, mark_install_finished,
    open_install_progress_dialog, push_download_progress, watch_install_progress,
};

/// 打开远程桌面连接；插件缺失时先引导安装，安装成功后自动打开连接。
pub(crate) fn open_remote_desktop_with_provider_guard(
    home: &mut HomePage,
    connection: StoredConnection,
    protocol: StorageRemoteDesktopProtocol,
    window: &mut Window,
    cx: &mut Context<HomePage>,
) {
    let runtime_protocol = to_runtime_protocol(protocol);
    if RemoteDesktopProviderRegistry::load_default()
        .find(runtime_protocol)
        .is_some()
    {
        home.open_remote_desktop(connection, protocol, window, cx);
        return;
    }
    prompt_provider_install(connection, protocol, runtime_protocol, window, cx);
}

#[derive(Clone)]
struct InstallContext {
    home: WeakEntity<HomePage>,
    connection: StoredConnection,
    protocol: StorageRemoteDesktopProtocol,
    provider_id: String,
    provider_label: String,
    connection_name: String,
    http_client: Arc<dyn HttpClient>,
}

fn to_runtime_protocol(protocol: StorageRemoteDesktopProtocol) -> RemoteDesktopProtocol {
    match protocol {
        StorageRemoteDesktopProtocol::Rdp => RemoteDesktopProtocol::Rdp,
        StorageRemoteDesktopProtocol::Vnc => RemoteDesktopProtocol::Vnc,
    }
}

fn prompt_provider_install(
    connection: StoredConnection,
    protocol: StorageRemoteDesktopProtocol,
    runtime_protocol: RemoteDesktopProtocol,
    window: &mut Window,
    cx: &mut Context<HomePage>,
) {
    let ctx = InstallContext {
        home: cx.weak_entity(),
        connection_name: connection.name.clone(),
        connection,
        protocol,
        provider_id: runtime_protocol.provider_id().to_string(),
        provider_label: runtime_protocol.label().to_string(),
        http_client: cx.http_client(),
    };
    let title = t!("RemoteDesktop.install_required_title").to_string();
    let message = t!(
        "RemoteDesktop.install_required_message",
        connection = ctx.connection_name.clone(),
        label = ctx.provider_label.clone()
    )
    .to_string();
    let ok_text = t!("RemoteDesktop.install_confirm").to_string();

    window.open_dialog(cx, move |dialog, _window, _cx| {
        let ctx = ctx.clone();
        dialog
            .title(SharedString::from(title.clone()))
            .confirm()
            .button_props(DialogButtonProps::default().ok_text(ok_text.clone()))
            .child(div().text_sm().child(message.clone()))
            .on_ok(move |_, window, cx| {
                start_provider_install(ctx.clone(), window, cx);
                true
            })
    });
}

fn start_provider_install(ctx: InstallContext, window: &mut Window, cx: &mut App) {
    let view =
        cx.new(|_| ProviderInstallProgressView::new(&ctx.provider_label, &ctx.connection_name));
    let view_weak = view.downgrade();
    let snapshot: Arc<Mutex<VecDeque<DownloadProgress>>> = Arc::new(Mutex::new(VecDeque::new()));
    let finished = Arc::new(AtomicBool::new(false));
    watch_install_progress(
        view_weak.clone(),
        Arc::clone(&snapshot),
        Arc::clone(&finished),
        cx,
    );

    let install_task = spawn_install_task(&ctx, snapshot, cx);
    let InstallContext {
        home,
        connection,
        protocol,
        provider_label,
        ..
    } = ctx;
    window
        .spawn(cx, async move |cx| {
            // 确认弹窗在本帧关闭，进度弹窗延迟到异步上下文打开，避免被误关
            let _ = cx.update(|window, cx| {
                open_install_progress_dialog(view, window, cx);
            });
            let outcome = match install_task.await {
                Ok(result) => result.map_err(|error| format!("{error:?}")),
                Err(error) => Err(format!("安装任务执行失败: {error}")),
            };
            finished.store(true, Ordering::Relaxed);
            if outcome.is_ok() {
                mark_install_finished(&view_weak, cx);
            }
            let _ = cx.update(move |window, cx| {
                window.close_dialog(cx);
                finish_install(
                    home,
                    connection,
                    protocol,
                    provider_label,
                    outcome,
                    window,
                    cx,
                );
            });
        })
        .detach();
}

fn spawn_install_task(
    ctx: &InstallContext,
    snapshot: Arc<Mutex<VecDeque<DownloadProgress>>>,
    cx: &mut App,
) -> Task<Result<anyhow::Result<PathBuf>, JoinError>> {
    let http_client = Arc::clone(&ctx.http_client);
    let provider_id = ctx.provider_id.clone();
    Tokio::spawn(cx, async move {
        let result = run_install(http_client, &provider_id, &snapshot).await;
        if let Err(error) = &result {
            push_snapshot(
                &snapshot,
                DownloadProgress::Failed {
                    error: format!("{error:?}"),
                },
            );
        }
        result
    })
}

async fn run_install(
    http_client: Arc<dyn HttpClient>,
    provider_id: &str,
    snapshot: &Arc<Mutex<VecDeque<DownloadProgress>>>,
) -> anyhow::Result<PathBuf> {
    let package = fetch_provider_package(Arc::clone(&http_client), provider_id).await?;
    tracing::debug!(version = %package.version, "已获取远程桌面插件下载信息");
    push_snapshot(snapshot, DownloadProgress::Started);
    let bytes_snapshot = Arc::clone(snapshot);
    let archive = download_package(
        http_client,
        &package.download_url,
        &package.sha256,
        move |downloaded, total| {
            push_snapshot(
                &bytes_snapshot,
                DownloadProgress::Bytes { downloaded, total },
            );
        },
    )
    .await?;
    push_snapshot(snapshot, DownloadProgress::Finished);
    install_provider_from_archive(&archive, &default_providers_root()?)
}

fn push_snapshot(snapshot: &Arc<Mutex<VecDeque<DownloadProgress>>>, progress: DownloadProgress) {
    if let Ok(mut queue) = snapshot.lock() {
        push_download_progress(&mut queue, progress);
    }
}

fn finish_install(
    home: WeakEntity<HomePage>,
    connection: StoredConnection,
    protocol: StorageRemoteDesktopProtocol,
    provider_label: String,
    outcome: Result<PathBuf, String>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(home) = home.upgrade() else {
        return;
    };
    match outcome {
        Ok(_) => {
            window.push_notification(
                Notification::success(
                    t!("RemoteDesktop.install_success", label = provider_label).to_string(),
                )
                .autohide(true),
                cx,
            );
            home.update(cx, |home, cx| {
                home.open_remote_desktop(connection, protocol, window, cx);
            });
        }
        Err(error) => {
            window.push_notification(
                Notification::error(t!("RemoteDesktop.install_failed", error = error).to_string())
                    .autohide(true),
                cx,
            );
        }
    }
}

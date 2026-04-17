use crate::storage::Certificate;
use gpui::{App, AppContext, Context, Entity, EventEmitter};

/// 证书数据变更事件
#[derive(Debug, Clone)]
pub enum CertificateDataEvent {
    Saved { certificate: Certificate },
    Deleted { certificate_id: i64 },
}

pub struct CertificateDataNotifier;

impl EventEmitter<CertificateDataEvent> for CertificateDataNotifier {}

#[derive(Clone)]
pub struct GlobalCertificateNotifier(pub Entity<CertificateDataNotifier>);

impl gpui::Global for GlobalCertificateNotifier {}

pub fn init(cx: &mut App) {
    let notifier = cx.new(|_| CertificateDataNotifier);
    cx.set_global(GlobalCertificateNotifier(notifier));
}

pub fn get_notifier(cx: &App) -> Option<Entity<CertificateDataNotifier>> {
    cx.try_global::<GlobalCertificateNotifier>()
        .map(|global| global.0.clone())
}

pub fn emit_certificate_event<T>(event: CertificateDataEvent, cx: &mut Context<T>) {
    if let Some(notifier) = cx.try_global::<GlobalCertificateNotifier>().cloned() {
        notifier.0.update(cx, |_, cx| {
            cx.emit(event);
        });
    }
}

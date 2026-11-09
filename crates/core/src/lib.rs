use gpui::App;

rust_i18n::i18n!("locales", fallback = "zh-CN");

pub mod agent;
pub mod ai_chat;
pub mod certificate_manager;
pub mod certificate_notifier;
pub mod cloud_sync;
pub mod config;
pub mod connection_notifier;
pub mod connection_restore;
pub mod connection_state;
pub mod crypto;
pub mod gpui_tokio;
pub mod key_storage;
pub mod keybindings;
pub mod layout;
pub mod llm;
pub mod popup_window;
pub mod running_state;
pub use crate::running_state::{PendingChangeLevel, RunningKind, RunningState};
pub mod storage;
pub mod tab_container;
// S4：`tab_persistence` 内 unsafe 用于读取未初始化内存（性能原因跳过零初始化）；
// 已限定作用域，调用方契约清晰。
#[allow(unsafe_code)]
pub mod tab_persistence;
pub mod themes;
pub mod utils;

pub use crate::agent::{
    Agent, AgentContext, AgentDescriptor, AgentDispatcher, AgentEvent, AgentRegistry, AgentResult,
    SessionAffinity,
};
pub use crate::ai_chat::{
    AiChatColors, AiChatPanel, AiChatPanelEvent, ChatMessageUI, ChatMessageUIGeneric, ChatRole,
    CodeBlockAction, CodeBlockActionBuilder, CodeBlockActionCallback, CodeBlockActionRegistry,
    LanguageMatcher, MessageExtension, MessageVariant, NoExtension, ProviderItem,
};
pub use crate::ai_chat::{
    ChatEngine, ChatMessageRenderer, ChatStreamProcessor, CoreStreamEvent, StreamError,
};
pub use serde_json;

/// 测试专用：跨 crate 的进程级 `HOME` 环境变量锁。
///
/// 多个 crate 的测试会修改全局 `HOME`（隔离 shell / storage 配置），
/// `cargo test --workspace` 并行运行不同 crate 的测试二进制时会互相竞态
/// （典型症状：PTY 测试启动 shell 时读到被其他测试改写的 HOME 而偶发失败）。
/// 所有修改 `HOME` 的测试都应持有此锁串行执行。
#[doc(hidden)]
pub fn home_env_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn init(cx: &mut App) {
    gpui_tokio::init(cx);
    themes::init(cx);
    storage::init(cx);
    llm::init(cx);
    agent::init(cx);
    connection_notifier::init(cx);
    certificate_notifier::init(cx);
    popup_window::init(cx);
}

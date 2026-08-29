//! AI Chat Panel - 数据库 AI 助手对话面板

use crate::agent::registry::AgentRegistry;
use crate::agent::{AgentContext, AgentDispatcher, AgentEvent, SessionAffinity};
use crate::cloud_sync::GlobalCloudUser;
use crate::gpui_tokio::Tokio;
use crate::llm::chat_history::ChatMessage;
use crate::llm::{
    Message, ProviderConfig, ProviderConfigRevision, Role, ToolCall,
    chat_history::{MessageRepository, SessionRepository},
    manager::GlobalProviderState,
    storage::ProviderRepository,
};
use crate::storage::{GlobalStorageState, StorageManager, traits::Repository};
use gpui::{
    AnyElement, AnyWindowHandle, App, AppContext, AsyncApp, Context, Corner, Entity, EventEmitter,
    FocusHandle, Focusable, Hsla, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Subscription, Window, div,
    prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, IconName, Sizable, Size, WindowExt as _,
    button::{Button, ButtonVariants},
    dialog::DialogButtonProps,
    h_flex,
    input::{Input, InputEvent, InputState},
    list::{List, ListState},
    popover::Popover,
    text::{CodeBlock, CodeBlockRenderOptions, CodeBlockRenderer},
    v_flex,
};
use rust_i18n::t;
use std::any::Any;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
// 使用引擎和渲染器
use super::engine::ChatEngine;
use super::rendering::ChatMessageRenderer;
use super::stream::{ChatStreamProcessor, StreamEvent};
use super::types::{ChatMessageUI, MessageVariant, ToolCallStatus};
// 使用共享组件
use super::components::{
    ModelSettings, ModelSettingsEvent, ModelSettingsPanel, ProviderItem, ProviderSelectEvent,
    ProviderSelectState, SessionData, SessionListConfig, SessionListDelegate, SessionListHost,
};

/// AI 聊天面板的自定义颜色配置
///
/// 用于在终端等需要自定义主题的场景中覆盖默认颜色
#[derive(Clone, Debug)]
pub struct AiChatColors {
    /// 主背景色
    pub background: Hsla,
    /// 主前景色（文字）
    pub foreground: Hsla,
    /// 次要背景色（卡片、列表项）
    pub muted: Hsla,
    /// 次要前景色（占位符、次要文字）
    pub muted_foreground: Hsla,
    /// 边框色
    pub border: Hsla,
    /// 强调背景色
    pub accent: Hsla,
    /// 强调前景色
    pub accent_foreground: Hsla,
}

// ============================================================================
// 代码块操作扩展机制
// ============================================================================

/// 语言匹配器 - 用于匹配代码块的语言类型
#[derive(Clone)]
pub enum LanguageMatcher {
    /// 精确匹配（不区分大小写）
    Exact(Vec<&'static str>),
    /// 前缀匹配
    Prefix(&'static str),
    /// 自定义匹配函数
    Custom(Arc<dyn Fn(&str) -> bool + Send + Sync>),
    /// 匹配所有语言（包括未指定语言的代码块）
    Any,
}

impl LanguageMatcher {
    /// 创建精确匹配器（单个语言）
    pub fn exact(lang: &'static str) -> Self {
        Self::Exact(vec![lang])
    }

    /// 创建精确匹配器（多个语言）
    pub fn exact_many(langs: Vec<&'static str>) -> Self {
        Self::Exact(langs)
    }

    /// 创建 SQL 语言匹配器
    pub fn sql() -> Self {
        Self::Exact(vec![
            "sql",
            "mysql",
            "postgresql",
            "postgres",
            "sqlite",
            "mssql",
            "oracle",
            "plsql",
        ])
    }

    /// 创建 Shell/Bash 语言匹配器
    pub fn shell() -> Self {
        Self::Exact(vec![
            "bash",
            "sh",
            "shell",
            "zsh",
            "fish",
            "powershell",
            "ps1",
            "cmd",
            "batch",
        ])
    }

    /// 创建 Python 语言匹配器
    pub fn python() -> Self {
        Self::Exact(vec!["python", "py", "python3"])
    }

    /// 创建 Rust 语言匹配器
    pub fn rust() -> Self {
        Self::Exact(vec!["rust", "rs"])
    }

    /// 创建 JavaScript/TypeScript 语言匹配器
    pub fn javascript() -> Self {
        Self::Exact(vec!["javascript", "js", "typescript", "ts", "jsx", "tsx"])
    }

    /// 检查是否匹配给定的语言
    pub fn matches(&self, lang: Option<&str>) -> bool {
        match self {
            LanguageMatcher::Exact(langs) => lang.map_or(false, |l| {
                let l_lower = l.to_lowercase();
                langs
                    .iter()
                    .any(|&expected| expected.eq_ignore_ascii_case(&l_lower))
            }),
            LanguageMatcher::Prefix(prefix) => lang.map_or(false, |l| {
                l.to_lowercase().starts_with(&prefix.to_lowercase())
            }),
            LanguageMatcher::Custom(f) => lang.map_or(false, |l| f(l)),
            LanguageMatcher::Any => true,
        }
    }
}

/// 代码块操作回调函数类型
///
/// 参数：
/// - `code`: 代码块内容
/// - `lang`: 代码块语言（可能为空）
/// - `window`: 窗口引用
/// - `cx`: 应用上下文
pub type CodeBlockActionCallback =
    Arc<dyn Fn(String, Option<String>, &mut Window, &mut App) + Send + Sync>;

/// 代码块操作定义
///
/// 用于定义一个可以在代码块上执行的操作，例如：
/// - SQL 代码发送到编辑器
/// - Shell 命令复制到终端
/// - Python 代码直接运行
#[derive(Clone)]
pub struct CodeBlockAction {
    /// 唯一标识符
    pub id: SharedString,
    /// 显示图标
    pub icon: IconName,
    /// 按钮标签（可选，如果为 None 则只显示图标）
    pub label: Option<SharedString>,
    /// 语言匹配器
    pub matcher: LanguageMatcher,
    /// 操作回调
    pub callback: CodeBlockActionCallback,
}

impl CodeBlockAction {
    /// 创建新的代码块操作
    pub fn new(id: impl Into<SharedString>) -> CodeBlockActionBuilder {
        CodeBlockActionBuilder {
            id: id.into(),
            icon: IconName::SquareTerminal,
            label: None,
            matcher: LanguageMatcher::Any,
            callback: None,
        }
    }
}

/// 代码块操作构建器
pub struct CodeBlockActionBuilder {
    id: SharedString,
    icon: IconName,
    label: Option<SharedString>,
    matcher: LanguageMatcher,
    callback: Option<CodeBlockActionCallback>,
}

impl CodeBlockActionBuilder {
    /// 设置图标
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = icon;
        self
    }

    /// 设置标签
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// 设置语言匹配器
    pub fn matcher(mut self, matcher: LanguageMatcher) -> Self {
        self.matcher = matcher;
        self
    }

    /// 设置回调函数
    pub fn on_click<F>(mut self, f: F) -> Self
    where
        F: Fn(String, Option<String>, &mut Window, &mut App) + Send + Sync + 'static,
    {
        self.callback = Some(Arc::new(f));
        self
    }

    /// 构建代码块操作
    pub fn build(self) -> Option<CodeBlockAction> {
        self.callback.map(|callback| CodeBlockAction {
            id: self.id,
            icon: self.icon,
            label: self.label,
            matcher: self.matcher,
            callback,
        })
    }
}

/// 代码块操作注册表
///
/// 用于管理和查询已注册的代码块操作
#[derive(Clone, Default)]
pub struct CodeBlockActionRegistry {
    actions: Vec<CodeBlockAction>,
}

impl CodeBlockActionRegistry {
    /// 创建空的注册表
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    /// 注册一个代码块操作
    pub fn register(&mut self, action: CodeBlockAction) {
        self.actions.push(action);
    }

    /// 获取匹配指定语言的所有操作
    pub fn get_actions_for_lang(&self, lang: Option<&str>) -> Vec<&CodeBlockAction> {
        self.actions
            .iter()
            .filter(|action| action.matcher.matches(lang))
            .collect()
    }

    /// 检查是否有注册的操作
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// 获取所有操作数量
    pub fn len(&self) -> usize {
        self.actions.len()
    }
}

/// AI 聊天面板事件
#[derive(Clone, Debug)]
pub enum AiChatPanelEvent {
    Close,
    ExecuteSql {
        sql: String,
        connection_id: String,
        database: Option<String>,
        schema: Option<String>,
    },
}

/// Agent 能力注入器：构建 AgentContext 时逐个调用
type CapabilityInjector = Box<dyn Fn(&mut AgentContext) + Send + Sync>;

/// AI 聊天面板
pub struct AiChatPanel {
    focus_handle: FocusHandle,

    /// 共享业务逻辑引擎
    engine: ChatEngine,

    ai_input_state: Entity<InputState>,
    provider_select_state: ProviderSelectState,

    _subscriptions: Vec<Subscription>,
    connection_name: Option<String>,
    database: Option<String>,
    history_popover_open: bool,
    session_list: Option<Entity<ListState<SessionListDelegate<AiChatPanel>>>>,
    /// 可选的自定义颜色（用于终端等需要自定义主题的场景）
    custom_colors: Option<AiChatColors>,
    /// 模型设置面板
    settings_panel: Entity<ModelSettingsPanel>,
    is_logged_in: bool,
    /// 场景专属系统提示词，仅在发送消息时前置注入
    system_instruction: Option<String>,
    /// Agent 调度模式开关（启用后 send_message 走 AgentDispatcher）
    agent_mode: bool,
    /// Agent 能力注入器（构建 AgentContext 时逐个调用）
    capability_injectors: Vec<CapabilityInjector>,
    /// Agent 会话亲和性（跨请求保持同一路由）
    session_affinity: SessionAffinity,
    /// 自定义代码块渲染器（如终端工具卡片重建）
    code_block_renderer: Option<Arc<CodeBlockRenderer>>,
    /// 面板所在窗口句柄（异步刷新 provider 时需回到自己的窗口更新选择器）
    window_handle: AnyWindowHandle,
}

impl AiChatPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        // 创建引擎
        let global_state = cx.global::<GlobalStorageState>();
        let engine = ChatEngine::new(global_state.storage.clone());

        // Agent 模式输入框
        let agent_input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("AiChat.input_placeholder").to_string())
                .auto_grow(2, 6)
                .default_value("")
        });

        // Provider/Model 选择器（回调直接接收 &mut Self，避免重复借用）
        let provider_select_state =
            ProviderSelectState::new(window, cx, |event, this, window, cx| match event {
                ProviderSelectEvent::ProviderChanged { provider_id, .. } => {
                    this.engine.provider_id = Some(provider_id.clone());
                    this.engine.selected_model = this
                        .provider_select_state
                        .update_models_for_provider(&provider_id, window, cx);
                    cx.notify();
                }
                ProviderSelectEvent::ModelChanged { model } => {
                    this.engine.selected_model = Some(model.clone());
                    cx.notify();
                }
            });

        let mut subscriptions = Vec::new();

        // 订阅 Agent 输入事件
        subscriptions.push(cx.subscribe_in(
            &agent_input_state,
            window,
            |this, _state, event, window, cx| {
                if let InputEvent::PressEnter { secondary } = event {
                    if !secondary {
                        this.submit(window, cx);
                    }
                }
            },
        ));

        // 创建模型设置面板
        let model_settings = ModelSettings::default();
        let settings_panel =
            cx.new(|cx| ModelSettingsPanel::new(model_settings.clone(), window, cx));

        // 订阅模型设置事件
        subscriptions.push(cx.subscribe_in(
            &settings_panel,
            window,
            |this, _panel, event: &ModelSettingsEvent, _window, cx| match event {
                ModelSettingsEvent::Changed(settings) => {
                    this.engine.model_settings = settings.clone();
                    cx.notify();
                }
            },
        ));

        // 订阅供应商配置变更（设置页增删改 provider 后通知已打开的面板重新加载）
        subscriptions.push(cx.observe_global::<ProviderConfigRevision>(|this, cx| {
            this.load_providers(cx);
        }));

        let window_handle = window.window_handle();

        let mut panel = Self {
            focus_handle,
            engine,
            ai_input_state: agent_input_state,
            provider_select_state,
            _subscriptions: subscriptions,
            connection_name: None,
            database: None,
            history_popover_open: false,
            session_list: None,
            custom_colors: None,
            settings_panel,
            is_logged_in: GlobalCloudUser::is_logged_in(cx),
            system_instruction: None,
            agent_mode: false,
            capability_injectors: Vec::new(),
            session_affinity: SessionAffinity::new(),
            code_block_renderer: None,
            window_handle,
        };

        // 加载 providers
        panel.load_providers(cx);
        panel
    }

    fn load_providers(&mut self, cx: &mut Context<Self>) {
        let global_state = cx.global::<GlobalStorageState>();
        let storage_manager = global_state.storage.clone();
        let is_logged_in = GlobalCloudUser::is_logged_in(cx);
        self.is_logged_in = is_logged_in;
        // 必须回到面板自己的窗口更新选择器；激活窗口可能是设置弹窗等别的窗口
        let window_handle = self.window_handle;

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let providers = {
                let repo = match storage_manager.get::<ProviderRepository>() {
                    Some(r) => r,
                    None => return,
                };
                let mut list = match repo.list() {
                    Ok(all) => all.into_iter().filter(|p| p.enabled).collect::<Vec<_>>(),
                    Err(_) => Vec::new(),
                };
                if is_logged_in {
                    if let Ok(onet) = repo.ensure_omnihub_provider() {
                        if !list.iter().any(|p| p.id == onet.id) {
                            list.insert(0, onet);
                        }
                    }
                } else {
                    list.retain(|p| !p.is_builtin());
                }
                list
            };

            let _ = cx.update(|cx| {
                let _ = window_handle.update(cx, |_, window, cx| {
                    if let Some(entity) = this.upgrade() {
                        entity.update(cx, |panel, cx| {
                            panel.engine.provider_configs = providers.clone();
                            let items: Vec<_> =
                                providers.iter().map(ProviderItem::from_config).collect();
                            panel.provider_select_state.set_providers(items, window, cx);
                            panel.engine.provider_id =
                                panel.provider_select_state.selected_provider().cloned();
                            panel.engine.selected_model =
                                panel.provider_select_state.selected_model().cloned();
                            cx.notify();
                        });
                    }
                });
            });
        })
        .detach();
    }

    pub fn set_connection_info(
        &mut self,
        connection_name: Option<String>,
        database: Option<String>,
    ) {
        self.connection_name = connection_name;
        self.database = database;
    }

    /// 设置场景专属系统提示词
    pub fn set_system_instruction(&mut self, instruction: Option<String>, cx: &mut Context<Self>) {
        self.system_instruction = instruction.and_then(|instruction| {
            let trimmed = instruction.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        });
        cx.notify();
    }

    /// 启用或关闭 Agent 调度模式
    ///
    /// 启用后 `send_message` 改走 `AgentDispatcher::dispatch`，支持工具调用；
    /// 未启用时行为与原流式聊天路径完全一致。
    pub fn set_agent_dispatch(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.agent_mode = enabled;
        cx.notify();
    }

    /// 注册 Agent 能力值，Agent 模式构建 `AgentContext` 时自动注入
    pub fn set_capability_value<T: Any + Send + Sync + Clone>(
        &mut self,
        key: impl Into<String>,
        value: T,
    ) {
        let key = key.into();
        self.capability_injectors.push(Box::new(move |ctx| {
            ctx.set_capability(key.clone(), value.clone());
        }));
    }

    /// 设置自定义代码块渲染器（如终端工具卡片重建）
    pub fn set_code_block_renderer<F>(&mut self, renderer: F, cx: &mut Context<Self>)
    where
        F: Fn(&CodeBlock, CodeBlockRenderOptions, AnyElement, &mut Window, &mut App) -> AnyElement
            + Send
            + Sync
            + 'static,
    {
        self.code_block_renderer = Some(Arc::new(renderer));
        cx.notify();
    }

    /// 设置自定义颜色（用于终端等需要自定义主题的场景）
    pub fn set_colors(&mut self, colors: AiChatColors, cx: &mut Context<Self>) {
        self.custom_colors = Some(colors);
        cx.notify();
    }

    /// 获取背景色（自定义或默认主题）
    fn background(&self, cx: &App) -> Hsla {
        self.custom_colors
            .as_ref()
            .map(|c| c.background)
            .unwrap_or_else(|| cx.theme().background)
    }

    /// 获取前景色（自定义或默认主题）
    fn foreground(&self, cx: &App) -> Hsla {
        self.custom_colors
            .as_ref()
            .map(|c| c.foreground)
            .unwrap_or_else(|| cx.theme().foreground)
    }

    /// 获取次要背景色（自定义或默认主题）
    fn muted(&self, cx: &App) -> Hsla {
        self.custom_colors
            .as_ref()
            .map(|c| c.muted)
            .unwrap_or_else(|| cx.theme().muted)
    }

    /// 获取边框色（自定义或默认主题）
    fn border(&self, cx: &App) -> Hsla {
        self.custom_colors
            .as_ref()
            .map(|c| c.border)
            .unwrap_or_else(|| cx.theme().border)
    }

    pub fn set_provider_id(&mut self, provider_id: String, cx: &mut Context<Self>) {
        self.engine.provider_id = Some(provider_id.clone());
        if let Some(config) = self
            .engine
            .provider_configs
            .iter()
            .find(|provider| provider.id.to_string() == provider_id)
        {
            let models = ProviderSelectState::build_model_list_from_config(config);
            self.engine.selected_model =
                ProviderSelectState::resolve_default_model_from_config(config, &models);
        }
        cx.notify();
    }

    /// 注册代码块操作
    pub fn register_code_block_action(&mut self, action: CodeBlockAction, cx: &mut Context<Self>) {
        self.engine.code_block_actions.register(action);
        cx.notify();
    }

    /// 批量注册代码块操作
    pub fn register_code_block_actions(
        &mut self,
        actions: Vec<CodeBlockAction>,
        cx: &mut Context<Self>,
    ) {
        for action in actions {
            self.engine.code_block_actions.register(action);
        }
        cx.notify();
    }

    /// 获取代码块操作注册表的引用（用于外部查询）
    pub fn code_block_actions(&self) -> &CodeBlockActionRegistry {
        &self.engine.code_block_actions
    }

    /// 从外部发送消息到AI聊天
    pub fn send_external_message(&mut self, message: String, cx: &mut Context<Self>) {
        if !message.trim().is_empty() {
            self.send_message(message, cx);
        }
    }

    // 创建新会话 - 同步返回，异步保存
    pub fn start_new_session(&mut self, cx: &mut Context<Self>) {
        self.engine.start_new_session();
        self.session_affinity.reset();
        cx.notify();
    }

    /// 确保会话存在，如果不存在则创建新会话
    fn ensure_session_id(&mut self, provider_id: &str, cx: &mut Context<Self>) -> Option<i64> {
        let result = self.engine.ensure_session_id(
            provider_id,
            t!("AiChat.new_session_name").as_ref(),
            None,
        );
        if result.is_some() && self.engine.is_new_session {
            // 新创建的会话，需要刷新历史列表（ensure_session_id 已经设置了 is_new_session）
            self.load_history_sessions(cx);
        }
        result
    }

    /// 持久化用户消息，并在新会话时更新标题
    fn persist_user_message(&mut self, session_id: i64, content: &str, cx: &mut Context<Self>) {
        let was_new = self.engine.is_new_session;
        self.engine.persist_user_message(session_id, content);
        if was_new {
            self.load_history_sessions(cx);
        }
    }

    /// 取消当前操作
    pub fn cancel_current_operation(&mut self, cx: &mut Context<Self>) {
        self.engine.cancel_current_operation();
        cx.notify();
    }

    /// 是否可以取消
    pub fn can_cancel(&self) -> bool {
        self.engine.can_cancel()
    }

    fn update_session_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sessions_data: Vec<SessionData> = self
            .engine
            .history_sessions
            .iter()
            .map(|s| SessionData::new(s.id, s.name.clone(), s.updated_at))
            .collect();
        let panel = cx.entity();

        if let Some(session_list) = &self.session_list {
            session_list.update(cx, |state, _| {
                let delegate = state.delegate_mut();
                delegate.update_sessions(sessions_data);
            });
        } else {
            self.session_list = Some(cx.new(|cx| {
                ListState::new(
                    SessionListDelegate::new(panel, sessions_data, SessionListConfig::default()),
                    window,
                    cx,
                )
                .searchable(true)
            }));
        }
    }

    fn load_history_sessions(&mut self, cx: &mut Context<Self>) {
        let global_state = cx.global::<GlobalStorageState>();
        let storage_manager = global_state.storage.clone();

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let sessions = {
                let session_repo = match storage_manager.get::<SessionRepository>() {
                    Some(r) => r,
                    None => return,
                };
                match session_repo.list() {
                    Ok(s) => s,
                    Err(_) => return,
                }
            };

            if let Some(entity) = this.upgrade() {
                let _ = cx.update(|cx| {
                    if let Some(window_id) = cx.active_window() {
                        let _ = cx.update_window(window_id, |_, window, cx| {
                            entity.update(cx, |this, cx| {
                                this.engine.history_sessions = sessions;
                                this.update_session_list(window, cx);
                                cx.notify();
                            });
                        });
                    }
                });
            }
        })
        .detach();
    }

    fn delete_session(&mut self, session_id: i64, cx: &mut Context<Self>) {
        let global_state = cx.global::<GlobalStorageState>();
        let storage_manager = global_state.storage.clone();

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let delete_ok = {
                let session_repo = match storage_manager.get::<SessionRepository>() {
                    Some(r) => r,
                    None => return,
                };
                let message_repo = match storage_manager.get::<MessageRepository>() {
                    Some(r) => r,
                    None => return,
                };
                message_repo.delete_by_session(session_id).is_ok()
                    && session_repo.delete(session_id).is_ok()
            };

            if delete_ok {
                if let Some(entity) = this.upgrade() {
                    let _ = cx.update(|cx| {
                        entity.update(cx, |this, cx| {
                            if this.engine.session_id == Some(session_id) {
                                this.engine.session_id = None;
                                this.engine.messages.clear();
                            }
                            this.engine.history_sessions.retain(|s| s.id != session_id);
                            cx.notify();
                        });
                    });
                }
            }
        })
        .detach();
    }

    fn start_rename_session(
        &mut self,
        session_id: i64,
        current_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(1)
                .default_value(&current_name)
                .placeholder(t!("AiChat.session_name_placeholder").to_string())
        });

        let panel_entity = cx.entity();
        let input_for_dialog = input_state.clone();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let input_for_ok = input_for_dialog.clone();
            let panel_for_ok = panel_entity.clone();

            dialog
                .title(t!("AiChat.rename_session_title").to_string())
                .w(px(360.0))
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text(t!("Common.save").to_string())
                        .cancel_text(t!("Common.cancel").to_string()),
                )
                .on_ok(move |_, _window, cx| {
                    let new_name = input_for_ok.read(cx).value().to_string();
                    if !new_name.trim().is_empty() {
                        panel_for_ok.update(cx, |this, cx| {
                            this.rename_session(session_id, new_name, cx);
                        });
                    }
                    true
                })
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_sm()
                                .child(t!("AiChat.rename_session_prompt").to_string()),
                        )
                        .child(Input::new(&input_for_dialog).w_full()),
                )
        });
    }

    fn rename_session(&mut self, session_id: i64, new_name: String, cx: &mut Context<Self>) {
        let global_state = cx.global::<GlobalStorageState>();
        let storage_manager = global_state.storage.clone();

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let renamed = {
                let session_repo = match storage_manager.get::<SessionRepository>() {
                    Some(r) => r,
                    None => return,
                };
                if let Ok(Some(mut session)) = session_repo.get(session_id) {
                    session.name = new_name;
                    session_repo.update(&session).is_ok()
                } else {
                    false
                }
            };

            if renamed {
                if let Some(entity) = this.upgrade() {
                    let _ = cx.update(|cx| {
                        entity.update(cx, |this, cx| {
                            this.load_history_sessions(cx);
                        });
                    });
                }
            }
        })
        .detach();
    }

    fn load_session(&mut self, session_id: i64, cx: &mut Context<Self>) {
        let global_state = cx.global::<GlobalStorageState>();
        let storage_manager = global_state.storage.clone();

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let messages = {
                let message_repo = match storage_manager.get::<MessageRepository>() {
                    Some(r) => r,
                    None => return,
                };
                match message_repo.list_by_session(session_id) {
                    Ok(m) => m,
                    Err(_) => return,
                }
            };

            if let Some(entity) = this.upgrade() {
                let _ = cx.update(|cx| {
                    entity.update(cx, |this, cx| {
                        this.engine.session_id = Some(session_id);
                        this.engine.messages = ChatEngine::messages_from_history(&messages);
                        this.session_affinity.reset();
                        this.history_popover_open = false;
                        cx.notify();
                    });
                });
            }
        })
        .detach();
    }

    fn send_message(&mut self, content: String, cx: &mut Context<Self>) {
        if content.trim().is_empty() || self.engine.is_loading {
            return;
        }

        // Agent 调度模式：走 AgentDispatcher 路由（支持工具调用）
        if self.agent_mode {
            self.send_via_agent(content, cx);
            return;
        }

        let Some(provider_id_str) = self.engine.provider_id.clone() else {
            self.engine
                .push_assistant(t!("AiChat.select_provider_first").to_string());
            cx.notify();
            return;
        };

        let provider_id: i64 = match provider_id_str.parse() {
            Ok(id) => id,
            Err(_) => {
                self.engine
                    .push_assistant(t!("AiChat.invalid_provider_id").to_string());
                cx.notify();
                return;
            }
        };

        // 确保会话存在并持久化用户消息
        if let Some(session_id) = self.ensure_session_id(&provider_id_str, cx) {
            self.persist_user_message(session_id, &content, cx);
        }

        let global_provider_state = cx.global::<GlobalProviderState>().clone();
        let global_state = cx.global::<GlobalStorageState>();
        let storage_manager = global_state.storage.clone();
        let session_id = self.engine.session_id;
        let history_count = self.engine.model_settings.history_count;
        let max_tokens = self.engine.model_settings.max_tokens;
        let temperature = self.engine.model_settings.temperature;
        let system_instruction = self.system_instruction.clone();

        // 获取用户选择的模型
        let selected_model = self.engine.selected_model.clone().unwrap_or_else(|| {
            self.engine
                .provider_configs
                .iter()
                .find(|c| c.id == provider_id)
                .map(|c| c.model.clone())
                .unwrap_or_default()
        });

        // 添加用户消息到 UI 并创建助手消息占位符
        self.engine.push_user_message(content.clone());
        let assistant_msg_id = self.engine.push_streaming_assistant();

        self.engine.auto_scroll_enabled = true;
        self.engine.is_loading = true;

        // 创建取消令牌
        let cancel_token = CancellationToken::new();
        self.engine.cancel_token = Some(cancel_token.clone());

        self.engine.scroll_to_bottom();
        cx.notify();

        // ChatStreamProcessor::start 内部用 tokio::spawn 把流式请求派到
        // tokio worker 线程（worker 自带 reactor 上下文），无需主线程 enter()。

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            if cancel_token.is_cancelled() {
                return;
            }

            // 构建聊天历史消息
            let messages: Vec<Message> = {
                let mut messages = if let Some(sid) = session_id {
                    if let Some(message_repo) = storage_manager.get::<MessageRepository>() {
                        match message_repo.list_by_session(sid) {
                            Ok(messages) => {
                                let mut msgs: Vec<Message> = messages
                                    .iter()
                                    .map(|msg| {
                                        let role = match msg.role.as_str() {
                                            "user" => Role::User,
                                            "assistant" => Role::Assistant,
                                            "system" => Role::System,
                                            _ => Role::User,
                                        };
                                        Message::text(role, &msg.content)
                                    })
                                    .collect();
                                // 限制历史条数
                                if msgs.len() > history_count {
                                    msgs = msgs.split_off(msgs.len() - history_count);
                                }
                                msgs
                            }
                            Err(_) => vec![Message::text(Role::User, &content)],
                        }
                    } else {
                        vec![Message::text(Role::User, &content)]
                    }
                } else {
                    vec![Message::text(Role::User, &content)]
                };

                if let Some(instruction) = system_instruction.as_deref() {
                    messages.insert(0, Message::text(Role::System, instruction));
                }

                messages
            };

            // 直接使用 ChatStreamProcessor 进行流式对话
            let mut rx = match ChatStreamProcessor::start(
                provider_id,
                Some(selected_model),
                messages,
                max_tokens as u32,
                temperature,
                cancel_token,
                global_provider_state,
                storage_manager.clone(),
            )
            .await
            {
                Ok(rx) => rx,
                Err(e) => {
                    if let Some(entity) = this.upgrade() {
                        let msg_id = assistant_msg_id.clone();
                        let error_msg = e.to_string();
                        let _ = cx.update(|cx| {
                            entity.update(cx, |this, cx| {
                                this.engine.set_message_error(&msg_id, error_msg);
                                this.engine.is_loading = false;
                                this.engine.cancel_token = None;
                                cx.notify();
                            });
                        });
                    }
                    return;
                }
            };

            // 处理流式事件
            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::ContentDelta { full_content, .. } => {
                        if let Some(entity) = this.upgrade() {
                            let msg_id = assistant_msg_id.clone();
                            cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    this.engine.update_streaming_content(&msg_id, full_content);
                                    this.engine.scroll_to_bottom();
                                    cx.notify();
                                })
                            });
                        } else {
                            return;
                        }
                    }
                    StreamEvent::ReasoningDelta { full_reasoning, .. } => {
                        if let Some(entity) = this.upgrade() {
                            let msg_id = assistant_msg_id.clone();
                            cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    this.engine
                                        .update_streaming_reasoning(&msg_id, full_reasoning);
                                    this.engine.scroll_to_bottom();
                                    cx.notify();
                                })
                            });
                        } else {
                            return;
                        }
                    }
                    StreamEvent::Completed { full_content } => {
                        if let Some(entity) = this.upgrade() {
                            let msg_id = assistant_msg_id.clone();
                            let storage_for_save = storage_manager.clone();
                            cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    this.engine
                                        .finalize_streaming(&msg_id, full_content.clone());
                                    this.engine.is_loading = false;
                                    this.engine.cancel_token = None;
                                    this.engine.scroll_to_bottom();

                                    // 持久化助手消息
                                    if let Some(sid) = session_id {
                                        let content_to_save = full_content;
                                        let storage = storage_for_save;
                                        cx.spawn(async move |_this, _cx: &mut AsyncApp| {
                                            if let Some(repo) = storage.get::<MessageRepository>() {
                                                let mut msg = ChatMessage::new(
                                                    sid,
                                                    "assistant".to_string(),
                                                    content_to_save,
                                                );
                                                if let Err(e) = repo.insert(&mut msg) {
                                                    warn!(
                                                        "Failed to save assistant message: {}",
                                                        e
                                                    );
                                                }
                                            }
                                        })
                                        .detach();
                                    }

                                    cx.notify();
                                })
                            });
                        }
                        break;
                    }
                    StreamEvent::Error { message } => {
                        if let Some(entity) = this.upgrade() {
                            let msg_id = assistant_msg_id.clone();
                            cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    this.engine.set_message_error(&msg_id, message);
                                    this.engine.is_loading = false;
                                    this.engine.cancel_token = None;
                                    this.engine.scroll_to_bottom();
                                    cx.notify();
                                })
                            });
                        }
                        break;
                    }
                    StreamEvent::Cancelled => {
                        info!("Stream cancelled by user");
                        if let Some(entity) = this.upgrade() {
                            let _ = cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    this.engine.is_loading = false;
                                    this.engine.cancel_token = None;
                                    cx.notify();
                                });
                            });
                        }
                        break;
                    }
                }
            }
        })
        .detach();
    }

    /// Agent 调度模式入口：通过 AgentDispatcher 路由并消费 AgentEvent 流
    fn send_via_agent(&mut self, content: String, cx: &mut Context<Self>) {
        let Some(provider_id_str) = self.engine.provider_id.clone() else {
            self.engine
                .push_assistant(t!("AiChat.select_provider_first").to_string());
            cx.notify();
            return;
        };

        let provider_id: i64 = match provider_id_str.parse() {
            Ok(id) => id,
            Err(_) => {
                self.engine
                    .push_assistant(t!("AiChat.invalid_provider_id").to_string());
                cx.notify();
                return;
            }
        };

        // 确保会话存在并持久化用户消息
        if let Some(session_id) = self.ensure_session_id(&provider_id_str, cx) {
            self.persist_user_message(session_id, &content, cx);
        }

        let global_provider_state = cx.global::<GlobalProviderState>().clone();
        let storage_manager = cx.global::<GlobalStorageState>().storage.clone();
        let session_id = self.engine.session_id;
        let history_count = self.engine.model_settings.history_count;
        let provider_config = self.build_agent_provider_config(provider_id);

        // 添加用户消息到 UI 并创建助手消息占位符
        self.engine.push_user_message(content.clone());
        let assistant_msg_id = self.engine.push_streaming_assistant();

        self.engine.auto_scroll_enabled = true;
        self.engine.is_loading = true;

        let cancel_token = CancellationToken::new();
        self.engine.cancel_token = Some(cancel_token.clone());
        self.engine.scroll_to_bottom();
        cx.notify();

        let registry = cx.global::<AgentRegistry>().clone();
        let mut affinity = self.session_affinity.clone();

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            // 构建聊天历史（agent 自带系统提示词，不注入 system_instruction）
            let history = Self::build_agent_history_messages(
                &storage_manager,
                session_id,
                history_count,
                &content,
            );

            let mut ctx_agent = AgentContext::new(
                content,
                history,
                provider_config,
                global_provider_state,
                storage_manager,
                cancel_token,
            )
            .with_session_id(session_id);

            // 注入外部能力（如终端操作句柄）
            if let Some(entity) = this.upgrade() {
                cx.update(|cx| {
                    entity.update(cx, |this, _cx| {
                        for injector in &this.capability_injectors {
                            injector(&mut ctx_agent);
                        }
                    });
                });
            }

            // 关键：把 dispatch 整体放到 tokio worker 线程上执行。
            //
            // 背景：AgentDispatcher::dispatch → IntentRouter::route 会在调用线程上
            // 同步 .await provider.chat()，最终走到 reqwest 的 tokio::time::sleep，
            // 该调用要求"当前线程处于 tokio reactor 上下文"。GPUI 主线程（foreground
            // executor）不在 tokio runtime 内，没有 reactor 上下文——直接在主线程
            // await 会触发 "there is no reactor running" panic。
            //
            // 旧代码用 `tokio_handle.enter()` 在主线程伪注入 reactor 上下文来规避，
            // 但 EnterGuard 是 thread-local 且要求 LIFO 释放；本 async block 内存在
            // 嵌套的 cx.spawn 子任务（保存助手消息），子任务也在主线程 poll，会
            // 破坏 EnterGuard 的释放顺序，触发 "dropped out of order" panic。
            // 多 AI 助手并发时这一冲突尤其频繁。
            //
            // 正解：用 Tokio::spawn（通过全局 runtime Handle 提交，不依赖 current
            // context）把 dispatch 派到 tokio multi-threaded runtime 的 worker 线程。
            // worker 线程自带 reactor 上下文，reqwest 的 sleep 正常工作；dispatch
            // 内部既有/后续的 tokio::spawn 也在 worker 上 Handle::current() 成功。
            // registry / affinity / ctx_agent 均为 owned 且 Send，可安全跨线程 move。
            // 返回的 mpsc::Receiver<AgentEvent> 是 Send，回传主线程消费即可。
            let dispatch_task = Tokio::spawn(cx, async move {
                let mut rx = AgentDispatcher::dispatch(ctx_agent, &registry, &mut affinity).await;
                (rx, affinity)
            });

            let (mut rx, affinity) = match dispatch_task.await {
                Ok((rx, affinity)) => (rx, affinity),
                Err(join_err) => {
                    if let Some(entity) = this.upgrade() {
                        let msg_id = assistant_msg_id.clone();
                        let error_msg = format!("Agent 调度任务失败: {join_err}");
                        let _ = cx.update(|cx| {
                            entity.update(cx, |this, cx| {
                                this.engine.set_message_error(&msg_id, error_msg);
                                this.engine.is_loading = false;
                                this.engine.cancel_token = None;
                                cx.notify();
                            });
                        });
                    }
                    return;
                }
            };

            // 回写亲和性状态
            if let Some(entity) = this.upgrade() {
                let affinity_clone = affinity.clone();
                cx.update(|cx| {
                    entity.update(cx, |this, _cx| {
                        this.session_affinity = affinity_clone;
                    });
                });
            }

            // __AGENT_EVENT_LOOP__
            let mut full_content = String::new();
            let mut full_reasoning = String::new();
            while let Some(event) = rx.recv().await {
                match event {
                    AgentEvent::Progress(stage) => {
                        if let Some(entity) = this.upgrade() {
                            let msg_id = assistant_msg_id.clone();
                            cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    if let Some(msg) =
                                        this.engine.messages.iter_mut().find(|m| m.id == msg_id)
                                    {
                                        msg.variant = MessageVariant::Status {
                                            title: stage,
                                            is_done: false,
                                        };
                                    }
                                    this.engine.scroll_to_bottom();
                                    cx.notify();
                                });
                            });
                        } else {
                            return;
                        }
                    }
                    AgentEvent::TextDelta(delta) => {
                        full_content.push_str(&delta);
                        if let Some(entity) = this.upgrade() {
                            let msg_id = assistant_msg_id.clone();
                            let content_clone = full_content.clone();
                            cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    this.engine.update_streaming_content(&msg_id, content_clone);
                                    this.engine.scroll_to_bottom();
                                    cx.notify();
                                });
                            });
                        } else {
                            return;
                        }
                    }
                    AgentEvent::ReasoningDelta(delta) => {
                        full_reasoning.push_str(&delta);
                        if let Some(entity) = this.upgrade() {
                            let msg_id = assistant_msg_id.clone();
                            let reasoning_clone = full_reasoning.clone();
                            cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    this.engine
                                        .update_streaming_reasoning(&msg_id, reasoning_clone);
                                    cx.notify();
                                });
                            });
                        } else {
                            return;
                        }
                    }
                    AgentEvent::ToolCallStarted {
                        call_id,
                        name,
                        seq,
                        title,
                        args_summary,
                    } => {
                        if let Some(entity) = this.upgrade() {
                            let msg_id = assistant_msg_id.clone();
                            cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    this.insert_tool_call_message(
                                        &msg_id,
                                        call_id,
                                        name,
                                        seq,
                                        title,
                                        args_summary,
                                    );
                                    this.engine.scroll_to_bottom();
                                    cx.notify();
                                });
                            });
                        } else {
                            return;
                        }
                    }
                    AgentEvent::ToolCallFinished {
                        call_id,
                        ok,
                        output,
                    } => {
                        if let Some(entity) = this.upgrade() {
                            cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    this.update_tool_call_message(&call_id, ok, output);
                                    cx.notify();
                                });
                            });
                        } else {
                            return;
                        }
                    }
                    AgentEvent::Completed(result) => {
                        if let Some(entity) = this.upgrade() {
                            // result.content 是权威终态（含 fenced 工具记录）；
                            // 仅在为空时回落到流式累积内容
                            let final_content = if result.content.is_empty() {
                                full_content.clone()
                            } else {
                                result.content
                            };
                            // 实时会话已由独立工具卡片展示，展示内容剥离 fenced 记录块避免重复渲染；
                            // 持久化仍保留完整内容（重载时由卡片渲染器重建）
                            let display_content = strip_tool_record_blocks(&final_content);
                            let msg_id = assistant_msg_id.clone();
                            cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    this.engine.finalize_streaming(&msg_id, display_content);
                                    this.engine.is_loading = false;
                                    this.engine.cancel_token = None;
                                    this.engine.scroll_to_bottom();
                                    // 持久化助手消息（含 fenced 工具记录）
                                    if let Some(sid) = session_id {
                                        this.engine.persist_assistant_message(sid, final_content);
                                    }
                                    cx.notify();
                                });
                            });
                        }
                        break;
                    }
                    AgentEvent::Error(message) => {
                        if let Some(entity) = this.upgrade() {
                            let msg_id = assistant_msg_id.clone();
                            cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    this.engine.set_message_error(&msg_id, message);
                                    this.engine.is_loading = false;
                                    this.engine.cancel_token = None;
                                    this.engine.scroll_to_bottom();
                                    cx.notify();
                                });
                            });
                        }
                        break;
                    }
                    AgentEvent::Cancelled => {
                        info!("Agent run cancelled by user");
                        if let Some(entity) = this.upgrade() {
                            let msg_id = assistant_msg_id.clone();
                            cx.update(|cx| {
                                entity.update(cx, |this, cx| {
                                    if !full_content.is_empty() {
                                        this.engine
                                            .finalize_streaming(&msg_id, full_content.clone());
                                    }
                                    this.engine.is_loading = false;
                                    this.engine.cancel_token = None;
                                    cx.notify();
                                });
                            });
                        }
                        break;
                    }
                }
            }
        })
        .detach();
    }

    /// 构建 Agent 调度使用的 ProviderConfig（基础配置 + 用户选择覆盖）
    fn build_agent_provider_config(&self, provider_id: i64) -> ProviderConfig {
        let base = self
            .engine
            .provider_configs
            .iter()
            .find(|c| c.id == provider_id)
            .cloned()
            .unwrap_or_default();
        ProviderConfig {
            model: self.engine.selected_model.clone().unwrap_or(base.model),
            // 0 表示不限制，传 None 让 Provider 使用默认值
            max_tokens: (self.engine.model_settings.max_tokens > 0)
                .then_some(self.engine.model_settings.max_tokens as i32),
            temperature: Some(self.engine.model_settings.temperature),
            ..base
        }
    }

    /// 构建发送给 Agent 的历史消息（从 DB 读取、按条数截断、去除末尾重复输入）
    ///
    /// 关键：还原结构化工具消息对。DB 中 role='assistant' 且 tool_calls_json 非空的消息
    /// 重建为带 `tool_calls` 的 assistant 消息；role='tool' 的消息重建为带 `tool_call_id`
    /// 的 tool 结果消息。这样下一轮 LLM 请求能拿到完整的工具调用上下文，避免多轮任务
    /// 时模型丢失中间过程而重复执行或答非所问。
    fn build_agent_history_messages(
        storage_manager: &StorageManager,
        session_id: Option<i64>,
        history_count: usize,
        content: &str,
    ) -> Vec<Message> {
        let mut messages: Vec<Message> = match session_id {
            Some(sid) => match storage_manager.get::<MessageRepository>() {
                Some(repo) => match repo.list_by_session(sid) {
                    Ok(rows) => rows.iter().map(chat_message_to_llm_message).collect(),
                    Err(_) => vec![Message::text(Role::User, content)],
                },
                None => vec![Message::text(Role::User, content)],
            },
            None => vec![Message::text(Role::User, content)],
        };

        // 从头部截断到 history_count 条（0 表示不携带历史）
        let keep_from = messages.len().saturating_sub(history_count);
        messages = messages.split_off(keep_from);
        // 去掉末尾与当前输入重复的用户消息（user_input 由 AgentContext 单独携带）
        if messages
            .last()
            .is_some_and(|m| m.role == Role::User && m.content_as_text() == content)
        {
            messages.pop();
        }
        messages
    }

    /// 在流式占位消息之前插入工具调用卡片，保持助手回复始终位于执行过程下方
    fn insert_tool_call_message(
        &mut self,
        assistant_msg_id: &str,
        call_id: String,
        name: String,
        seq: u32,
        title: String,
        args_summary: String,
    ) {
        let msg = ChatMessageUI::tool_call(call_id, name, seq, title, args_summary);
        match self
            .engine
            .messages
            .iter()
            .position(|m| m.id == assistant_msg_id)
        {
            Some(pos) => self.engine.messages.insert(pos, msg),
            None => self.engine.messages.push(msg),
        }
    }

    /// 更新工具调用卡片的最终状态与输出摘要
    fn update_tool_call_message(&mut self, call_id: &str, ok: bool, output: String) {
        for msg in self.engine.messages.iter_mut().rev() {
            if let MessageVariant::ToolCall {
                call_id: id,
                status,
                output: out,
                ..
            } = &mut msg.variant
                && id == call_id
            {
                *status = if ok {
                    ToolCallStatus::Success
                } else {
                    ToolCallStatus::Failed
                };
                *out = output;
                return;
            }
        }
    }

    fn render_header(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = self.border(cx);
        let muted = self.muted(cx);
        let fg = self.foreground(cx);
        let session_list = self.session_list.clone();

        h_flex()
            .flex_shrink_0()
            .w_full()
            .px_1()
            .py_1()
            .border_b_1()
            .border_color(border)
            .bg(muted)
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(fg)
                    .child(t!("AiChat.title").to_string()),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("new-session")
                            .icon(IconName::Plus)
                            .ghost()
                            .small()
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.start_new_session(cx);
                            })),
                    )
                    .child(
                        Popover::new("history-popover")
                            .anchor(Corner::TopRight)
                            .p_0()
                            .open(self.history_popover_open)
                            .on_open_change(cx.listener(|this, open, window, cx| {
                                this.history_popover_open = *open;
                                if *open {
                                    this.update_session_list(window, cx);
                                    this.load_history_sessions(cx);
                                }
                                cx.notify();
                            }))
                            .when_some(session_list.as_ref(), |popover, list| {
                                popover.track_focus(&list.focus_handle(cx))
                            })
                            .trigger(
                                Button::new("history")
                                    .icon(IconName::BookOpen)
                                    .ghost()
                                    .small(),
                            )
                            .when_some(session_list, |popover, list| {
                                popover.child(
                                    List::new(&list)
                                        .w(px(280.0))
                                        .max_h(px(350.0))
                                        .border_1()
                                        .border_color(border)
                                        .rounded(cx.theme().radius),
                                )
                            }),
                    )
                    .child(
                        Button::new("close-panel")
                            .icon(IconName::Close)
                            .ghost()
                            .small()
                            .on_click(cx.listener(|_this, _event, _window, cx| {
                                cx.emit(AiChatPanelEvent::Close);
                            })),
                    ),
            )
    }

    fn render_messages(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let code_block_actions = self.engine.code_block_actions.clone();
        let code_block_renderer = self.code_block_renderer.clone();

        div()
            .id("chat-messages-list")
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .track_scroll(&self.engine.scroll_handle)
            .p_1()
            .pb_4()
            .child(
                v_flex()
                    .w_full()
                    .gap_4()
                    .children(self.engine.messages.iter().map(|msg| {
                        ChatMessageRenderer::render_message(
                            msg,
                            &code_block_actions,
                            code_block_renderer.clone(),
                            window,
                            cx,
                        )
                    })),
            )
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let content = self.ai_input_state.read(cx).value().to_string();
        if content.trim().is_empty() {
            return;
        }
        self.send_message(content, cx);
        self.ai_input_state.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
    }

    fn render_input(&self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = self.border(cx);
        let bg = self.background(cx);
        let muted = self.muted(cx);

        v_flex()
            .flex_shrink_0()
            .w_full()
            .px_1()
            .py_1()
            .gap_1()
            .border_t_1()
            .border_color(border)
            .bg(bg)
            // 输入框
            .child(
                Input::new(&self.ai_input_state)
                    .w_full()
                    .with_size(Size::Large)
                    .bordered(false)
                    .appearance(false)
                    .bg(muted)
                    .rounded(cx.theme().radius),
            )
            // 底部工具栏
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        h_flex()
                            .flex_1()
                            .gap_2()
                            .min_w_0()
                            .overflow_hidden()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(self.provider_select_state.render(cx)),
                            )
                            // 模型设置按钮
                            .child({
                                let settings_panel = self.settings_panel.clone();
                                Popover::new("model-settings-popover")
                                    .anchor(Corner::BottomLeft)
                                    .trigger(
                                        Button::new("model-settings-btn")
                                            .icon(IconName::Settings)
                                            .ghost()
                                            .with_size(Size::Small),
                                    )
                                    .content(move |_state, _window, _cx| settings_panel.clone())
                            }),
                    )
                    .child(if self.can_cancel() {
                        // 加载中显示终止按钮
                        Button::new("cancel")
                            .with_size(Size::Small)
                            .danger()
                            .icon(IconName::CircleX)
                            .label(t!("AiChat.cancel").to_string())
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.cancel_current_operation(cx);
                            }))
                    } else {
                        // 正常状态显示发送按钮
                        Button::new("send")
                            .with_size(Size::Small)
                            .primary()
                            .icon(IconName::ArrowRight)
                            .label(t!("AiChat.send").to_string())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.submit(window, cx);
                            }))
                    }),
            )
    }
}

impl EventEmitter<AiChatPanelEvent> for AiChatPanel {}

impl Focusable for AiChatPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl SessionListHost for AiChatPanel {
    fn on_session_select(&mut self, session_id: i64, cx: &mut Context<Self>) {
        self.load_session(session_id, cx);
    }

    fn on_session_edit(
        &mut self,
        session_id: i64,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_rename_session(session_id, name, window, cx);
    }

    fn on_session_delete(&mut self, session_id: i64, cx: &mut Context<Self>) {
        self.delete_session(session_id, cx);
    }

    fn is_current_session(&self, session_id: i64) -> bool {
        self.engine.session_id == Some(session_id)
    }

    fn on_session_list_confirm(&mut self, cx: &mut Context<Self>) {
        self.history_popover_open = false;
        cx.notify();
    }

    fn on_session_list_cancel(&mut self, cx: &mut Context<Self>) {
        self.history_popover_open = false;
        cx.notify();
    }
}

impl Render for AiChatPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_logged_in = GlobalCloudUser::is_logged_in(cx);
        if is_logged_in != self.is_logged_in {
            self.is_logged_in = is_logged_in;
            self.load_providers(cx);
        }

        let bg_color = self.background(cx);
        let fg_color = self.foreground(cx);

        div().size_full().child(
            v_flex()
                .size_full()
                .bg(bg_color)
                .text_color(fg_color)
                .child(self.render_header(window, cx))
                .child(self.render_messages(window, cx))
                .child(self.render_input(window, cx)),
        )
    }
}

/// 剥离内容中的 omnihub-tool fenced 记录块（实时会话已由独立卡片展示，避免重复渲染）
fn strip_tool_record_blocks(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut in_block = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        if !in_block && trimmed.starts_with("```omnihub-tool") {
            in_block = true;
            continue;
        }
        if in_block {
            if trimmed.starts_with("```") {
                in_block = false;
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// 把持久化的 `ChatMessage` 还原为发送给 LLM 的 `Message`。
///
/// - role='assistant' 且 `tool_calls_json` 非空：重建带 `tool_calls` 的 assistant 消息
///   （OpenAI 协议要求的 assistant tool_calls 消息）；
/// - role='tool'：重建带 `tool_call_id` 的 tool 结果消息；
/// - 其他：按 role 构造普通文本消息。
///
/// `tool_calls_json` 反序列化失败时降级为纯文本 assistant 消息（不丢消息，仅丢工具结构），
/// 并记 warn 日志便于诊断。
fn chat_message_to_llm_message(msg: &ChatMessage) -> Message {
    let role = match msg.role.as_str() {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "system" => Role::System,
        "tool" => Role::Tool,
        _ => Role::User,
    };
    let mut message = Message::text(role, &msg.content);
    if let Some(json) = msg.tool_calls_json.as_deref() {
        match serde_json::from_str::<Vec<ToolCall>>(json) {
            Ok(tool_calls) if !tool_calls.is_empty() => {
                message.tool_calls = Some(tool_calls);
            }
            Ok(_) => {} // 空数组，保持纯文本
            Err(err) => {
                tracing::warn!("反序列化 tool_calls_json 失败，降级为纯文本 assistant 消息: {err}");
            }
        }
    }
    if let Some(call_id) = msg.tool_call_id.as_deref() {
        message.tool_call_id = Some(call_id.to_string());
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_storage() -> StorageManager {
        let path =
            std::env::temp_dir().join(format!("omnihub-ai-chat-test-{}.db", uuid::Uuid::new_v4()));
        StorageManager::with_path(&path).expect("创建测试存储失败")
    }

    /// history_count=0 时不携带任何历史消息
    #[test]
    fn agent_history_count_zero_carries_nothing() {
        let storage = temp_storage();
        let history = AiChatPanel::build_agent_history_messages(&storage, None, 0, "你好");
        assert!(history.is_empty());
    }

    /// 末尾与当前输入重复的用户消息会被去除（user_input 由 AgentContext 单独携带）
    #[test]
    fn agent_history_dedups_trailing_current_input() {
        let storage = temp_storage();
        // 无会话时历史仅含当前输入一条，去重后应为空
        let history = AiChatPanel::build_agent_history_messages(&storage, None, 10, "你好");
        assert!(history.is_empty());
    }

    /// 剥离 omnihub-tool 记录块，保留其余正文
    #[test]
    fn strip_removes_tool_record_blocks() {
        let content = "结论如下\n```omnihub-tool\n{\"name\":\"write\"}\n```\n补充说明";
        assert_eq!(strip_tool_record_blocks(content), "结论如下\n补充说明");
    }

    /// 普通 fenced 代码块不受影响
    #[test]
    fn strip_keeps_normal_code_blocks() {
        let content = "```bash\nls -la\n```";
        assert_eq!(strip_tool_record_blocks(content), "```bash\nls -la\n```");
    }

    /// 未闭合的记录块剥离到文末，且输入无记录块时原样返回
    #[test]
    fn strip_handles_unclosed_block_and_passthrough() {
        assert_eq!(
            strip_tool_record_blocks("前文\n```omnihub-tool\n{\"name\":\"x\"}"),
            "前文"
        );
        assert_eq!(strip_tool_record_blocks("纯文本"), "纯文本");
    }

    /// 工具调用中间态往返回归：run_loop 持久化的 assistant(tool_calls) + tool(result)
    /// 消息对，重建后必须还原为结构等价、顺序正确的消息序列。
    /// 回归背景：created_at 为秒级精度，工具消息对几乎必然同秒落库，
    /// 若排序缺少 id 平局裁决，tool 结果可能先于其 assistant 工具调用消息被重建。
    #[test]
    fn agent_history_roundtrips_structured_tool_messages() {
        use crate::storage::traits::Repository;

        let storage = temp_storage();
        // 复刻 llm::storage::init 的注册；init 依赖 GPUI App 上下文，测试内直接注册等价仓库
        storage.register(SessionRepository::new(storage.connection()));
        storage.register(MessageRepository::new(storage.connection()));
        // chat_messages 外键指向 chat_sessions，先创建会话再落消息
        let session_repo = storage
            .get::<SessionRepository>()
            .expect("SessionRepository 应已注册");
        let mut session = crate::llm::chat_history::ChatSession::new(
            "工具消息对往返测试".to_string(),
            "test-provider".to_string(),
        );
        let sid = session_repo.insert(&mut session).expect("创建会话失败");
        let repo = storage
            .get::<MessageRepository>()
            .expect("MessageRepository 应已注册");

        // 普通用户消息（tool_call_id / tool_calls_json 均为 NULL，覆盖旧行 NULL 回落）
        let mut user_msg = ChatMessage::user(sid, "列出文件".to_string());
        repo.insert(&mut user_msg).expect("插入用户消息失败");

        // 模拟 run_loop 的持久化路径：assistant(tool_calls) → tool(result)
        let tool_calls = vec![ToolCall {
            id: "call-1".to_string(),
            call_type: "function".to_string(),
            function: llm_connector::types::FunctionCall {
                name: "write_to_terminal".to_string(),
                arguments: r#"{"command":"ls -la"}"#.to_string(),
                ..Default::default()
            },
            ..Default::default()
        }];
        let tool_calls_json = serde_json::to_string(&tool_calls).expect("序列化失败");
        let mut assistant_msg =
            ChatMessage::assistant_tool_calls(sid, "执行写入".to_string(), tool_calls_json);
        repo.insert(&mut assistant_msg)
            .expect("插入 assistant 消息失败");
        let mut tool_msg =
            ChatMessage::tool_result(sid, "call-1".to_string(), "total 2".to_string());
        repo.insert(&mut tool_msg).expect("插入 tool 消息失败");

        let history =
            AiChatPanel::build_agent_history_messages(&storage, Some(sid), 10, "当前输入");

        assert_eq!(
            history.len(),
            3,
            "应重建出 user + assistant + tool 三条消息"
        );

        // 普通文本消息不受影响
        assert_eq!(history[0].role, Role::User);
        assert_eq!(history[0].content_as_text(), "列出文件");
        assert!(history[0].tool_calls.is_none());

        // assistant 消息还原出结构化 tool_calls
        assert_eq!(history[1].role, Role::Assistant);
        let calls = history[1].tool_calls.as_ref().expect("应还原 tool_calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call-1");
        assert_eq!(calls[0].function.name, "write_to_terminal");
        assert_eq!(calls[0].function.arguments, r#"{"command":"ls -la"}"#);

        // tool 结果消息还原 tool_call_id，且顺序在 assistant 之后
        assert_eq!(history[2].role, Role::Tool);
        assert_eq!(history[2].tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(history[2].content_as_text(), "total 2");
    }
}

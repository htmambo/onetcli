//! AI 终端操作员的全局终端注册表。
//!
//! `TerminalView` 首次渲染时注册（并持续刷新窗口句柄），
//! 失活的弱引用在每次访问时惰性清扫。

use gpui::{AnyWindowHandle, App, AppContext, Entity, Global, WeakEntity};
use terminal::terminal::{Terminal, TerminalConnectionKind};

use crate::view::TerminalView;

/// 面向 Agent 的终端摘要信息。
///
/// `is_focused` 表示该终端是否就是"用户最近一次主动激活"的终端
/// （点进终端内容区 / dock tab 切换到本 tab / AI 调 focus_terminal
/// 工具）；**不**等同于 GPUI focus 当前所在——AI 侧栏抢焦点时
/// 所有 TerminalView 的 `is_focused` 都会是 false（如果用户切回 AI
/// 面板前没有点过任何终端），但用户点过的那个仍是 true。
#[derive(Debug, Clone)]
pub struct TerminalInfo {
    pub id: u64,
    pub title: String,
    pub connection_kind: TerminalConnectionKind,
    pub cwd: Option<String>,
    /// 是否等于 `TerminalViewRegistry.active_terminal_id`。
    pub is_focused: bool,
}

pub(crate) struct RegistryEntry {
    id: u64,
    weak: WeakEntity<TerminalView>,
    window_handle: Option<AnyWindowHandle>,
}

/// 全局终端注册表（GPUI Global）。
///
/// 仅持有弱引用，终端关闭后由 `sweep` 惰性回收。
///
/// 额外跟踪"最近一次主动激活的终端"（`active_terminal_id`）：
/// - 含义：用户最近一次主动点击/聚焦的 TerminalView，或 AI 助手主动调
///   `focus_terminal` 工具选中的终端。**用户在 AI 侧栏打字不会清空它**，
///   因为那是 GPUI focus 的副作用，不是用户对激活终端的撤换。
/// - 写入方：`TerminalView.on_focus` / `TerminalView::on_activate` /
///   `pump::focus_terminal`（AI 焦点工具调用）。
/// - 读取方：`snapshot` 把这一字段转译为 `TerminalInfo.is_focused` 与
///   `TerminalListSnapshot.focused_id`，供 AI 助手判断"用户最可能想操作哪个"。
#[derive(Default)]
pub struct TerminalViewRegistry {
    entries: Vec<RegistryEntry>,
    next_id: u64,
    active_terminal_id: Option<u64>,
}

impl Global for TerminalViewRegistry {}

impl TerminalViewRegistry {
    /// 初始化全局注册表（幂等）。
    pub fn init(cx: &mut App) {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
    }

    /// 注册新终端或刷新已有条目的窗口句柄，返回稳定的终端 id。
    pub fn register_or_refresh(
        &mut self,
        view: &Entity<TerminalView>,
        window_handle: Option<AnyWindowHandle>,
        known_id: Option<u64>,
    ) -> u64 {
        self.sweep();
        if let Some(id) = known_id {
            if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
                if window_handle.is_some() {
                    entry.window_handle = window_handle;
                }
                return id;
            }
        }
        self.next_id += 1;
        let id = self.next_id;
        self.entries.push(RegistryEntry {
            id,
            weak: view.downgrade(),
            window_handle,
        });
        id
    }

    /// 当前存活终端的摘要列表。
    ///
    /// `is_focused` 等于"该终端 id 是否就是 `active_terminal_id`"，反映
    /// 用户最近一次主动激活，而非 GPUI focus 当前所在——AI 侧栏抢焦点
    /// 时 GPUI focus 跑到了侧栏 input，但仍报告"用户上次点过的那个终端"。
    pub fn snapshot(&mut self, cx: &mut App) -> Vec<TerminalInfo> {
        self.sweep();
        let active = self.active_terminal_id;
        self.entries
            .iter()
            .filter_map(|entry| {
                let view = entry.weak.upgrade()?;
                let terminal = view.read(cx).terminal();
                let terminal = terminal.read(cx);
                Some(TerminalInfo {
                    id: entry.id,
                    title: terminal_title(terminal),
                    connection_kind: terminal.connection_kind(),
                    cwd: terminal.latest_working_dir(),
                    is_focused: active == Some(entry.id),
                })
            })
            .collect()
    }

    /// 查询终端弱引用与窗口句柄。
    pub(crate) fn get(
        &self,
        id: u64,
    ) -> Option<(WeakEntity<TerminalView>, Option<AnyWindowHandle>)> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| (entry.weak.clone(), entry.window_handle))
    }

    /// 标记某个终端为"最近一次主动激活"。
    ///
    /// 仅当 id 在当前 entries 里查得到时才生效；传入未注册 id 一律忽略，
    /// 避免 AI 助手使用过期 id 时把状态写到不存在的实体上。
    pub fn set_active(&mut self, id: u64) {
        if self.entries.iter().any(|entry| entry.id == id) {
            self.active_terminal_id = Some(id);
        }
    }

    /// 清空"最近一次主动激活"标记（外部 UI 主动撤换时使用，目前未挂 UI）。
    pub fn clear_active(&mut self) {
        self.active_terminal_id = None;
    }

    /// 读取当前激活 id（供 snapshot 之外的快速路径，例如单元测试）。
    pub fn active_id(&self) -> Option<u64> {
        self.active_terminal_id
    }

    /// 惰性回收已释放的终端条目。
    ///
    /// 若 `active_terminal_id` 指向被回收的条目，一并清空，避免 list_terminals
    /// 返回一个指向已关闭终端的 focused_id（会让 AI 助手下一次工具调用失败）。
    fn sweep(&mut self) {
        self.entries.retain(|entry| entry.weak.upgrade().is_some());
        if let Some(active) = self.active_terminal_id {
            if !self.entries.iter().any(|entry| entry.id == active) {
                self.active_terminal_id = None;
            }
        }
    }
}

/// 终端展示标题：优先连接名，回退到终端自报标题。
fn terminal_title(terminal: &Terminal) -> String {
    terminal
        .connection_name()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| terminal.title().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_is_empty() {
        let registry = TerminalViewRegistry::default();
        assert!(registry.get(1).is_none());
        assert_eq!(registry.active_id(), None);
    }

    #[test]
    fn set_active_ignores_unknown_id() {
        let mut registry = TerminalViewRegistry::default();
        // 没有 register 直接 set：应保持 None（避免外部脏数据写入）
        registry.set_active(42);
        assert_eq!(registry.active_id(), None);
    }

    #[test]
    fn sweep_clears_active_when_target_entry_dies() {
        // 构造一个 entries 项 id=1，但其 weak 引用已释放，模拟"终端关闭"；
        // set_active(1) 之后再 sweep，应自动清空 active_terminal_id。
        let mut registry = TerminalViewRegistry::default();
        // 直接 push 一个永远 upgrade 不出来的弱引用：
        // WeakEntity::new_invalid() 返回"无效" weak，upgrade() 必然返回 None。
        registry.entries.push(RegistryEntry {
            id: 1,
            weak: WeakEntity::new_invalid(),
            window_handle: None,
        });
        registry.set_active(1);
        assert_eq!(registry.active_id(), Some(1));
        // 触发 sweep：id=1 的 entry 被回收，active 跟着清空。
        registry.sweep();
        assert_eq!(registry.active_id(), None);
        assert!(registry.entries.is_empty());
    }

    #[test]
    fn clear_active_is_idempotent() {
        let mut registry = TerminalViewRegistry::default();
        registry.clear_active(); // 空状态调用不 panic
        assert_eq!(registry.active_id(), None);
        registry.clear_active();
        assert_eq!(registry.active_id(), None);
    }
}

//! AI 终端操作员的全局终端注册表。
//!
//! `TerminalView` 首次渲染时注册（并持续刷新窗口句柄），
//! 失活的弱引用在每次访问时惰性清扫。

use gpui::{AnyWindowHandle, App, Entity, Global, WeakEntity};
use terminal::terminal::{Terminal, TerminalConnectionKind};

use crate::view::TerminalView;

/// 面向 Agent 的终端摘要信息。
#[derive(Debug, Clone)]
pub struct TerminalInfo {
    pub id: u64,
    pub title: String,
    pub connection_kind: TerminalConnectionKind,
    pub cwd: Option<String>,
}

pub(crate) struct RegistryEntry {
    id: u64,
    weak: WeakEntity<TerminalView>,
    window_handle: Option<AnyWindowHandle>,
}

/// 全局终端注册表（GPUI Global）。
///
/// 仅持有弱引用，终端关闭后由 `sweep` 惰性回收。
#[derive(Default)]
pub struct TerminalViewRegistry {
    entries: Vec<RegistryEntry>,
    next_id: u64,
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
    pub fn snapshot(&mut self, cx: &App) -> Vec<TerminalInfo> {
        self.sweep();
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

    /// 惰性回收已释放的终端条目。
    fn sweep(&mut self) {
        self.entries.retain(|entry| entry.weak.upgrade().is_some());
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
    }
}

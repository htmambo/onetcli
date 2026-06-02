use gpui::{App, Entity, Window};
use one_core::tab_container::{TabContainer, TabContentRegistry};
use one_core::tab_persistence::load_tab_state;

/// 加载启动时标签布局，并跳过连接恢复占位标签。
pub fn load_startup_tabs_without_connection_restore(
    tab_container: &Entity<TabContainer>,
    registry: &TabContentRegistry,
    window: &mut Window,
    cx: &mut App,
) -> anyhow::Result<usize> {
    let mut state = load_tab_state()?;
    crate::connection_restore::strip_restorable_tabs_from_tab_state(&mut state);

    let active_index = state.active_index;
    if state.tabs.is_empty() {
        return Ok(0);
    }

    tab_container.update(cx, |container, cx| {
        container.load(state, registry, window, cx);
    });

    Ok(active_index)
}

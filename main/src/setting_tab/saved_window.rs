//! 主窗口尺寸与位置的持久化模型。
//!
//! 抽取自 `setting_tab.rs`（轮 4 重构）。`SavedWindowBounds` 与
//! `SavedWindowDisplayState` 通过父模块以 `pub(crate) use` 重导出，
//! 维持 `crate::setting_tab::SavedWindowBounds` 外部引用路径不变。
//!
//! 父模块（`AppSettings`）或父模块测试调用的方法升级为 `pub(super)`；
//! 仅供本模块内部使用的辅助方法/函数保持私有。

use gpui::{App, Bounds, Pixels, Size, WindowBounds, point, px, size};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SavedWindowDisplayState {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SavedWindowBounds {
    pub state: SavedWindowDisplayState,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

fn centered_bounds_in_visible_area(
    requested_size: Size<Pixels>,
    visible_bounds: Bounds<Pixels>,
) -> Bounds<Pixels> {
    let centered_size = size(
        requested_size.width.min(visible_bounds.size.width),
        requested_size.height.min(visible_bounds.size.height),
    );
    Bounds::centered_at(visible_bounds.center(), centered_size)
}

pub(super) fn centered_window_bounds_within_visible_area(
    requested_size: Size<Pixels>,
    visible_bounds: Option<Bounds<Pixels>>,
) -> WindowBounds {
    let bounds = visible_bounds
        .map(|visible_bounds| centered_bounds_in_visible_area(requested_size, visible_bounds))
        .unwrap_or_else(|| Bounds {
            origin: point(px(0.0), px(0.0)),
            size: requested_size,
        });
    WindowBounds::Windowed(bounds)
}

impl SavedWindowBounds {
    fn from_bounds(state: SavedWindowDisplayState, bounds: Bounds<Pixels>) -> Option<Self> {
        let saved = Self {
            state,
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        };

        saved.is_valid().then_some(saved)
    }

    pub(super) fn from_window_bounds(window_bounds: WindowBounds) -> Option<Self> {
        match window_bounds {
            WindowBounds::Windowed(bounds) => {
                Self::from_bounds(SavedWindowDisplayState::Windowed, bounds)
            }
            WindowBounds::Maximized(bounds) => {
                Self::from_bounds(SavedWindowDisplayState::Maximized, bounds)
            }
            WindowBounds::Fullscreen(bounds) => {
                Self::from_bounds(SavedWindowDisplayState::Fullscreen, bounds)
            }
        }
    }

    fn is_valid(&self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
    }

    fn build_window_bounds(state: SavedWindowDisplayState, bounds: Bounds<Pixels>) -> WindowBounds {
        match state {
            SavedWindowDisplayState::Windowed => WindowBounds::Windowed(bounds),
            SavedWindowDisplayState::Maximized => WindowBounds::Maximized(bounds),
            SavedWindowDisplayState::Fullscreen => WindowBounds::Fullscreen(bounds),
        }
    }

    pub(super) fn to_window_bounds(self) -> Option<WindowBounds> {
        if !self.is_valid() {
            return None;
        }

        let bounds = Bounds {
            origin: point(px(self.x), px(self.y)),
            size: size(px(self.width), px(self.height)),
        };

        Some(Self::build_window_bounds(self.state, bounds))
    }

    pub(super) fn fit_in_visible_bounds(
        self,
        visible_bounds: Bounds<Pixels>,
    ) -> Option<WindowBounds> {
        let restored_window_bounds = self.to_window_bounds()?;
        let restored_bounds = restored_window_bounds.get_bounds();

        if restored_bounds.is_contained_within(&visible_bounds) {
            return Some(restored_window_bounds);
        }

        let centered_bounds = centered_bounds_in_visible_area(restored_bounds.size, visible_bounds);
        Some(Self::build_window_bounds(self.state, centered_bounds))
    }

    pub(super) fn to_restored_window_bounds(self, cx: &App) -> Option<WindowBounds> {
        let restored_window_bounds = self.to_window_bounds()?;
        let restored_bounds = restored_window_bounds.get_bounds();

        if cx
            .displays()
            .into_iter()
            .any(|display| restored_bounds.is_contained_within(&display.visible_bounds()))
        {
            return Some(restored_window_bounds);
        }

        cx.primary_display()
            .map(|display| display.visible_bounds())
            .and_then(|visible_bounds| self.fit_in_visible_bounds(visible_bounds))
            .or(Some(restored_window_bounds))
    }
}

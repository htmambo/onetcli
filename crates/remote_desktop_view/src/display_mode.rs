//! 远程桌面显示模式：决定远端帧如何映射到本地视图区域。
//!
//! 每种模式同时定义两件事：
//! - 渲染时的 `ObjectFit`（图像如何放进视图）
//! - 指针坐标映射方式（本地点击如何换算成远端坐标）

use gpui::ObjectFit;
use rust_i18n::t;

/// 显示模式。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayMode {
    /// 等比缩放完整可见（letterbox 黑边，不变形）。默认。
    #[default]
    Contain,
    /// 原始尺寸 1:1（超出窗口可滚动）。
    Original,
    /// 等比缩放填满窗口（裁剪超出，无黑边）。
    Cover,
    /// 拉伸变形铺满（历史默认行为）。
    Fill,
}

impl DisplayMode {
    /// 全部模式（用于切换控件）。
    pub const ALL: [DisplayMode; 4] = [
        DisplayMode::Contain,
        DisplayMode::Original,
        DisplayMode::Cover,
        DisplayMode::Fill,
    ];

    /// 渲染用的 `ObjectFit`。`Original` 用 `None`（保持原始尺寸）。
    pub fn object_fit(self) -> ObjectFit {
        match self {
            DisplayMode::Contain => ObjectFit::Contain,
            DisplayMode::Original => ObjectFit::None,
            DisplayMode::Cover => ObjectFit::Cover,
            DisplayMode::Fill => ObjectFit::Fill,
        }
    }

    /// 稳定标识（用于元素 id）。
    pub fn id_key(self) -> &'static str {
        match self {
            DisplayMode::Contain => "contain",
            DisplayMode::Original => "original",
            DisplayMode::Cover => "cover",
            DisplayMode::Fill => "fill",
        }
    }

    /// i18n 标签（rust_i18n `t!` 需字面量 key，故逐分支返回）。
    pub fn label(self) -> String {
        match self {
            DisplayMode::Contain => t!("RemoteDesktop.display_mode.contain").to_string(),
            DisplayMode::Original => t!("RemoteDesktop.display_mode.original").to_string(),
            DisplayMode::Cover => t!("RemoteDesktop.display_mode.cover").to_string(),
            DisplayMode::Fill => t!("RemoteDesktop.display_mode.fill").to_string(),
        }
    }

    /// 是否为可滚动的原始尺寸模式（需要滚动容器与滚动偏移校正）。
    pub fn is_scrollable(self) -> bool {
        matches!(self, DisplayMode::Original)
    }
}

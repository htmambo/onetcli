//! 间距 Token
//!
//! 统一管理所有间距值。组件应使用此 enum 而非硬编码像素值。

use gpui::Pixels;

/// 间距 enum — 基于 4px 基准网格
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spacing {
    /// 4px
    Unit1 = 4,
    /// 8px
    Unit2 = 8,
    /// 12px
    Unit3 = 12,
    /// 16px
    Unit4 = 16,
    /// 20px
    Unit5 = 20,
    /// 24px
    Unit6 = 24,
    /// 32px
    Unit8 = 32,
    /// 40px
    Unit10 = 40,
    /// 48px
    Unit12 = 48,
    /// 64px
    Unit16 = 64,
}

impl Spacing {
    /// 转换为 Pixels
    pub fn px(self) -> Pixels {
        Pixels::from(self as i32 as f32)
    }

    /// 直接数值（用于需要 f32 的场景）
    pub fn value(self) -> f32 {
        self as i32 as f32
    }
}

// === 布局间距常量 ===

/// 面板间隙
pub const PANEL_GAP: f32 = 1.0;
/// 内容区块间隙
pub const SECTION_GAP: f32 = 16.0;
/// 页面边距
pub const PAGE_PADDING: f32 = 16.0;

/// 侧边栏宽度
pub const SIDEBAR_WIDTH: f32 = 255.0;
pub const SIDEBAR_MIN_WIDTH: f32 = 200.0;
pub const SIDEBAR_MAX_WIDTH: f32 = 400.0;

/// 数据库对象树宽度
pub const TREE_PANEL_WIDTH: f32 = 250.0;
pub const TREE_PANEL_MIN_WIDTH: f32 = 150.0;
pub const TREE_PANEL_MAX_WIDTH: f32 = 500.0;

/// 聊天侧边栏宽度
pub const CHAT_SIDEBAR_WIDTH: f32 = 360.0;
pub const CHAT_SIDEBAR_MIN_WIDTH: f32 = 320.0;
pub const CHAT_SIDEBAR_MAX_WIDTH: f32 = 600.0;

/// 工具栏高度
pub const TOOLBAR_HEIGHT: f32 = 36.0;
/// 工具栏按钮尺寸
pub const TOOLBAR_BUTTON_SIZE: f32 = 28.0;

/// 标题栏高度
pub const TITLE_BAR_HEIGHT: f32 = 38.0;

/// 面板最小尺寸
pub const PANEL_MIN_SIZE: f32 = 100.0;

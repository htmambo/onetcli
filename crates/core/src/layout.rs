//! 全局布局常量
//!
//! 侧边栏、工具栏等通用布局尺寸的统一定义。
//! 所有值来源于 gpui_component::tokens::spacing。

use gpui::{Pixels, px};
use gpui_component::tokens::spacing::{
    SIDEBAR_WIDTH, SIDEBAR_MIN_WIDTH as SIDEBAR_MIN_VAL, SIDEBAR_MAX_WIDTH as SIDEBAR_MAX_VAL,
    TREE_PANEL_WIDTH, TREE_PANEL_MIN_WIDTH as TREE_PANEL_MIN_VAL, TREE_PANEL_MAX_WIDTH as TREE_PANEL_MAX_VAL,
    CHAT_SIDEBAR_WIDTH, CHAT_SIDEBAR_MIN_WIDTH as CHAT_SIDEBAR_MIN_VAL, CHAT_SIDEBAR_MAX_WIDTH as CHAT_SIDEBAR_MAX_VAL,
    TOOLBAR_HEIGHT, TOOLBAR_BUTTON_SIZE,
    TITLE_BAR_HEIGHT, PANEL_MIN_SIZE as PANEL_MIN_SIZE_VAL,
};

/// 侧边栏默认宽度
pub const SIDEBAR_DEFAULT_WIDTH: Pixels = px(SIDEBAR_WIDTH);
/// 侧边栏最小宽度
pub const SIDEBAR_MIN_WIDTH: Pixels = px(SIDEBAR_MIN_VAL);
/// 侧边栏最大宽度
pub const SIDEBAR_MAX_WIDTH: Pixels = px(SIDEBAR_MAX_VAL);
/// 工具栏宽度
pub const TOOLBAR_WIDTH: Pixels = px(TOOLBAR_HEIGHT);

/// 数据库对象树默认宽度
pub const TREE_PANEL_DEFAULT_SIZE: Pixels = px(TREE_PANEL_WIDTH);
/// 数据库对象树最小宽度
pub const TREE_PANEL_MIN_SIZE: Pixels = px(TREE_PANEL_MIN_VAL);
/// 数据库对象树最大宽度
pub const TREE_PANEL_MAX_SIZE: Pixels = px(TREE_PANEL_MAX_VAL);

/// 聊天侧边栏默认宽度
pub const CHAT_SIDEBAR_DEFAULT_WIDTH: Pixels = px(CHAT_SIDEBAR_WIDTH);
/// 聊天侧边栏最小宽度
pub const CHAT_SIDEBAR_MIN_WIDTH: Pixels = px(CHAT_SIDEBAR_MIN_VAL);
/// 聊天侧边栏最大宽度
pub const CHAT_SIDEBAR_MAX_WIDTH: Pixels = px(CHAT_SIDEBAR_MAX_VAL);

/// 面板最小尺寸
pub const PANEL_MIN_SIZE: Pixels = px(PANEL_MIN_SIZE_VAL);

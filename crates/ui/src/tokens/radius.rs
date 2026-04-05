//! 圆角 Token
//!
//! 统一管理 border-radius 值。

use gpui::Pixels;

/// 圆角 enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radius {
    /// 0px — 无圆角
    None,
    /// 4px — 小按钮、标签
    Sm,
    /// 6px — 输入框、下拉框（默认）
    Md,
    /// 8px — 卡片、面板
    Lg,
    /// 12px — 模态框
    Xl,
    /// 全圆角 — 头像、徽章
    Full,
}

impl Radius {
    pub fn px(self) -> Pixels {
        Pixels::from(match self {
            Self::None => 0.0,
            Self::Sm => 4.0,
            Self::Md => 6.0,
            Self::Lg => 8.0,
            Self::Xl => 12.0,
            Self::Full => 9999.0,
        })
    }
}

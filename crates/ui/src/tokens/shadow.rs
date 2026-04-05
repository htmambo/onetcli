//! 阴影 Token
//!
//! 统一管理阴影效果值。

use gpui::Pixels;

/// 阴影 token — 表示阴影级别的枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowToken {
    /// 无阴影
    None,
    /// 小阴影 — 用于轻微浮起效果
    Sm,
    /// 中阴影 — 用于卡片、面板
    Md,
    /// 大阴影 — 用于弹窗、浮层
    Lg,
    /// 超大阴影 — 用于模态框
    Xl,
}

impl ShadowToken {
    /// 返回 (blur, offset_y, spread, alpha) 元组
    pub fn values(self) -> (Pixels, Pixels, Pixels, f32) {
        match self {
            Self::None => (Pixels::ZERO, Pixels::ZERO, Pixels::ZERO, 0.0),
            Self::Sm => (Pixels::from(4.0), Pixels::from(1.0), Pixels::ZERO, 0.08),
            Self::Md => (Pixels::from(8.0), Pixels::from(2.0), Pixels::ZERO, 0.12),
            Self::Lg => (Pixels::from(16.0), Pixels::from(4.0), Pixels::ZERO, 0.16),
            Self::Xl => (Pixels::from(24.0), Pixels::from(8.0), Pixels::ZERO, 0.20),
        }
    }
}

//! 运行状态数据结构，用于窗口关闭时统一确认。

use gpui::SharedString;

/// 内容类型标识
#[derive(Clone, PartialEq)]
pub enum RunningKind {
    /// 终端 - 子进程正在运行
    Terminal,
    /// SSH 交互式会话
    Ssh,
    /// SFTP 文件传输
    Sftp,
    /// 数据库事务
    Db,
}

/// 运行状态信息
#[derive(Clone)]
pub struct RunningState {
    pub kind: RunningKind,
    /// 标签标题
    pub title: SharedString,
    /// 具体活动描述，如 "vim 正在运行"、"文件传输进行中 (65%)"
    pub activity: SharedString,
}

impl RunningState {
    /// 创建一个终端运行状态
    pub fn terminal(title: SharedString, activity: SharedString) -> Option<Self> {
        Some(Self {
            kind: RunningKind::Terminal,
            title,
            activity,
        })
    }

    /// 创建一个 SSH 会话运行状态
    pub fn ssh(title: SharedString, activity: SharedString) -> Option<Self> {
        Some(Self {
            kind: RunningKind::Ssh,
            title,
            activity,
        })
    }

    /// 创建一个 SFTP 传输运行状态
    pub fn sftp(title: SharedString, activity: SharedString) -> Option<Self> {
        Some(Self {
            kind: RunningKind::Sftp,
            title,
            activity,
        })
    }

    /// 创建一个数据库运行状态
    pub fn db(title: SharedString, activity: SharedString) -> Option<Self> {
        Some(Self {
            kind: RunningKind::Db,
            title,
            activity,
        })
    }
}

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
    /// 数据库有待提交的变更
    DbPendingChanges,
}

/// 数据库待提交变更的等级（用于确定颜色）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PendingChangeLevel {
    /// 新增记录
    Insert = 1,
    /// 修改记录
    Modify = 2,
    /// 删除记录
    Delete = 3,
}

impl PendingChangeLevel {
    /// 获取对应的国际化 key
    pub fn i18n_key(&self) -> &'static str {
        match self {
            PendingChangeLevel::Delete => "RunningState.db_pending_changes.activity_delete",
            PendingChangeLevel::Modify => "RunningState.db_pending_changes.activity_modify",
            PendingChangeLevel::Insert => "RunningState.db_pending_changes.activity_insert",
        }
    }
}

/// 运行状态信息
#[derive(Clone)]
pub struct RunningState {
    pub kind: RunningKind,
    /// 标签标题
    pub title: SharedString,
    /// 具体活动描述，如 "vim 正在运行"、"文件传输进行中 (65%)"
    pub activity: SharedString,
    /// 数据库待提交变更的等级（如果有）
    pub pending_change_level: Option<PendingChangeLevel>,
}

impl RunningState {
    /// 创建一个终端运行状态
    pub fn terminal(title: SharedString, activity: SharedString) -> Option<Self> {
        Some(Self {
            kind: RunningKind::Terminal,
            title,
            activity,
            pending_change_level: None,
        })
    }

    /// 创建一个 SSH 会话运行状态
    pub fn ssh(title: SharedString, activity: SharedString) -> Option<Self> {
        Some(Self {
            kind: RunningKind::Ssh,
            title,
            activity,
            pending_change_level: None,
        })
    }

    /// 创建一个 SFTP 传输运行状态
    pub fn sftp(title: SharedString, activity: SharedString) -> Option<Self> {
        Some(Self {
            kind: RunningKind::Sftp,
            title,
            activity,
            pending_change_level: None,
        })
    }

    /// 创建一个数据库运行状态
    pub fn db(title: SharedString, activity: SharedString) -> Option<Self> {
        Some(Self {
            kind: RunningKind::Db,
            title,
            activity,
            pending_change_level: None,
        })
    }

    /// 创建一个数据库待提交变更状态
    pub fn db_pending_changes(
        title: SharedString,
        activity: SharedString,
        level: PendingChangeLevel,
    ) -> Option<Self> {
        Some(Self {
            kind: RunningKind::DbPendingChanges,
            title,
            activity,
            pending_change_level: Some(level),
        })
    }
}

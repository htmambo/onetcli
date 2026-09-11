//! 数据传输引擎的配置、进度事件与结果汇总类型

use one_core::storage::DbConnectionConfig;
use tokio::sync::mpsc;

/// 单次数据传输任务配置：把源库中的表（结构+数据）/视图复制到目标库
#[derive(Debug, Clone)]
pub struct TransferConfig {
    /// 源连接配置
    pub source_config: DbConnectionConfig,
    /// 源数据库名
    pub source_db: String,
    /// 目标连接配置
    pub target_config: DbConnectionConfig,
    /// 目标数据库名
    pub target_db: String,
    /// 待传输的表名列表
    pub tables: Vec<String>,
    /// 待传输的视图名列表
    pub views: Vec<String>,
    /// 单表/单视图失败时是否继续后续对象
    pub continue_on_error: bool,
    /// 建表前是否先 DROP TARGET 表
    pub drop_target_first: bool,
    /// 数据分页批量大小，0 表示使用默认值
    pub batch_size: usize,
}

/// 传输进度事件
#[derive(Debug, Clone)]
pub enum TransferProgressEvent {
    /// 开始传输某张表
    TableStart {
        /// 表名
        name: String,
        /// 当前表序号（从 0 开始）
        index: usize,
        /// 表总数
        total: usize,
    },
    /// 某张表已累计复制的行数
    TableProgress {
        /// 表名
        name: String,
        /// 累计行数
        rows: u64,
    },
    /// 某张表传输完成
    TableDone {
        /// 表名
        name: String,
        /// 总行数
        rows: u64,
    },
    /// 某张表传输失败
    TableFailed {
        /// 表名
        name: String,
        /// 错误信息
        error: String,
    },
    /// 某个视图传输完成
    ViewDone {
        /// 视图名
        name: String,
    },
    /// 某个视图传输失败
    ViewFailed {
        /// 视图名
        name: String,
        /// 错误信息
        error: String,
    },
    /// 普通日志
    Log(String),
    /// 任务被取消
    Cancelled,
    /// 任务结束（无论是否有失败项）
    Finished(TransferSummary),
}

/// 传输进度发送器类型
pub type TransferProgressSender = mpsc::UnboundedSender<TransferProgressEvent>;

/// 传输结果汇总
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransferSummary {
    /// 成功的表数量
    pub tables_ok: usize,
    /// 失败的表数量
    pub tables_failed: usize,
    /// 成功的视图数量
    pub views_ok: usize,
    /// 失败的视图数量
    pub views_failed: usize,
    /// 复制的总行数
    pub rows_copied: u64,
}

//! 数据传输预检测：连接、版本、类型映射与目标冲突检查，产出带建议的报告
//!
//! 在真正执行传输前运行，全部为只读探测；无法只读验证的能力（如目标账号
//! 写权限）以 Info 条目提示用户自行确认。

use anyhow::Result;
use one_core::storage::DatabaseType;
use rust_i18n::t;

use crate::connection::DbConnection;
use crate::manager::GlobalDbState;

use super::ddl_compat::TypeVerdict;
use super::session;
use super::types::TransferConfig;

/// 预检条目严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrecheckSeverity {
    /// 阻塞传输，必须处理后才能开始
    Error,
    /// 不会失败但有数据风险，给出建议
    Warning,
    /// 仅提示
    Info,
}

/// 一条预检结论
#[derive(Debug, Clone)]
pub struct PrecheckIssue {
    pub severity: PrecheckSeverity,
    /// 简短标题（已本地化）
    pub title: String,
    /// 详细说明与建议（已本地化）
    pub detail: String,
}

impl PrecheckIssue {
    pub fn error(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            severity: PrecheckSeverity::Error,
            title: title.into(),
            detail: detail.into(),
        }
    }

    pub fn warning(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            severity: PrecheckSeverity::Warning,
            title: title.into(),
            detail: detail.into(),
        }
    }

    pub fn info(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            severity: PrecheckSeverity::Info,
            title: title.into(),
            detail: detail.into(),
        }
    }
}

/// 预检报告
#[derive(Debug, Clone, Default)]
pub struct PrecheckReport {
    /// 两端连通性是否通过（失败时其余检查被跳过）
    pub connectivity_ok: bool,
    /// 源库服务器版本（探测不到为 None）
    pub source_version: Option<String>,
    /// 目标库服务器版本（探测不到为 None）
    pub target_version: Option<String>,
    /// 全部结论条目
    pub issues: Vec<PrecheckIssue>,
}

impl PrecheckReport {
    /// 是否存在阻塞级别的问题
    pub fn has_blocking_errors(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == PrecheckSeverity::Error)
    }
}

/// 运行完整预检：建两端会话执行只读探测后释放。
///
/// 两端任一连不通时直接返回带 Error 的报告，不做后续检查。
pub async fn run_precheck(state: &GlobalDbState, config: &TransferConfig) -> PrecheckReport {
    match run_precheck_inner(state, config).await {
        Ok(report) => report,
        Err(connect_error) => PrecheckReport {
            connectivity_ok: false,
            issues: vec![PrecheckIssue::error(
                t!("DataTransfer.precheck_connect_failed").to_string(),
                connect_error.to_string(),
            )],
            ..Default::default()
        },
    }
}

async fn run_precheck_inner(
    state: &GlobalDbState,
    config: &TransferConfig,
) -> Result<PrecheckReport> {
    let endpoints = session::open_endpoints(state, config).await?;
    let report = run_with_endpoints(state, config, &endpoints).await;
    session::release_sessions(state, &endpoints).await;
    Ok(report)
}

async fn run_with_endpoints(
    state: &GlobalDbState,
    config: &TransferConfig,
    endpoints: &session::TransferEndpoints,
) -> PrecheckReport {
    let (mut source_guard, mut target_guard) =
        match session::lock_connections(state, endpoints).await {
            Ok(guards) => guards,
            Err(e) => {
                return PrecheckReport {
                    connectivity_ok: false,
                    issues: vec![PrecheckIssue::error(
                        t!("DataTransfer.precheck_connect_failed").to_string(),
                        e.to_string(),
                    )],
                    ..Default::default()
                };
            }
        };
    let Some(source) = source_guard.connection() else {
        return PrecheckReport {
            connectivity_ok: false,
            issues: vec![PrecheckIssue::error(
                t!("DataTransfer.precheck_connect_failed").to_string(),
                "source session connection not found",
            )],
            ..Default::default()
        };
    };
    let Some(target) = target_guard.connection() else {
        return PrecheckReport {
            connectivity_ok: false,
            issues: vec![PrecheckIssue::error(
                t!("DataTransfer.precheck_connect_failed").to_string(),
                "target session connection not found",
            )],
            ..Default::default()
        };
    };

    let mut report = PrecheckReport {
        connectivity_ok: true,
        ..Default::default()
    };
    check_versions(config, source, target, &mut report).await;
    check_target_objects(config, endpoints, target, &mut report).await;
    check_column_types(config, endpoints, source, &mut report).await;
    check_write_permission_hint(config, &mut report);
    report
}

/// 版本探测与版本相关能力提示
async fn check_versions(
    config: &TransferConfig,
    source: &(dyn DbConnection + Send + Sync),
    target: &(dyn DbConnection + Send + Sync),
    report: &mut PrecheckReport,
) {
    for (label, db_type, connection) in [
        ("source", config.source_config.database_type, source),
        ("target", config.target_config.database_type, target),
    ] {
        let Some(version) = probe_version(db_type, connection).await else {
            if db_type != DatabaseType::External {
                report.issues.push(PrecheckIssue::warning(
                    t!("DataTransfer.precheck_version_unknown").to_string(),
                    t!("DataTransfer.precheck_version_unknown_detail", side = label).to_string(),
                ));
            }
            continue;
        };
        if label == "source" {
            report.source_version = Some(version);
        } else {
            report.target_version = Some(version);
        }
    }

    // 跨库种提示目标库对 JSON/CHECK 等能力的最低版本要求
    if let Some(target_version) = &report.target_version {
        if config.target_config.database_type == DatabaseType::MySQL
            && mysql_major_version(target_version).is_some_and(|major| major < 8)
        {
            report.issues.push(PrecheckIssue::warning(
                t!("DataTransfer.precheck_target_version_low").to_string(),
                t!("DataTransfer.precheck_target_mysql_lt8_detail").to_string(),
            ));
        }
    }

    if config.source_config.database_type != config.target_config.database_type {
        report.issues.push(PrecheckIssue::info(
            t!("DataTransfer.precheck_cross_engine").to_string(),
            t!(
                "DataTransfer.precheck_cross_engine_detail",
                source = config.source_config.database_type.as_str(),
                target = config.target_config.database_type.as_str()
            )
            .to_string(),
        ));
    }
}

/// 目标库同名对象冲突检测与建议
async fn check_target_objects(
    config: &TransferConfig,
    endpoints: &session::TransferEndpoints,
    target: &(dyn DbConnection + Send + Sync),
    report: &mut PrecheckReport,
) {
    let existing_tables = match endpoints
        .target_plugin
        .list_tables(target, &config.target_db, None)
        .await
    {
        Ok(tables) => tables,
        Err(e) => {
            report.issues.push(PrecheckIssue::warning(
                t!("DataTransfer.precheck_conflict_unknown").to_string(),
                e.to_string(),
            ));
            return;
        }
    };
    let existing_names: Vec<String> = existing_tables.iter().map(|t| t.name.clone()).collect();

    let mut conflicts: Vec<&String> = config
        .tables
        .iter()
        .filter(|t| existing_names.iter().any(|n| n == *t))
        .collect();
    let total = config.tables.len() + config.views.len();
    if conflicts.is_empty() {
        report.issues.push(PrecheckIssue::info(
            t!("DataTransfer.precheck_no_conflict").to_string(),
            t!("DataTransfer.precheck_no_conflict_detail", count = total).to_string(),
        ));
        return;
    }
    conflicts.sort();
    let preview: Vec<String> = conflicts.iter().take(5).map(|s| s.to_string()).collect();
    let preview_text = format!(
        "{}{}",
        preview.join(", "),
        if conflicts.len() > 5 { ", ..." } else { "" }
    );
    if config.drop_target_first {
        report.issues.push(PrecheckIssue::info(
            t!(
                "DataTransfer.precheck_conflict_will_drop",
                count = conflicts.len()
            )
            .to_string(),
            t!(
                "DataTransfer.precheck_conflict_will_drop_detail",
                tables = preview_text
            )
            .to_string(),
        ));
    } else {
        report.issues.push(PrecheckIssue::error(
            t!(
                "DataTransfer.precheck_conflict_exists",
                count = conflicts.len()
            )
            .to_string(),
            t!(
                "DataTransfer.precheck_conflict_exists_detail",
                tables = preview_text
            )
            .to_string(),
        ));
    }
}

/// 源列类型到目标方言的兼容性检查（逐表逐列，只读 list_columns）
async fn check_column_types(
    config: &TransferConfig,
    endpoints: &session::TransferEndpoints,
    source: &(dyn DbConnection + Send + Sync),
    report: &mut PrecheckReport,
) {
    let cross_engine = config.source_config.database_type != config.target_config.database_type;
    if !cross_engine {
        return;
    }
    let source_type = config.source_config.database_type;
    let target_type = config.target_config.database_type;
    let mut incompatible: Vec<String> = Vec::new();
    let mut lossy: Vec<String> = Vec::new();

    for table in &config.tables {
        let columns = match endpoints
            .source_plugin
            .list_columns(source, &config.source_db, None, table)
            .await
        {
            Ok(columns) => columns,
            Err(e) => {
                report.issues.push(PrecheckIssue::warning(
                    t!("DataTransfer.precheck_columns_unavailable").to_string(),
                    format!("{table}: {e}"),
                ));
                continue;
            }
        };
        for column in &columns {
            let verdict = super::ddl_compat::column_verdict(
                &super::ddl_compat::normalize_type(&column.data_type, source_type),
                target_type,
            );
            match verdict {
                TypeVerdict::Compatible => {}
                TypeVerdict::Lossy(hint) => {
                    lossy.push(format!(
                        "{}.{}: {} ({})",
                        table, column.name, column.data_type, hint
                    ));
                }
                TypeVerdict::Unsupported(hint) => {
                    incompatible.push(format!(
                        "{}.{}: {} ({})",
                        table, column.name, column.data_type, hint
                    ));
                }
            }
        }
    }

    if !incompatible.is_empty() {
        report.issues.push(PrecheckIssue::error(
            t!(
                "DataTransfer.precheck_types_unsupported",
                count = incompatible.len()
            )
            .to_string(),
            incompatible
                .into_iter()
                .take(10)
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    if !lossy.is_empty() {
        report.issues.push(PrecheckIssue::warning(
            t!("DataTransfer.precheck_types_lossy", count = lossy.len()).to_string(),
            lossy.into_iter().take(10).collect::<Vec<_>>().join("; "),
        ));
    }
}

/// 写权限只能真实执行才能验证，这里以 Info 提示用户自行确认
fn check_write_permission_hint(config: &TransferConfig, report: &mut PrecheckReport) {
    if config.target_config.database_type == DatabaseType::SQLite {
        return;
    }
    report.issues.push(PrecheckIssue::info(
        t!("DataTransfer.precheck_permission_hint").to_string(),
        t!("DataTransfer.precheck_permission_hint_detail").to_string(),
    ));
}

/// 各库种的服务器版本探测 SQL 与结果解析
async fn probe_version(
    db_type: DatabaseType,
    connection: &(dyn DbConnection + Send + Sync),
) -> Option<String> {
    let sql = match db_type {
        DatabaseType::MySQL => "SELECT VERSION()",
        DatabaseType::PostgreSQL => "SELECT version()",
        DatabaseType::SQLite => "SELECT sqlite_version()",
        _ => return None,
    };
    let result = connection.query(sql).await.ok()?;
    match result {
        crate::executor::SqlResult::Query(q) => {
            let raw = q.rows.first()?.first()?.clone()?;
            Some(match db_type {
                // PostgreSQL 返回 "PostgreSQL 16.4 (Debian ...)"，取前两段
                DatabaseType::PostgreSQL => {
                    let parts: Vec<&str> = raw.split_whitespace().collect();
                    if parts.len() >= 2 {
                        format!("{} {}", parts[0], parts[1])
                    } else {
                        raw
                    }
                }
                _ => raw,
            })
        }
        _ => None,
    }
}

fn mysql_major_version(version: &str) -> Option<u64> {
    version.trim().split('.').next()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_major_version_parses() {
        assert_eq!(mysql_major_version("8.0.36"), Some(8));
        assert_eq!(mysql_major_version("5.7.44-log"), Some(5));
        assert_eq!(mysql_major_version("not-a-version"), None);
    }
}

#[cfg(test)]
mod i18n_tests {
    #[test]
    fn precheck_keys_resolve() {
        let keys = [
            "DataTransfer.precheck_connect_failed",
            "DataTransfer.precheck_version_unknown",
            "DataTransfer.precheck_version_unknown_detail",
            "DataTransfer.precheck_target_version_low",
            "DataTransfer.precheck_target_mysql_lt8_detail",
            "DataTransfer.precheck_cross_engine",
            "DataTransfer.precheck_cross_engine_detail",
            "DataTransfer.precheck_conflict_unknown",
            "DataTransfer.precheck_no_conflict",
            "DataTransfer.precheck_no_conflict_detail",
            "DataTransfer.precheck_conflict_will_drop",
            "DataTransfer.precheck_conflict_will_drop_detail",
            "DataTransfer.precheck_conflict_exists",
            "DataTransfer.precheck_conflict_exists_detail",
            "DataTransfer.precheck_types_unsupported",
            "DataTransfer.precheck_types_lossy",
            "DataTransfer.precheck_permission_hint",
            "DataTransfer.precheck_permission_hint_detail",
            "DataTransfer.precheck_columns_unavailable",
        ];
        rust_i18n::set_locale("zh-CN");
        for key in keys {
            let text = rust_i18n::t!(key).to_string();
            assert_ne!(text, key, "key {key} 未解析到译文");
        }
        rust_i18n::set_locale("en");
        for key in keys {
            let text = rust_i18n::t!(key).to_string();
            assert_ne!(text, key, "key {key} 未解析到译文");
        }
    }
}

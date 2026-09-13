//! Oracle 方言 SQL 生成辅助。
//!
//! 内置 Oracle 驱动已移除，但外部 IPC 驱动（如达梦、oracle-go）仍声明
//! oracle 兼容方言，此模块为其保留表变更 / 数据复制 SQL 的 Oracle 语义
//! （DATE/TIMESTAMP 字面量、CLOB 分块、ROWID 定位等）。
//! 全部为纯函数，不依赖任何数据库连接。

use chrono::{DateTime, FixedOffset};

use crate::types::{ColumnInfo, CopySqlRequest, TableCellChange, TableRowChange, TableSaveRequest};

const ORACLE_SQL_LITERAL_CHUNK_BYTES: usize = 3000;
const ORACLE_DATE_FORMAT: &str = "YYYY-MM-DD";
const ORACLE_DATETIME_FORMAT: &str = "YYYY-MM-DD HH24:MI:SS";
const ORACLE_DATETIME_FRACTION_FORMAT: &str = "YYYY-MM-DD HH24:MI:SS.FF6";
const ORACLE_DATETIME_TZ_FORMAT: &str = "YYYY-MM-DD HH24:MI:SS TZH:TZM";
const ORACLE_DATETIME_TZ_FRACTION_FORMAT: &str = "YYYY-MM-DD HH24:MI:SS.FF6 TZH:TZM";

/// 生成整表变更 SQL（等价于原 OraclePlugin::generate_table_changes_sql）。
pub fn generate_table_changes_sql(request: &TableSaveRequest) -> String {
    let mut sql_statements = Vec::new();

    for change in &request.changes {
        if let Some(sql) = build_table_change_sql(request, change) {
            sql_statements.push(sql);
        }
    }

    if sql_statements.is_empty() {
        rust_i18n::t!("Error.no_changes").to_string()
    } else {
        sql_statements.join(";\n\n") + ";"
    }
}

/// 格式化复制场景的值字面量（DATE/LOB/超长字符串分块）。
pub fn format_copy_value(value: &Option<String>, col_info: Option<&ColumnInfo>) -> String {
    match value {
        None => "NULL".to_string(),
        Some(v) => table_change_value_expr(v, col_info),
    }
}

/// 生成复制用 INSERT SQL（等价原 OraclePlugin::generate_copy_insert_sql）。
pub fn copy_insert_sql(request: &CopySqlRequest) -> String {
    if request.rows.is_empty() || request.column_names.is_empty() {
        return String::new();
    }

    let table_name = copy_table_name(request.schema.as_deref(), &request.table);
    let columns_str = request
        .column_names
        .iter()
        .map(|c| quote_identifier(c))
        .collect::<Vec<_>>()
        .join(", ");

    let mut statements = Vec::new();
    for row in &request.rows {
        let values: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, val)| format_copy_value(val, request.columns.get(i)))
            .collect();
        statements.push(format!(
            "INSERT INTO {} ({}) VALUES ({});",
            table_name,
            columns_str,
            values.join(", ")
        ));
    }
    statements.join("\n")
}

/// 生成复制用带列注释 INSERT SQL。
pub fn copy_insert_with_comments_sql(request: &CopySqlRequest) -> String {
    if request.rows.is_empty() || request.column_names.is_empty() {
        return String::new();
    }

    let table_name = copy_table_name(request.schema.as_deref(), &request.table);
    let columns_str = request
        .column_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let quoted = quote_identifier(name);
            if let Some(col_info) = request.columns.get(i) {
                if let Some(comment) = &col_info.comment {
                    if !comment.is_empty() {
                        return format!("{} /* {} */", quoted, comment);
                    }
                }
            }
            quoted
        })
        .collect::<Vec<_>>()
        .join(", ");

    let mut statements = Vec::new();
    for row in &request.rows {
        let values: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, val)| format_copy_value(val, request.columns.get(i)))
            .collect();
        statements.push(format!(
            "INSERT INTO {} ({}) VALUES ({});",
            table_name,
            columns_str,
            values.join(", ")
        ));
    }
    statements.join("\n")
}

/// 生成复制用 UPDATE SQL。
pub fn copy_update_sql(request: &CopySqlRequest) -> String {
    if request.rows.is_empty() || request.column_names.is_empty() {
        return String::new();
    }

    let original_rows = request.original_rows.as_ref().unwrap_or(&request.rows);
    let table_name = copy_table_name(request.schema.as_deref(), &request.table);
    let mut statements = Vec::new();

    for (row, original_row) in request.rows.iter().zip(original_rows.iter()) {
        let set_parts: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, val)| {
                let col_name = quote_identifier(
                    request
                        .column_names
                        .get(i)
                        .map(|s| s.as_str())
                        .unwrap_or(""),
                );
                let value = format_copy_value(val, request.columns.get(i));
                format!("{} = {}", col_name, value)
            })
            .collect();
        let where_str = copy_where_clause(request, original_row);
        statements.push(format!(
            "UPDATE {} SET {} WHERE {};",
            table_name,
            set_parts.join(", "),
            where_str
        ));
    }
    statements.join("\n")
}

/// 生成复制用 DELETE SQL。
pub fn copy_delete_sql(request: &CopySqlRequest) -> String {
    if request.rows.is_empty() {
        return String::new();
    }

    let table_name = copy_table_name(request.schema.as_deref(), &request.table);
    let mut statements = Vec::new();
    for row in &request.rows {
        let where_str = copy_where_clause(request, row);
        statements.push(format!("DELETE FROM {} WHERE {};", table_name, where_str));
    }
    statements.join("\n")
}

fn copy_table_name(schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(s) if !s.is_empty() => format!("{}.{}", quote_identifier(s), quote_identifier(table)),
        _ => quote_identifier(table),
    }
}

/// 与 DatabasePlugin::generate_copy_where_clause 默认实现语义一致，值走 Oracle 字面量。
fn copy_where_clause(request: &CopySqlRequest, row: &[Option<String>]) -> String {
    let primary_key_indices: Vec<usize> = request
        .columns
        .iter()
        .enumerate()
        .filter(|(_, col)| col.is_primary_key)
        .map(|(i, _)| i)
        .collect();

    let indices_to_use = if primary_key_indices.is_empty() {
        (0..request.column_names.len()).collect::<Vec<_>>()
    } else {
        primary_key_indices
    };

    let conditions: Vec<String> = indices_to_use
        .iter()
        .filter_map(|&i| {
            let col_name = request.column_names.get(i)?;
            let val = row.get(i)?;
            let quoted_col = quote_identifier(col_name);
            match val {
                None => Some(format!("{} IS NULL", quoted_col)),
                Some(v) if v.is_empty() => Some(format!("{} IS NULL", quoted_col)),
                _ => Some(format!(
                    "{} = {}",
                    quoted_col,
                    format_copy_value(val, request.columns.get(i))
                )),
            }
        })
        .collect();

    if conditions.is_empty() {
        "1=1".to_string()
    } else {
        conditions.join(" AND ")
    }
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn format_table_reference(_database: &str, schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(s) => format!("{}.{}", quote_identifier(s), quote_identifier(table)),
        None => quote_identifier(table),
    }
}

fn table_ident(request: &TableSaveRequest) -> String {
    format_table_reference(&request.database, request.schema.as_deref(), &request.table)
}

fn table_change_value_expr(value: &str, column: Option<&ColumnInfo>) -> String {
    if value == "NULL" || value.is_empty() {
        return "NULL".to_string();
    }

    if let Some(expr) = oracle_temporal_value_expr(value, column) {
        return expr;
    }

    if is_oracle_lob_column(column) {
        return oracle_lob_literal_expr(value, column);
    }

    if escaped_sql_literal_len(value) > ORACLE_SQL_LITERAL_CHUNK_BYTES {
        return oracle_chunked_string_literal_expr(value);
    }

    oracle_string_literal(value)
}

fn build_table_change_sql(request: &TableSaveRequest, change: &TableRowChange) -> Option<String> {
    match change {
        TableRowChange::Added { .. } => build_added_sql(request, change),
        TableRowChange::Updated { .. } => build_updated_sql(request, change),
        TableRowChange::Deleted { .. } => build_deleted_sql(request, change),
    }
}

fn build_added_sql(request: &TableSaveRequest, change: &TableRowChange) -> Option<String> {
    let TableRowChange::Added { data } = change else {
        return None;
    };
    if data.is_empty() {
        return None;
    }

    let columns: Vec<String> = request
        .columns
        .iter()
        .map(|column| quote_identifier(&column.name))
        .collect();
    let values: Vec<String> = data
        .iter()
        .enumerate()
        .map(|(ix, value)| table_change_value_expr(value, request.columns.get(ix)))
        .collect();

    Some(format!(
        "INSERT INTO {} ({}) VALUES ({})",
        table_ident(request),
        columns.join(", "),
        values.join(", ")
    ))
}

fn build_updated_sql(request: &TableSaveRequest, change: &TableRowChange) -> Option<String> {
    let TableRowChange::Updated {
        original_data,
        changes,
        rowid,
    } = change
    else {
        return None;
    };
    if changes.is_empty() {
        return None;
    }

    let set_clause = update_set_clause(request, changes);
    if let Some(rid) = rowid {
        return Some(format!(
            "UPDATE {} SET {} WHERE ROWID = {}",
            table_ident(request),
            set_clause.join(", "),
            oracle_string_literal(rid)
        ));
    }

    let where_clause = build_where_clause(request, original_data);
    Some(format!(
        "UPDATE {} SET {}{}{}",
        table_ident(request),
        set_clause.join(", "),
        if where_clause.is_empty() {
            ""
        } else {
            " WHERE "
        },
        where_clause
    ))
}

fn build_deleted_sql(request: &TableSaveRequest, change: &TableRowChange) -> Option<String> {
    let TableRowChange::Deleted {
        original_data,
        rowid,
    } = change
    else {
        return None;
    };

    if let Some(rid) = rowid {
        return Some(format!(
            "DELETE FROM {} WHERE ROWID = {}",
            table_ident(request),
            oracle_string_literal(rid)
        ));
    }

    let where_clause = build_where_clause(request, original_data);
    Some(format!(
        "DELETE FROM {}{}{}",
        table_ident(request),
        if where_clause.is_empty() {
            ""
        } else {
            " WHERE "
        },
        where_clause
    ))
}

fn update_set_clause(request: &TableSaveRequest, changes: &[TableCellChange]) -> Vec<String> {
    changes
        .iter()
        .map(|change| {
            let column_name = change_column_name(request, change);
            let ident = quote_identifier(&column_name);
            let value = table_change_value_expr(
                &change.new_value,
                request.columns.get(change.column_index),
            );
            format!("{} = {}", ident, value)
        })
        .collect()
}

fn change_column_name(request: &TableSaveRequest, change: &TableCellChange) -> String {
    if !change.column_name.is_empty() {
        return change.column_name.clone();
    }

    request
        .columns
        .get(change.column_index)
        .map(|column| column.name.clone())
        .unwrap_or_default()
}

/// 与 DatabasePlugin::build_table_change_where_clause 的默认实现语义一致。
fn build_where_clause(request: &TableSaveRequest, original_data: &[String]) -> String {
    let column_names: Vec<&str> = request.columns.iter().map(|c| c.name.as_str()).collect();

    let primary_key_indices: Vec<usize> = request
        .columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_primary_key)
        .map(|(i, _)| i)
        .collect();

    let unique_key_indices: Vec<usize> = request
        .index_infos
        .iter()
        .filter(|idx| idx.is_unique)
        .flat_map(|idx| {
            idx.columns
                .iter()
                .filter_map(|col_name| column_names.iter().position(|n| n == col_name))
        })
        .collect();

    let indices: Vec<usize> = if !primary_key_indices.is_empty() {
        primary_key_indices
    } else if !unique_key_indices.is_empty() {
        unique_key_indices
    } else {
        (0..column_names.len()).collect()
    };

    let mut parts = Vec::new();
    for index in indices {
        if let (Some(column), Some(value)) = (column_names.get(index), original_data.get(index)) {
            let ident = quote_identifier(column);
            if value == "NULL" {
                parts.push(format!("{} IS NULL", ident));
            } else {
                parts.push(format!("{} = '{}'", ident, value.replace('\'', "''")));
            }
        }
    }

    parts.join(" AND ")
}

fn is_oracle_lob_column(column: Option<&ColumnInfo>) -> bool {
    column
        .map(|column| {
            let data_type = column.data_type.to_ascii_uppercase();
            data_type.contains("CLOB") || data_type.contains("NCLOB")
        })
        .unwrap_or(false)
}

#[derive(Clone, Copy)]
enum OracleTemporalKind {
    Date,
    Timestamp,
    TimestampTz,
}

fn oracle_temporal_value_expr(value: &str, column: Option<&ColumnInfo>) -> Option<String> {
    let kind = oracle_temporal_kind(column)?;
    let value = normalize_oracle_temporal_value(value);
    let has_time = value.contains(':');
    let has_fraction = value.contains('.');
    let has_timezone = oracle_temporal_has_timezone(&value);

    match kind {
        OracleTemporalKind::Date if has_timezone => Some(format!(
            "CAST({} AS DATE)",
            oracle_timestamp_tz_expr(&value)
        )),
        OracleTemporalKind::Date if has_fraction => {
            Some(format!("CAST({} AS DATE)", oracle_timestamp_expr(&value)))
        }
        OracleTemporalKind::Date if has_time => Some(oracle_date_expr(&value, true)),
        OracleTemporalKind::Date => Some(oracle_date_expr(&value, false)),
        OracleTemporalKind::Timestamp if has_timezone => Some(format!(
            "CAST({} AS TIMESTAMP)",
            oracle_timestamp_tz_expr(&value)
        )),
        OracleTemporalKind::Timestamp => Some(oracle_timestamp_expr(&value)),
        OracleTemporalKind::TimestampTz if has_timezone => Some(oracle_timestamp_tz_expr(&value)),
        OracleTemporalKind::TimestampTz => Some(oracle_timestamp_expr(&value)),
    }
}

fn oracle_temporal_kind(column: Option<&ColumnInfo>) -> Option<OracleTemporalKind> {
    let data_type = column?.data_type.to_ascii_uppercase();
    if data_type.contains("TIMESTAMP WITH TIME ZONE")
        || data_type.contains("TIMESTAMP WITH LOCAL TIME ZONE")
    {
        return Some(OracleTemporalKind::TimestampTz);
    }
    if data_type.contains("TIMESTAMP") {
        return Some(OracleTemporalKind::Timestamp);
    }
    if data_type.trim() == "DATE" {
        return Some(OracleTemporalKind::Date);
    }
    None
}

fn normalize_oracle_temporal_value(value: &str) -> String {
    let trimmed = value.trim();
    if let Ok(datetime) = DateTime::parse_from_rfc3339(trimmed) {
        return format_oracle_offset_datetime(datetime);
    }

    let normalized = trimmed.replace('T', " ");
    match normalized.strip_suffix('Z') {
        Some(prefix) => format!("{} +00:00", prefix.trim_end()),
        None => normalized,
    }
}

fn format_oracle_offset_datetime(value: DateTime<FixedOffset>) -> String {
    let micros = value.timestamp_subsec_micros();
    let datetime = if micros == 0 {
        value.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        format!("{}.{:06}", value.format("%Y-%m-%d %H:%M:%S"), micros)
    };
    format!("{} {}", datetime, value.format("%:z"))
}

fn oracle_temporal_has_timezone(value: &str) -> bool {
    let value = value.trim();
    if value.ends_with('Z') {
        return true;
    }
    if value.len() < 6 {
        return false;
    }
    let Some(suffix) = value.get(value.len().saturating_sub(6)..) else {
        return false;
    };
    let mut chars = suffix.chars();
    matches!(chars.next(), Some('+') | Some('-'))
        && chars.nth(2) == Some(':')
        && suffix[1..3].chars().all(|ch| ch.is_ascii_digit())
        && suffix[4..6].chars().all(|ch| ch.is_ascii_digit())
}

fn oracle_date_expr(value: &str, has_time: bool) -> String {
    let format = if has_time {
        ORACLE_DATETIME_FORMAT
    } else {
        ORACLE_DATE_FORMAT
    };
    format!(
        "TO_DATE({}, {})",
        oracle_string_literal(value),
        oracle_string_literal(format)
    )
}

fn oracle_timestamp_expr(value: &str) -> String {
    let format = oracle_timestamp_format(value);
    format!(
        "TO_TIMESTAMP({}, {})",
        oracle_string_literal(value),
        oracle_string_literal(format)
    )
}

fn oracle_timestamp_tz_expr(value: &str) -> String {
    let format = if value.contains('.') {
        ORACLE_DATETIME_TZ_FRACTION_FORMAT
    } else {
        ORACLE_DATETIME_TZ_FORMAT
    };
    format!(
        "TO_TIMESTAMP_TZ({}, {})",
        oracle_string_literal(value),
        oracle_string_literal(format)
    )
}

fn oracle_timestamp_format(value: &str) -> &'static str {
    if value.contains('.') {
        ORACLE_DATETIME_FRACTION_FORMAT
    } else if value.contains(':') {
        ORACLE_DATETIME_FORMAT
    } else {
        ORACLE_DATE_FORMAT
    }
}

fn escaped_sql_literal_len(value: &str) -> usize {
    value
        .chars()
        .map(|ch| if ch == '\'' { 2 } else { ch.len_utf8() })
        .sum()
}

fn oracle_lob_literal_expr(value: &str, column: Option<&ColumnInfo>) -> String {
    let chunks = split_oracle_literal_chunks(value, ORACLE_SQL_LITERAL_CHUNK_BYTES);
    let constructor = if column
        .map(|column| column.data_type.to_ascii_uppercase().contains("NCLOB"))
        .unwrap_or(false)
    {
        "TO_NCLOB"
    } else {
        "TO_CLOB"
    };

    chunks
        .into_iter()
        .map(|chunk| format!("{}({})", constructor, oracle_string_literal(&chunk)))
        .collect::<Vec<_>>()
        .join(" || ")
}

fn oracle_chunked_string_literal_expr(value: &str) -> String {
    split_oracle_literal_chunks(value, ORACLE_SQL_LITERAL_CHUNK_BYTES)
        .into_iter()
        .map(|chunk| oracle_string_literal(&chunk))
        .collect::<Vec<_>>()
        .join(" || ")
}

fn oracle_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn split_oracle_literal_chunks(value: &str, max_escaped_bytes: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_len = 0;

    for ch in value.chars() {
        let escaped_len = if ch == '\'' { 2 } else { ch.len_utf8() };
        if current_len + escaped_len > max_escaped_bytes && !current.is_empty() {
            chunks.push(current);
            current = String::new();
            current_len = 0;
        }
        current.push(ch);
        current_len += escaped_len;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_values_use_oracle_literals() {
        let col = ColumnInfo {
            name: "d".into(),
            data_type: "DATE".into(),
            is_nullable: true,
            is_primary_key: false,
            default_value: None,
            comment: None,
            charset: None,
            collation: None,
            enum_values: None,
        };
        assert_eq!(
            table_change_value_expr("2024-01-02 03:04:05", Some(&col)),
            "TO_DATE('2024-01-02 03:04:05', 'YYYY-MM-DD HH24:MI:SS')"
        );
    }

    #[test]
    fn long_literals_are_chunked() {
        let long_value = "a".repeat(ORACLE_SQL_LITERAL_CHUNK_BYTES + 50);
        let expr = table_change_value_expr(&long_value, None);
        assert!(expr.contains(" || "), "should be chunked with ||");
    }
}

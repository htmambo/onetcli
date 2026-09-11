//! 数据传输引擎的纯 SQL 构造函数（不依赖连接，可独立测试）

use crate::executor::QueryColumnMeta;
use crate::plugin::DatabasePlugin;
use crate::types::{ColumnInfo, FieldType};

/// 数值/布尔类型在 INSERT 中不加引号
fn is_unquoted_type(db_type: &str) -> bool {
    matches!(
        FieldType::from_db_type(db_type),
        FieldType::Integer | FieldType::Decimal | FieldType::Boolean
    )
}

/// 把单个值格式化为 INSERT 字面量：NULL 原样输出，数值/布尔不加引号，
/// 其余加单引号并转义内部单引号；数值类型的空串按 NULL 处理避免非法 SQL
pub fn format_insert_value(value: Option<&str>, db_type: &str) -> String {
    let Some(v) = value else {
        return "NULL".to_string();
    };
    if is_unquoted_type(db_type) {
        if v.trim().is_empty() {
            return "NULL".to_string();
        }
        return v.to_string();
    }
    format!("'{}'", v.replace('\'', "''"))
}

/// 用目标方言的标识符引用风格生成 CREATE TABLE IF NOT EXISTS
pub fn build_create_table_sql(
    plugin: &dyn DatabasePlugin,
    table: &str,
    columns: &[ColumnInfo],
) -> String {
    let defs = columns
        .iter()
        .map(|col| format!("    {}", plugin.build_column_definition(col, true)))
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "CREATE TABLE IF NOT EXISTS {} (\n{}\n);",
        plugin.quote_identifier(table),
        defs
    )
}

/// 生成 DROP TABLE IF EXISTS（目标方言引用风格）
pub fn build_drop_table_sql(plugin: &dyn DatabasePlugin, table: &str) -> String {
    format!("DROP TABLE IF EXISTS {};", plugin.quote_identifier(table))
}

/// 把一批行拼成单条多值 INSERT；空批次返回空串
pub fn build_insert_sql(
    plugin: &dyn DatabasePlugin,
    table: &str,
    column_meta: &[QueryColumnMeta],
    rows: &[Vec<Option<String>>],
) -> String {
    if rows.is_empty() || column_meta.is_empty() {
        return String::new();
    }
    let cols = column_meta
        .iter()
        .map(|c| plugin.quote_identifier(&c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!(
        "INSERT INTO {} ({}) VALUES ",
        plugin.quote_identifier(table),
        cols
    );
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            sql.push_str(", ");
        }
        sql.push('(');
        for (j, value) in row.iter().enumerate() {
            if j > 0 {
                sql.push_str(", ");
            }
            let db_type = column_meta.get(j).map(|c| c.db_type.as_str()).unwrap_or("");
            sql.push_str(&format_insert_value(value.as_deref(), db_type));
        }
        sql.push(')');
    }
    sql.push(';');
    sql
}

/// 生成分页 SELECT（源方言分页语法）
pub fn build_select_batch_sql(
    plugin: &dyn DatabasePlugin,
    table: &str,
    batch_size: usize,
    offset: usize,
) -> String {
    format!(
        "SELECT * FROM {}{}",
        plugin.quote_identifier(table),
        plugin.format_pagination(batch_size, offset, "")
    )
}

/// 生成 CREATE VIEW：定义本身已是完整 CREATE 语句时直接采用，否则包装
pub fn build_create_view_sql(plugin: &dyn DatabasePlugin, name: &str, definition: &str) -> String {
    let trimmed = definition.trim().trim_end_matches(';').trim();
    if trimmed.to_uppercase().starts_with("CREATE") {
        format!("{};", trimmed)
    } else {
        format!(
            "CREATE VIEW {} AS {};",
            plugin.quote_identifier(name),
            trimmed
        )
    }
}

/// 判断当前批是否为最后一批：返回行数小于批量即没有更多数据（空表首批即末批）
pub fn is_last_batch(returned: usize, batch_size: usize) -> bool {
    returned < batch_size
}

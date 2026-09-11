//! data_transfer 纯逻辑单元测试：INSERT 转义、CREATE TABLE 生成、批次边界

use super::sql::{
    build_create_table_sql, build_create_view_sql, build_insert_sql, build_select_batch_sql,
    format_insert_value, is_last_batch,
};
use crate::executor::QueryColumnMeta;
use crate::mysql::MySqlPlugin;
use crate::postgresql::PostgresPlugin;
use crate::types::ColumnInfo;

fn column(name: &str, data_type: &str, nullable: bool, primary: bool) -> ColumnInfo {
    ColumnInfo {
        name: name.to_string(),
        data_type: data_type.to_string(),
        is_nullable: nullable,
        is_primary_key: primary,
        default_value: None,
        comment: None,
        charset: None,
        collation: None,
        enum_values: None,
    }
}

fn meta(name: &str, db_type: &str) -> QueryColumnMeta {
    QueryColumnMeta::new(name, db_type)
}

#[test]
fn format_insert_value_handles_null() {
    assert_eq!("NULL", format_insert_value(None, "VARCHAR(10)"));
    assert_eq!("NULL", format_insert_value(None, "INT"));
}

#[test]
fn format_insert_value_escapes_quotes_in_strings() {
    assert_eq!(
        "'O''Brien'",
        format_insert_value(Some("O'Brien"), "VARCHAR(64)")
    );
    assert_eq!("''", format_insert_value(Some(""), "TEXT"));
    assert_eq!("'a''b''c'", format_insert_value(Some("a'b'c"), "CHAR(3)"));
}

#[test]
fn format_insert_value_keeps_numbers_and_booleans_unquoted() {
    assert_eq!("42", format_insert_value(Some("42"), "INT"));
    assert_eq!("-7", format_insert_value(Some("-7"), "BIGINT"));
    assert_eq!("3.14", format_insert_value(Some("3.14"), "DECIMAL(10,2)"));
    assert_eq!("true", format_insert_value(Some("true"), "BOOLEAN"));
    assert_eq!("1", format_insert_value(Some("1"), "BIT"));
}

#[test]
fn format_insert_value_treats_empty_numeric_as_null() {
    assert_eq!("NULL", format_insert_value(Some(""), "INT"));
    assert_eq!("NULL", format_insert_value(Some("   "), "DOUBLE"));
}

#[test]
fn format_insert_value_quotes_datetime_and_json() {
    assert_eq!(
        "'2026-09-11 10:00:00'",
        format_insert_value(Some("2026-09-11 10:00:00"), "DATETIME")
    );
    assert_eq!("'{}'", format_insert_value(Some("{}"), "JSONB"));
}

#[test]
fn build_create_table_sql_mysql_dialect() {
    let plugin = MySqlPlugin::new();
    let columns = vec![
        column("id", "INT", false, true),
        column("name", "VARCHAR(64)", true, false),
    ];
    let sql = build_create_table_sql(&plugin, "users", &columns);
    assert_eq!(
        "CREATE TABLE IF NOT EXISTS `users` (\n    `id` INT NOT NULL PRIMARY KEY,\n    `name` VARCHAR(64)\n);",
        sql
    );
}

#[test]
fn build_create_table_sql_postgresql_dialect() {
    let plugin = PostgresPlugin::new();
    let columns = vec![
        column("id", "INTEGER", false, true),
        column("title", "TEXT", true, false),
    ];
    let sql = build_create_table_sql(&plugin, "articles", &columns);
    assert_eq!(
        "CREATE TABLE IF NOT EXISTS \"articles\" (\n    \"id\" INTEGER NOT NULL PRIMARY KEY,\n    \"title\" TEXT\n);",
        sql
    );
}

#[test]
fn build_insert_sql_batches_rows_into_single_statement() {
    let plugin = PostgresPlugin::new();
    let metas = vec![meta("id", "INT"), meta("name", "VARCHAR(32)")];
    let rows = vec![
        vec![Some("1".to_string()), Some("alice".to_string())],
        vec![Some("2".to_string()), Some("o'hara".to_string())],
        vec![Some("3".to_string()), None],
    ];
    let sql = build_insert_sql(&plugin, "users", &metas, &rows);
    assert_eq!(
        "INSERT INTO \"users\" (\"id\", \"name\") VALUES (1, 'alice'), (2, 'o''hara'), (3, NULL);",
        sql
    );
}

#[test]
fn build_insert_sql_empty_batch_returns_empty_string() {
    let plugin = MySqlPlugin::new();
    let metas = vec![meta("id", "INT")];
    assert_eq!("", build_insert_sql(&plugin, "t", &metas, &[]));
    let rows = vec![vec![Some("1".to_string())]];
    assert_eq!("", build_insert_sql(&plugin, "t", &[], &rows));
}

#[test]
fn is_last_batch_covers_empty_and_exact_batch_boundaries() {
    // 空表：首批返回 0 行即为末批
    assert!(is_last_batch(0, 1000));
    // 恰好整批：需要再取一批确认结束
    assert!(!is_last_batch(1000, 1000));
    // 不足一批：已到最后
    assert!(is_last_batch(999, 1000));
    assert!(is_last_batch(0, 1));
    assert!(!is_last_batch(1, 1));
}

#[test]
fn build_select_batch_sql_uses_source_dialect_pagination() {
    let plugin = MySqlPlugin::new();
    assert_eq!(
        "SELECT * FROM `orders` LIMIT 1000 OFFSET 2000",
        build_select_batch_sql(&plugin, "orders", 1000, 2000)
    );
}

#[test]
fn build_create_view_sql_wraps_select_only_definition() {
    let plugin = PostgresPlugin::new();
    assert_eq!(
        "CREATE VIEW \"v_users\" AS SELECT id FROM users;",
        build_create_view_sql(&plugin, "v_users", "SELECT id FROM users;")
    );
}

#[test]
fn build_create_view_sql_passes_through_full_create_statement() {
    let plugin = MySqlPlugin::new();
    let definition =
        "CREATE ALGORITHM=UNDEFINED DEFINER=`root`@`%` SQL SECURITY DEFINER VIEW `v` AS SELECT 1;";
    assert_eq!(
        "CREATE ALGORITHM=UNDEFINED DEFINER=`root`@`%` SQL SECURITY DEFINER VIEW `v` AS SELECT 1;",
        build_create_view_sql(&plugin, "v", definition)
    );
}

use crate::table_data::filter_types::{
    ConditionItem, FilterGroup, FilterOperator, FilterState, FilterValue, LogicOperator,
    operators_for_column, uuid_simple,
};
#[cfg(test)]
use crate::table_data::filter_types::{is_datetime_type, is_numeric_type, is_string_type};
use db::ColumnInfo;
use gpui::prelude::*;
use gpui::{
    App, AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Render, SharedString, Styled, Window, px,
};
use gpui_component::input::{Input, InputState};
use gpui_component::{ActiveTheme, IconName, Sizable, checkbox::Checkbox};
use gpui_component::{
    IndexPath,
    button::{Button, ButtonVariants as _},
    select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
};
#[cfg(test)]
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Documentation, InsertReplaceEdit, Range,
};
use one_ui::edit_table::ColumnSort;

#[derive(Clone)]
pub struct TableSchema {
    pub columns: Vec<ColumnInfo>,
}

// ========== Completion providers ==========

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueSuggestionKind {
    General,
    LikePattern,
    InList,
    BetweenStart,
    BetweenEnd,
    NullOnly,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
enum SuggestionContext<'a> {
    Columns,
    Operators(&'a ColumnInfo),
    Values {
        column: Option<&'a ColumnInfo>,
        kind: ValueSuggestionKind,
    },
    IsKeywords,
    NotOperators(Option<&'a ColumnInfo>),
    Logic,
}

#[cfg(test)]
fn tokenize_where_context(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }

        if matches!(ch, '\'' | '"') {
            let quote = ch;
            index += 1;
            while index < chars.len() {
                if chars[index] == '\\' {
                    index += 2;
                    continue;
                }
                if chars[index] == quote {
                    index += 1;
                    break;
                }
                index += 1;
            }
            tokens.push("__STRING__".into());
            continue;
        }

        if ch.is_ascii_digit() {
            let start = index;
            index += 1;
            while index < chars.len() && (chars[index].is_ascii_digit() || chars[index] == '.') {
                index += 1;
            }
            let token: String = chars[start..index].iter().collect();
            tokens.push(token.to_uppercase());
            continue;
        }

        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
            let start = index;
            index += 1;
            while index < chars.len() {
                let next = chars[index];
                if !(next.is_ascii_alphanumeric() || next == '_' || next == '.') {
                    break;
                }
                index += 1;
            }
            let token: String = chars[start..index].iter().collect();
            tokens.push(token.to_uppercase());
            continue;
        }

        if let Some(next) = chars.get(index + 1) {
            let pair = match (ch, *next) {
                ('!', '=') => Some("!="),
                ('<', '=') => Some("<="),
                ('>', '=') => Some(">="),
                ('<', '>') => Some("<>"),
                _ => None,
            };
            if let Some(pair) = pair {
                tokens.push(pair.into());
                index += 2;
                continue;
            }
        }

        if matches!(ch, '=' | '<' | '>' | '(' | ')' | ',') {
            tokens.push(ch.to_string());
        }
        index += 1;
    }

    tokens
}

#[cfg(test)]
fn find_column<'a>(schema: &'a TableSchema, token: &str) -> Option<&'a ColumnInfo> {
    schema
        .columns
        .iter()
        .find(|column| column.name.eq_ignore_ascii_case(token))
}

#[cfg(test)]
fn find_preceding_column<'a>(
    schema: &'a TableSchema,
    tokens: &[String],
    skip_from_end: usize,
) -> Option<&'a ColumnInfo> {
    let limit = tokens.len().saturating_sub(skip_from_end);
    tokens[..limit]
        .iter()
        .rev()
        .find_map(|token| find_column(schema, token))
}

#[cfg(test)]
fn token_is_operator(token: &str) -> bool {
    matches!(
        token,
        "=" | "!=" | "<>" | ">" | "<" | ">=" | "<=" | "LIKE" | "IN" | "BETWEEN"
    )
}

#[cfg(test)]
fn token_is_value(token: &str) -> bool {
    token == "__STRING__"
        || token == ")"
        || token == "NULL"
        || token == "TRUE"
        || token == "FALSE"
        || token == "CURRENT_DATE"
        || token == "CURRENT_TIMESTAMP"
        || token == "NOW"
        || token.chars().next().is_some_and(|ch| ch.is_ascii_digit())
}

#[cfg(test)]
fn infer_suggestion_context<'a>(
    schema: &'a TableSchema,
    tokens: &[String],
) -> SuggestionContext<'a> {
    let Some(last) = tokens.last().map(String::as_str) else {
        return SuggestionContext::Columns;
    };

    match last {
        "(" => {
            let previous = tokens.iter().rev().nth(1).map(String::as_str);
            if previous == Some("IN") {
                SuggestionContext::Values {
                    column: find_preceding_column(schema, tokens, 2),
                    kind: ValueSuggestionKind::InList,
                }
            } else {
                SuggestionContext::Columns
            }
        }
        "AND" => {
            if tokens.len() >= 3
                && tokens[tokens.len() - 2].as_str() != "BETWEEN"
                && tokens[tokens.len() - 3].as_str() == "BETWEEN"
            {
                SuggestionContext::Values {
                    column: find_preceding_column(schema, tokens, 3),
                    kind: ValueSuggestionKind::BetweenEnd,
                }
            } else {
                SuggestionContext::Columns
            }
        }
        "OR" => SuggestionContext::Columns,
        "," => {
            if tokens.iter().rev().any(|token| token == "IN") {
                SuggestionContext::Values {
                    column: find_preceding_column(schema, tokens, 1),
                    kind: ValueSuggestionKind::InList,
                }
            } else {
                SuggestionContext::Columns
            }
        }
        "IS" => SuggestionContext::IsKeywords,
        "NOT" => {
            let previous = tokens.iter().rev().nth(1).map(String::as_str);
            if previous == Some("IS") {
                SuggestionContext::Values {
                    column: find_preceding_column(schema, tokens, 2),
                    kind: ValueSuggestionKind::NullOnly,
                }
            } else {
                SuggestionContext::NotOperators(find_preceding_column(schema, tokens, 1))
            }
        }
        "BETWEEN" => SuggestionContext::Values {
            column: find_preceding_column(schema, tokens, 1),
            kind: ValueSuggestionKind::BetweenStart,
        },
        token if token_is_operator(token) => SuggestionContext::Values {
            column: find_preceding_column(schema, tokens, 1),
            kind: if token == "LIKE" {
                ValueSuggestionKind::LikePattern
            } else if token == "IN" {
                ValueSuggestionKind::InList
            } else {
                ValueSuggestionKind::General
            },
        },
        token => {
            if let Some(column) = find_column(schema, token) {
                return SuggestionContext::Operators(column);
            }
            if token_is_value(token) {
                return SuggestionContext::Logic;
            }
            SuggestionContext::Columns
        }
    }
}

#[cfg(test)]
fn is_boolean_type(data_type: &str) -> bool {
    let data_type = data_type.to_uppercase();
    data_type.contains("BOOL") || data_type == "BOOLEAN" || data_type == "BIT"
}

#[cfg(test)]
fn suggest_items(
    schema: &TableSchema,
    current_word: &str,
    replace_range: Range,
    context_prefix: &str,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let tokens = tokenize_where_context(context_prefix);

    match infer_suggestion_context(schema, &tokens) {
        SuggestionContext::Columns => {
            suggest_columns(schema, current_word, replace_range, &mut items);
            suggest_functions(current_word, replace_range, &mut items);
        }
        SuggestionContext::Operators(column) => {
            suggest_operators(column, current_word, replace_range, &mut items);
        }
        SuggestionContext::Values { column, kind } => {
            suggest_value_templates(column, kind, current_word, replace_range, &mut items);
            if kind != ValueSuggestionKind::NullOnly {
                suggest_functions(current_word, replace_range, &mut items);
            }
        }
        SuggestionContext::IsKeywords => {
            suggest_is_keywords(current_word, replace_range, &mut items);
        }
        SuggestionContext::NotOperators(column) => {
            suggest_not_operators(column, current_word, replace_range, &mut items);
        }
        SuggestionContext::Logic => {
            add_logic_keywords(current_word, replace_range, &mut items);
        }
    }

    items.sort_by_key(|x| x.sort_text.clone().unwrap_or("9".into()));
    items
}

#[cfg(test)]
fn suggest_value_templates(
    column: Option<&ColumnInfo>,
    kind: ValueSuggestionKind,
    current_word: &str,
    range: Range,
    items: &mut Vec<CompletionItem>,
) {
    let data_type = column.map(|column| column.data_type.as_str()).unwrap_or("");
    let templates: Vec<(&str, &str, &str)> = match kind {
        ValueSuggestionKind::NullOnly => vec![("NULL", "NULL", "NULL value")],
        ValueSuggestionKind::LikePattern => vec![
            ("'%...%'", "'%'", "Contains pattern"),
            ("'...%'", "'%'", "Starts with pattern"),
            ("'%...'", "'%'", "Ends with pattern"),
            ("NULL", "NULL", "NULL value"),
        ],
        ValueSuggestionKind::BetweenStart | ValueSuggestionKind::BetweenEnd
            if is_datetime_type(data_type) =>
        {
            vec![
                ("'2024-01-01'", "'2024-01-01'", "Date value"),
                ("NOW()", "NOW()", "Current timestamp"),
                ("CURRENT_DATE", "CURRENT_DATE", "Current date"),
            ]
        }
        ValueSuggestionKind::BetweenStart | ValueSuggestionKind::BetweenEnd
            if is_numeric_type(data_type) =>
        {
            vec![
                ("0", "0", "Numeric value"),
                ("1", "1", "Numeric value"),
                ("NULL", "NULL", "NULL value"),
            ]
        }
        _ if is_boolean_type(data_type) => vec![
            ("true", "true", "Boolean true"),
            ("false", "false", "Boolean false"),
            ("NULL", "NULL", "NULL value"),
        ],
        _ if is_numeric_type(data_type) => vec![
            ("0", "0", "Numeric value"),
            ("1", "1", "Numeric value"),
            ("NULL", "NULL", "NULL value"),
        ],
        _ if is_datetime_type(data_type) => vec![
            ("'2024-01-01'", "'2024-01-01'", "Date value"),
            ("NOW()", "NOW()", "Current timestamp"),
            ("CURRENT_DATE", "CURRENT_DATE", "Current date"),
            ("NULL", "NULL", "NULL value"),
        ],
        _ => vec![
            ("'...'", "''", "String value"),
            ("NULL", "NULL", "NULL value"),
            ("true", "true", "Boolean true"),
            ("false", "false", "Boolean false"),
        ],
    };

    for (label, text, doc) in templates {
        if label
            .to_uppercase()
            .starts_with(&current_word.to_uppercase())
            || current_word.is_empty()
        {
            items.push(CompletionItem {
                label: label.to_string(),
                kind: Some(CompletionItemKind::VALUE),
                documentation: Some(Documentation::String(doc.to_string())),
                text_edit: Some(insert_replace(text, range)),
                sort_text: Some("0_VALUE".into()),
                ..Default::default()
            });
        }
    }
}

#[cfg(test)]
fn suggest_columns(
    schema: &TableSchema,
    current_word: &str,
    replace_range: Range,
    items: &mut Vec<CompletionItem>,
) {
    for col in &schema.columns {
        if col.name.to_uppercase().starts_with(current_word) || current_word.is_empty() {
            let detail = if col.is_nullable {
                format!("{} (nullable)", col.data_type)
            } else {
                format!("{} (not null)", col.data_type)
            };

            items.push(CompletionItem {
                label: col.name.clone(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some(detail),
                documentation: Some(Documentation::String(format!(
                    "Column: {}\nType: {}\nNullable: {}",
                    col.name, col.data_type, col.is_nullable
                ))),
                sort_text: Some("2_COLUMN".into()),
                text_edit: Some(insert_replace(&col.name, replace_range)),
                ..Default::default()
            });
        }
    }
}

#[cfg(test)]
fn suggest_operators(
    col: &ColumnInfo,
    current_word: &str,
    range: Range,
    items: &mut Vec<CompletionItem>,
) {
    let dt = col.data_type.to_uppercase();
    let ops: Vec<(&str, &str, &str)> =
        if dt.contains("CHAR") || dt.contains("TEXT") || dt.contains("VARCHAR") {
            vec![
                ("= ''", "= ''", "Equal to"),
                ("!= ''", "!= ''", "Not equal to"),
                ("LIKE '%%'", "LIKE '%%'", "Pattern match (contains)"),
                ("LIKE '%'", "LIKE '%'", "Pattern match (starts with)"),
                ("IN ()", "IN ()", "In list"),
                ("IS NULL", "IS NULL", "Is null"),
                ("IS NOT NULL", "IS NOT NULL", "Is not null"),
            ]
        } else if dt.contains("INT")
            || dt.contains("DECIMAL")
            || dt.contains("FLOAT")
            || dt.contains("DOUBLE")
            || dt.contains("NUMERIC")
        {
            vec![
                ("=", "= ", "Equal to"),
                ("!=", "!= ", "Not equal to"),
                ("<", "< ", "Less than"),
                (">", "> ", "Greater than"),
                ("<=", "<= ", "Less than or equal"),
                (">=", ">= ", "Greater than or equal"),
                ("BETWEEN", "BETWEEN  AND ", "Between range"),
                ("IN ()", "IN ()", "In list"),
                ("IS NULL", "IS NULL", "Is null"),
                ("IS NOT NULL", "IS NOT NULL", "Is not null"),
            ]
        } else if dt.contains("DATE") || dt.contains("TIME") {
            vec![
                ("= ''", "= ''", "Equal to"),
                ("!= ''", "!= ''", "Not equal to"),
                ("< ''", "< ''", "Before"),
                ("> ''", "> ''", "After"),
                ("BETWEEN '' AND ''", "BETWEEN '' AND ''", "Between dates"),
                ("IS NULL", "IS NULL", "Is null"),
                ("IS NOT NULL", "IS NOT NULL", "Is not null"),
            ]
        } else {
            vec![
                ("=", "= ", "Equal to"),
                ("!=", "!= ", "Not equal to"),
                ("IS NULL", "IS NULL", "Is null"),
                ("IS NOT NULL", "IS NOT NULL", "Is not null"),
            ]
        };

    for (label, text, doc) in ops {
        if !current_word.is_empty()
            && !label
                .to_uppercase()
                .starts_with(&current_word.to_uppercase())
        {
            continue;
        }

        items.push(CompletionItem {
            label: label.to_string(),
            kind: Some(CompletionItemKind::OPERATOR),
            detail: Some(format!("{} ({})", doc, col.data_type)),
            documentation: Some(Documentation::String(format!(
                "{}\n\nColumn: {} ({})",
                doc, col.name, col.data_type
            ))),
            text_edit: Some(insert_replace(text, range)),
            sort_text: Some("1_OPERATOR".into()),
            ..Default::default()
        });
    }
}

#[cfg(test)]
fn add_logic_keywords(current_word: &str, range: Range, items: &mut Vec<CompletionItem>) {
    let keywords = [
        ("AND", "AND ", "Logical AND - both conditions must be true"),
        (
            "OR",
            "OR ",
            "Logical OR - at least one condition must be true",
        ),
    ];

    for (label, snippet, doc) in &keywords {
        if label.starts_with(&current_word.to_uppercase()) || current_word.is_empty() {
            items.push(CompletionItem {
                label: label.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                documentation: Some(Documentation::String(doc.to_string())),
                text_edit: Some(insert_replace(snippet, range)),
                sort_text: Some("3_LOGIC".into()),
                ..Default::default()
            });
        }
    }
}

#[cfg(test)]
fn suggest_is_keywords(current_word: &str, range: Range, items: &mut Vec<CompletionItem>) {
    let keywords = [
        ("NULL", "NULL", "NULL value"),
        ("NOT NULL", "NOT NULL", "Not null value"),
    ];

    for (label, text, doc) in keywords {
        if label.starts_with(&current_word.to_uppercase()) || current_word.is_empty() {
            items.push(CompletionItem {
                label: label.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                documentation: Some(Documentation::String(doc.to_string())),
                text_edit: Some(insert_replace(text, range)),
                sort_text: Some("0_IS".into()),
                ..Default::default()
            });
        }
    }
}

#[cfg(test)]
fn suggest_not_operators(
    column: Option<&ColumnInfo>,
    current_word: &str,
    range: Range,
    items: &mut Vec<CompletionItem>,
) {
    let data_type = column.map(|column| column.data_type.as_str()).unwrap_or("");
    let ops: Vec<(&str, &str, &str)> = if is_string_type(data_type) {
        vec![
            ("LIKE", "LIKE ", "Negated pattern match"),
            ("IN", "IN ", "Negated list match"),
        ]
    } else if is_numeric_type(data_type) || is_datetime_type(data_type) {
        vec![
            ("IN", "IN ", "Negated list match"),
            ("BETWEEN", "BETWEEN ", "Negated range match"),
        ]
    } else {
        vec![("IN", "IN ", "Negated list match")]
    };

    for (label, text, doc) in ops {
        if !current_word.is_empty() && !label.starts_with(&current_word.to_uppercase()) {
            continue;
        }

        items.push(CompletionItem {
            label: label.to_string(),
            kind: Some(CompletionItemKind::OPERATOR),
            documentation: Some(Documentation::String(doc.to_string())),
            text_edit: Some(insert_replace(text, range)),
            sort_text: Some("0_NOT".into()),
            ..Default::default()
        });
    }
}

#[cfg(test)]
fn suggest_functions(current_word: &str, range: Range, items: &mut Vec<CompletionItem>) {
    let fns = [
        ("UPPER()", "UPPER()", "Convert to uppercase", "String"),
        ("LOWER()", "LOWER()", "Convert to lowercase", "String"),
        ("LENGTH()", "LENGTH()", "Get string length", "String"),
        ("TRIM()", "TRIM()", "Remove spaces", "String"),
        ("CONCAT()", "CONCAT(, )", "Concatenate strings", "String"),
        (
            "SUBSTRING()",
            "SUBSTRING(, , )",
            "Extract substring",
            "String",
        ),
        ("DATE()", "DATE()", "Extract date part", "Date"),
        ("YEAR()", "YEAR()", "Extract year", "Date"),
        ("MONTH()", "MONTH()", "Extract month", "Date"),
        ("DAY()", "DAY()", "Extract day", "Date"),
        ("NOW()", "NOW()", "Current timestamp", "Date"),
        ("COALESCE()", "COALESCE(, )", "First non-null", "Utility"),
        ("CAST()", "CAST( AS )", "Convert type", "Utility"),
    ];

    for (label, text, doc, category) in &fns {
        if label
            .to_uppercase()
            .starts_with(&current_word.to_uppercase())
            || current_word.is_empty()
        {
            items.push(CompletionItem {
                label: label.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(category.to_string()),
                documentation: Some(Documentation::String(doc.to_string())),
                text_edit: Some(insert_replace(text, range)),
                sort_text: Some("4_function".into()),
                ..Default::default()
            });
        }
    }
}

#[cfg(test)]
fn insert_replace(text: &str, range: Range) -> CompletionTextEdit {
    CompletionTextEdit::InsertAndReplace(InsertReplaceEdit {
        new_text: text.into(),
        insert: range,
        replace: range,
    })
}

// ========== 可视化筛选构建器 ==========

#[derive(Clone)]
pub enum FilterEditorEvent {
    QueryApply,
}

/// 筛选项的 Select 数据项
#[derive(Clone, Debug)]
pub struct FilterColumnItem {
    pub name: String,
}

impl SelectItem for FilterColumnItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.name.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.name
    }
}

/// 操作符的 Select 数据项
#[derive(Clone, Debug)]
pub struct FilterOperatorItem {
    pub op: FilterOperator,
}

impl SelectItem for FilterOperatorItem {
    type Value = FilterOperator;

    fn title(&self) -> SharedString {
        self.op.label().into()
    }

    fn value(&self) -> &Self::Value {
        &self.op
    }
}

/// 单个筛选条件行的状态
#[derive(Clone)]
struct ConditionRow {
    id: String,
    enabled: bool,
    column: String,
    operator: FilterOperator,
    value_text: String,
    value_start: String,
    value_end: String,
    /// 连接到此条件的逻辑操作符（用于显示在条件前的 AND/OR）
    logic_operator: LogicOperator,
}

impl ConditionRow {
    fn new(column: String, operator: FilterOperator, logic_op: LogicOperator) -> Self {
        Self {
            id: uuid_simple(),
            enabled: true,
            column,
            operator,
            value_text: String::new(),
            value_start: String::new(),
            value_end: String::new(),
            logic_operator: logic_op,
        }
    }

    /// 检查条件是否有效（所有必填值都已填写）
    fn is_valid(&self) -> bool {
        if self.operator.needs_two_values() {
            // BETWEEN 需要两个值
            !self.value_start.trim().is_empty() && !self.value_end.trim().is_empty()
        } else if self.operator.needs_list_value() {
            // IN/NOT IN 需要至少一个值
            !self.value_text.trim().is_empty()
        } else if self.operator.needs_value() {
            // 其他需要单个值
            !self.value_text.trim().is_empty()
        } else {
            // IS NULL / IS NOT NULL 不需要值
            true
        }
    }

    fn to_condition_item(&self) -> ConditionItem {
        let value = if self.operator.needs_two_values() {
            FilterValue::Range {
                start: self.value_start.clone(),
                end: self.value_end.clone(),
            }
        } else if self.operator.needs_list_value() {
            FilterValue::List(self.value_text.clone())
        } else {
            FilterValue::Single(self.value_text.clone())
        };

        // 如果条件无效，设置为 disabled
        let enabled = self.enabled && self.is_valid();

        ConditionItem {
            column: self.column.clone(),
            operator: self.operator,
            value,
            enabled,
            logic_operator: self.logic_operator,
        }
    }
}

/// 分组行的状态
#[derive(Clone)]
struct GroupRow {
    id: String,
    enabled: bool,
    logic_operator: LogicOperator,
    /// 分组内的子项（条件或嵌套分组）
    children: Vec<FilterItem>,
}

impl GroupRow {
    fn new(logic_op: LogicOperator) -> Self {
        Self {
            id: uuid_simple(),
            enabled: true,
            logic_operator: logic_op,
            children: Vec::new(),
        }
    }
}

/// 可视化筛选项（条件或分组）
#[derive(Clone)]
enum FilterItem {
    Condition(ConditionRow),
    Group(GroupRow),
}

pub struct VisualFilterBuilder {
    schema: Option<TableSchema>,
    filter_state: FilterState,
    collapsed_groups: std::collections::HashSet<String>,
    /// 根级别的筛选项（条件或分组）
    root_items: Vec<FilterItem>,
    /// 列选择器实体，按行 ID 索引
    column_selects:
        std::collections::HashMap<String, Entity<SelectState<SearchableVec<FilterColumnItem>>>>,
    /// 操作符选择器实体，按行 ID 索引
    operator_selects:
        std::collections::HashMap<String, Entity<SelectState<SearchableVec<FilterOperatorItem>>>>,
    /// 值输入框实体，按行 ID 索引
    value_inputs: std::collections::HashMap<String, Entity<InputState>>,
    /// 范围起始值输入框实体，按行 ID 索引（BETWEEN 时使用）
    value_start_inputs: std::collections::HashMap<String, Entity<InputState>>,
    /// 范围结束值输入框实体，按行 ID 索引（BETWEEN 时使用）
    value_end_inputs: std::collections::HashMap<String, Entity<InputState>>,
    /// 输入框订阅，按行 ID 索引（避免被 drop）
    value_subscriptions: std::collections::HashMap<String, gpui::Subscription>,
}

// ========== 递归辅助函数（避免 borrow checker 问题）==========

/// 递归查找并更新条件列，同时返回是否需要重置操作符
fn update_condition_column_in_items(
    items: &mut Vec<FilterItem>,
    id: &str,
    column: String,
    valid_operators: &[FilterOperator],
) -> Option<FilterOperator> {
    for item in items.iter_mut() {
        match item {
            FilterItem::Condition(row) if row.id == id => {
                // 检查当前操作符是否对新列有效
                let need_reset = !valid_operators.contains(&row.operator);
                row.column = column;
                return if need_reset { valid_operators.first().copied() } else { None };
            }
            FilterItem::Group(group) => {
                if let Some(new_op) =
                    update_condition_column_in_items(&mut group.children, id, column.clone(), valid_operators)
                {
                    return Some(new_op);
                }
            }
            _ => {}
        }
    }
    None
}

/// 递归查找并更新条件操作符
fn update_condition_operator_in_items(
    items: &mut Vec<FilterItem>,
    id: &str,
    operator: FilterOperator,
) -> bool {
    for item in items.iter_mut() {
        match item {
            FilterItem::Condition(row) if row.id == id => {
                row.operator = operator;
                return true;
            }
            FilterItem::Group(group) => {
                if update_condition_operator_in_items(&mut group.children, id, operator) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// 递归查找并更新条件值
fn update_condition_value_in_items(items: &mut Vec<FilterItem>, id: &str, value: &str) -> bool {
    for item in items.iter_mut() {
        match item {
            FilterItem::Condition(row) if row.id == id => {
                row.value_text = value.to_string();
                return true;
            }
            FilterItem::Group(group) => {
                if update_condition_value_in_items(&mut group.children, id, value) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// 递归查找并更新范围起始值
fn update_condition_value_start_in_items(
    items: &mut Vec<FilterItem>,
    id: &str,
    value: &str,
) -> bool {
    for item in items.iter_mut() {
        match item {
            FilterItem::Condition(row) if row.id == id => {
                row.value_start = value.to_string();
                return true;
            }
            FilterItem::Group(group) => {
                if update_condition_value_start_in_items(&mut group.children, id, value) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// 递归查找并更新范围结束值
fn update_condition_value_end_in_items(items: &mut Vec<FilterItem>, id: &str, value: &str) -> bool {
    for item in items.iter_mut() {
        match item {
            FilterItem::Condition(row) if row.id == id => {
                row.value_end = value.to_string();
                return true;
            }
            FilterItem::Group(group) => {
                if update_condition_value_end_in_items(&mut group.children, id, value) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// 递归切换条件启用状态
fn toggle_condition_in_items(items: &mut Vec<FilterItem>, id: &str) -> bool {
    for item in items.iter_mut() {
        match item {
            FilterItem::Condition(row) if row.id == id => {
                row.enabled = !row.enabled;
                return true;
            }
            FilterItem::Group(group) => {
                if toggle_condition_in_items(&mut group.children, id) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// 递归切换条件的逻辑操作符
fn toggle_condition_logic_in_items(items: &mut Vec<FilterItem>, id: &str) -> bool {
    for item in items.iter_mut() {
        match item {
            FilterItem::Condition(row) if row.id == id => {
                row.logic_operator = match row.logic_operator {
                    LogicOperator::And => LogicOperator::Or,
                    LogicOperator::Or => LogicOperator::And,
                };
                return true;
            }
            FilterItem::Group(group) => {
                if toggle_condition_logic_in_items(&mut group.children, id) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// 递归查找分组
fn find_group_mut(items: &mut Vec<FilterItem>, id: &str) -> Option<usize> {
    for (i, item) in items.iter_mut().enumerate() {
        match item {
            FilterItem::Group(group) if group.id == id => {
                return Some(i);
            }
            FilterItem::Group(group) => {
                if let Some(idx) = find_group_mut(&mut group.children, id) {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }
    None
}

/// 递归从 items 中删除条件，返回是否找到并删除
fn delete_condition_from_items(items: &mut Vec<FilterItem>, id: &str) -> bool {
    for i in 0..items.len() {
        match &mut items[i] {
            FilterItem::Condition(row) if row.id == id => {
                items.remove(i);
                return true;
            }
            FilterItem::Group(group) => {
                if delete_condition_from_items(&mut group.children, id) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// 递归从 items 中删除分组，返回是否找到并删除
fn delete_group_from_items(items: &mut Vec<FilterItem>, id: &str) -> bool {
    for i in 0..items.len() {
        match &mut items[i] {
            FilterItem::Group(group) if group.id == id => {
                items.remove(i);
                return true;
            }
            FilterItem::Group(group) => {
                if delete_group_from_items(&mut group.children, id) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

impl VisualFilterBuilder {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            schema: None,
            filter_state: FilterState::new(),
            collapsed_groups: std::collections::HashSet::new(),
            root_items: Vec::new(),
            column_selects: std::collections::HashMap::new(),
            operator_selects: std::collections::HashMap::new(),
            value_inputs: std::collections::HashMap::new(),
            value_start_inputs: std::collections::HashMap::new(),
            value_end_inputs: std::collections::HashMap::new(),
            value_subscriptions: std::collections::HashMap::new(),
        }
    }

    pub fn set_schema(&mut self, schema: TableSchema, _cx: &mut Context<Self>) {
        self.schema = Some(schema);
        // 通知重新渲染以更新列选择器
        _cx.notify();
    }

    pub fn get_where_clause(&self) -> String {
        self.filter_state.to_where_clause()
    }

    pub fn get_order_by_clause(&self) -> String {
        self.filter_state.to_order_by_clause()
    }

    pub fn add_sort_column(
        &mut self,
        column: &str,
        direction: ColumnSort,
        _cx: &mut Context<Self>,
    ) {
        tracing::info!(
            "[SORT] add_sort_column: column={}, dir={:?}",
            column,
            direction
        );

        self.filter_state.apply_header_sort(column, direction);

        tracing::info!(
            "[SORT] add_sort_column: sorts now = {:?}",
            self.filter_state
                .sorts
                .iter()
                .map(|s| format!("{} {}", s.column, s.direction.label()))
                .collect::<Vec<_>>()
        );
    }

    /// 从 root_items 树同步到 filter_state
    fn sync_filter_state(&mut self) {
        self.filter_state.root = FilterGroup::new(LogicOperator::And);
        for item in &self.root_items {
            match item {
                FilterItem::Condition(row) => {
                    let condition = row.to_condition_item();
                    self.filter_state.root.add_condition(condition);
                }
                FilterItem::Group(group_row) => {
                    let group = self.build_filter_group_from_row(group_row);
                    self.filter_state.root.add_group(group);
                }
            }
        }
    }

    /// 从 GroupRow 递归构建 FilterGroup
    fn build_filter_group_from_row(&self, row: &GroupRow) -> FilterGroup {
        let mut group = FilterGroup::new(row.logic_operator);
        group.enabled = row.enabled;
        for item in &row.children {
            match item {
                FilterItem::Condition(cr) => {
                    group.add_condition(cr.to_condition_item());
                }
                FilterItem::Group(gr) => {
                    let nested = self.build_filter_group_from_row(gr);
                    group.add_group(nested);
                }
            }
        }
        group
    }

    fn add_condition(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let schema = self.schema.clone();
        let first_col = schema
            .as_ref()
            .and_then(|s| s.columns.first())
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let first_op = schema
            .as_ref()
            .and_then(|s| s.columns.first())
            .map(operators_for_column)
            .and_then(|ops| ops.first().copied())
            .unwrap_or(FilterOperator::Equal);

        // 第一条条件使用 AND，后续条件从其 logic_operator 字段读取
        let row = ConditionRow::new(first_col.clone(), first_op, LogicOperator::And);
        let row_id = row.id.clone();

        // 创建列选择器
        let column_items: Vec<FilterColumnItem> = schema
            .as_ref()
            .map(|s| {
                s.columns
                    .iter()
                    .map(|c| FilterColumnItem {
                        name: c.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let selected_col_index = schema
            .as_ref()
            .and_then(|s| s.columns.iter().position(|c| c.name == first_col))
            .map(|i| IndexPath::new(i));

        let column_select_entity = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(column_items),
                selected_col_index,
                window,
                cx,
            )
        });

        // 订阅列选择事件
        let row_id_clone_for_col = row_id.clone();
        cx.subscribe(
            &column_select_entity,
            move |this, _, event: &SelectEvent<SearchableVec<FilterColumnItem>>, _cx| {
                let SelectEvent::Confirm(value) = event;
                if let Some(col_name) = value {
                    this.update_condition_column(&row_id_clone_for_col, col_name.clone());
                }
            },
        )
        .detach();

        // 创建范围值输入框（BETWEEN 时使用）
        let value_start_input_entity =
            cx.new(|cx| InputState::new(window, cx).placeholder("起始值".to_string()));
        let value_end_input_entity =
            cx.new(|cx| InputState::new(window, cx).placeholder("结束值".to_string()));

        // 观察范围起始值输入变化
        let row_id_clone_for_val_start = row_id.clone();
        let value_start_input_clone = value_start_input_entity.clone();
        let val_start_sub = cx.observe(&value_start_input_entity, move |this, _, cx| {
            let text = value_start_input_clone.read(cx).text().to_string();
            this.update_condition_value_start(&row_id_clone_for_val_start, text);
            cx.notify();
        });

        // 观察范围结束值输入变化
        let row_id_clone_for_val_end = row_id.clone();
        let value_end_input_clone = value_end_input_entity.clone();
        let val_end_sub = cx.observe(&value_end_input_entity, move |this, _, cx| {
            let text = value_end_input_clone.read(cx).text().to_string();
            this.update_condition_value_end(&row_id_clone_for_val_end, text);
            cx.notify();
        });

        // 创建操作符选择器
        let operator_items: Vec<FilterOperatorItem> = schema
            .as_ref()
            .and_then(|s| s.columns.iter().find(|c| c.name == first_col))
            .map(operators_for_column)
            .map(|ops| {
                ops.iter()
                    .map(|op| FilterOperatorItem { op: *op })
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![
                    FilterOperatorItem {
                        op: FilterOperator::Equal,
                    },
                    FilterOperatorItem {
                        op: FilterOperator::NotEqual,
                    },
                    FilterOperatorItem {
                        op: FilterOperator::IsNull,
                    },
                    FilterOperatorItem {
                        op: FilterOperator::IsNotNull,
                    },
                ]
            });

        let selected_op_index = operator_items.iter().position(|item| item.op == first_op);

        let operator_select_entity = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(operator_items),
                selected_op_index.map(|i| IndexPath::new(i)),
                window,
                cx,
            )
        });

        // 订阅操作符选择事件
        let row_id_clone_for_op = row_id.clone();
        cx.subscribe(
            &operator_select_entity,
            move |this, _, event: &SelectEvent<SearchableVec<FilterOperatorItem>>, _cx| {
                let SelectEvent::Confirm(value) = event;
                if let Some(op) = value {
                    this.update_condition_operator(&row_id_clone_for_op, *op);
                }
            },
        )
        .detach();

        // 创建值输入框
        let value_input_entity =
            cx.new(|cx| InputState::new(window, cx).placeholder("输入值...".to_string()));

        // 观察值输入变化
        let row_id_clone_for_val = row_id.clone();
        let value_input_clone = value_input_entity.clone();
        let val_sub = cx.observe(&value_input_entity, move |this, _, cx| {
            let text = value_input_clone.read(cx).text().to_string();
            this.update_condition_value(&row_id_clone_for_val, text);
            cx.notify();
        });

        self.root_items.push(FilterItem::Condition(row));
        self.column_selects
            .insert(row_id.clone(), column_select_entity);
        self.operator_selects
            .insert(row_id.clone(), operator_select_entity);
        self.value_inputs.insert(row_id.clone(), value_input_entity);
        self.value_start_inputs
            .insert(row_id.clone(), value_start_input_entity);
        self.value_end_inputs
            .insert(row_id.clone(), value_end_input_entity);
        // 存储订阅，防止被 drop
        self.value_subscriptions.insert(row_id.clone(), val_sub);
        self.value_subscriptions
            .insert(format!("{}_start", row_id), val_start_sub);
        self.value_subscriptions
            .insert(format!("{}_end", row_id), val_end_sub);
        self.sync_filter_state();
        cx.notify();
    }

    /// 在所有分组中递归查找并更新条件列
    fn update_condition_column(&mut self, id: &str, column: String) {
        // 获取新列的操作符列表
        let valid_operators: Vec<FilterOperator> = self
            .schema
            .as_ref()
            .and_then(|s| s.columns.iter().find(|c| c.name == column))
            .map(operators_for_column)
            .unwrap_or_else(|| vec![FilterOperator::Equal]);

        if let Some(new_op) =
            update_condition_column_in_items(&mut self.root_items, id, column, &valid_operators)
        {
            // 需要重置操作符
            self.update_condition_operator(id, new_op);
        }
        self.sync_filter_state();
    }

    /// 在所有分组中递归查找并更新条件操作符
    fn update_condition_operator(&mut self, id: &str, operator: FilterOperator) {
        if update_condition_operator_in_items(&mut self.root_items, id, operator) {
            self.sync_filter_state();
        }
    }

    fn add_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut group_row = GroupRow::new(LogicOperator::And);

        // 创建默认条件
        let (condition_row, column_select_entity, operator_select_entity, value_input_entity) =
            self.create_default_condition(window, cx);
        let condition_id = condition_row.id.clone();
        group_row
            .children
            .push(FilterItem::Condition(condition_row));
        self.root_items.push(FilterItem::Group(group_row));

        // 存储 UI 组件
        self.column_selects
            .insert(condition_id.clone(), column_select_entity);
        self.operator_selects
            .insert(condition_id.clone(), operator_select_entity);
        self.value_inputs.insert(condition_id, value_input_entity);

        self.sync_filter_state();
        cx.notify();
    }

    /// 清除所有筛选条件
    fn clear_all(&mut self, cx: &mut Context<Self>) {
        self.root_items.clear();
        self.collapsed_groups.clear();
        self.column_selects.clear();
        self.operator_selects.clear();
        self.value_inputs.clear();
        self.value_start_inputs.clear();
        self.value_end_inputs.clear();
        self.value_subscriptions.clear();
        self.filter_state = FilterState::new();
        cx.notify();
    }

    /// 添加分组到指定分组内
    /// 创建默认条件行及其 UI 组件
    fn create_default_condition(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (
        ConditionRow,
        Entity<SelectState<SearchableVec<FilterColumnItem>>>,
        Entity<SelectState<SearchableVec<FilterOperatorItem>>>,
        Entity<InputState>,
    ) {
        let schema = self.schema.clone();
        let first_col = schema
            .as_ref()
            .and_then(|s| s.columns.first())
            .map(|c| c.name.clone())
            .unwrap_or_default();
        let first_op = schema
            .as_ref()
            .and_then(|s| s.columns.first())
            .map(operators_for_column)
            .and_then(|ops| ops.first().copied())
            .unwrap_or(FilterOperator::Equal);

        let condition_row = ConditionRow::new(first_col.clone(), first_op, LogicOperator::And);

        let column_items: Vec<FilterColumnItem> = schema
            .as_ref()
            .map(|s| {
                s.columns
                    .iter()
                    .map(|c| FilterColumnItem {
                        name: c.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let selected_col_index = schema
            .as_ref()
            .and_then(|s| s.columns.iter().position(|c| c.name == first_col))
            .map(|i| IndexPath::new(i));

        let column_select_entity = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(column_items),
                selected_col_index,
                window,
                cx,
            )
        });

        let operator_items: Vec<FilterOperatorItem> = schema
            .as_ref()
            .and_then(|s| s.columns.first())
            .map(operators_for_column)
            .map(|ops| {
                ops.iter()
                    .map(|op| FilterOperatorItem { op: *op })
                    .collect()
            })
            .unwrap_or_default();

        let selected_op_index = operator_items.iter().position(|item| item.op == first_op);

        let operator_select_entity = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(operator_items),
                selected_op_index.map(|i| IndexPath::new(i)),
                window,
                cx,
            )
        });

        let value_input_entity =
            cx.new(|cx| InputState::new(window, cx).placeholder("输入值...".to_string()));

        (
            condition_row,
            column_select_entity,
            operator_select_entity,
            value_input_entity,
        )
    }

    fn add_group_to_group(
        &mut self,
        parent_group_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(idx) = find_group_mut(&mut self.root_items, parent_group_id) {
            if let FilterItem::Group(parent) = &mut self.root_items[idx] {
                let schema = self.schema.clone();
                let first_col = schema
                    .as_ref()
                    .and_then(|s| s.columns.first())
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let first_op = schema
                    .as_ref()
                    .and_then(|s| s.columns.first())
                    .map(operators_for_column)
                    .and_then(|ops| ops.first().copied())
                    .unwrap_or(FilterOperator::Equal);

                let condition_row =
                    ConditionRow::new(first_col.clone(), first_op, LogicOperator::And);
                let condition_id = condition_row.id.clone();

                let column_items: Vec<FilterColumnItem> = schema
                    .as_ref()
                    .map(|s| {
                        s.columns
                            .iter()
                            .map(|c| FilterColumnItem {
                                name: c.name.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let selected_col_index = schema
                    .as_ref()
                    .and_then(|s| s.columns.iter().position(|c| c.name == first_col))
                    .map(|i| IndexPath::new(i));

                let column_select_entity = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(column_items),
                        selected_col_index,
                        window,
                        cx,
                    )
                });

                let operator_items: Vec<FilterOperatorItem> = schema
                    .as_ref()
                    .and_then(|s| s.columns.first())
                    .map(operators_for_column)
                    .map(|ops| {
                        ops.iter()
                            .map(|op| FilterOperatorItem { op: *op })
                            .collect()
                    })
                    .unwrap_or_default();

                let selected_op_index = operator_items.iter().position(|item| item.op == first_op);

                let operator_select_entity = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(operator_items),
                        selected_op_index.map(|i| IndexPath::new(i)),
                        window,
                        cx,
                    )
                });

                let value_input_entity =
                    cx.new(|cx| InputState::new(window, cx).placeholder("输入值...".to_string()));

                let mut group_row = GroupRow::new(LogicOperator::And);
                group_row
                    .children
                    .push(FilterItem::Condition(condition_row));
                parent.children.push(FilterItem::Group(group_row));

                self.column_selects
                    .insert(condition_id.clone(), column_select_entity);
                self.operator_selects
                    .insert(condition_id.clone(), operator_select_entity);
                self.value_inputs.insert(condition_id, value_input_entity);

                self.collapsed_groups.remove(parent_group_id);
                self.sync_filter_state();
                cx.notify();
            }
        }
    }

    /// 添加条件到指定分组内
    fn add_condition_to_group(
        &mut self,
        parent_group_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(idx) = find_group_mut(&mut self.root_items, parent_group_id) {
            if let FilterItem::Group(parent) = &mut self.root_items[idx] {
                let schema = self.schema.clone();
                let first_col = schema
                    .as_ref()
                    .and_then(|s| s.columns.first())
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                let first_op = schema
                    .as_ref()
                    .and_then(|s| s.columns.first())
                    .map(operators_for_column)
                    .and_then(|ops| ops.first().copied())
                    .unwrap_or(FilterOperator::Equal);

                let row = ConditionRow::new(first_col.clone(), first_op, LogicOperator::And);
                let row_id = row.id.clone();

                // 创建列选择器
                let column_items: Vec<FilterColumnItem> = schema
                    .as_ref()
                    .map(|s| {
                        s.columns
                            .iter()
                            .map(|c| FilterColumnItem {
                                name: c.name.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let selected_col_index = schema
                    .as_ref()
                    .and_then(|s| s.columns.iter().position(|c| c.name == first_col))
                    .map(|i| IndexPath::new(i));

                let column_select_entity = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(column_items),
                        selected_col_index,
                        window,
                        cx,
                    )
                });

                // 订阅列选择事件
                let row_id_clone = row_id.clone();
                cx.subscribe(
                    &column_select_entity,
                    move |this, _, event: &SelectEvent<SearchableVec<FilterColumnItem>>, _cx| {
                        let SelectEvent::Confirm(value) = event;
                        if let Some(col_name) = value {
                            this.update_condition_column(&row_id_clone, col_name.clone());
                        }
                    },
                )
                .detach();

                // 创建范围值输入框
                let value_start_input_entity =
                    cx.new(|cx| InputState::new(window, cx).placeholder("起始值".to_string()));
                let value_end_input_entity =
                    cx.new(|cx| InputState::new(window, cx).placeholder("结束值".to_string()));

                // 观察范围起始值输入变化
                let row_id_clone_start = row_id.clone();
                let value_start_input_clone = value_start_input_entity.clone();
                let val_start_sub = cx.observe(&value_start_input_entity, move |this, _, cx| {
                    let text = value_start_input_clone.read(cx).text().to_string();
                    this.update_condition_value_start(&row_id_clone_start, text);
                    cx.notify();
                });

                // 观察范围结束值输入变化
                let row_id_clone_end = row_id.clone();
                let value_end_input_clone = value_end_input_entity.clone();
                let val_end_sub = cx.observe(&value_end_input_entity, move |this, _, cx| {
                    let text = value_end_input_clone.read(cx).text().to_string();
                    this.update_condition_value_end(&row_id_clone_end, text);
                    cx.notify();
                });

                // 创建操作符选择器
                let operator_items: Vec<FilterOperatorItem> = schema
                    .as_ref()
                    .and_then(|s| s.columns.iter().find(|c| c.name == first_col))
                    .map(operators_for_column)
                    .map(|ops| {
                        ops.iter()
                            .map(|op| FilterOperatorItem { op: *op })
                            .collect()
                    })
                    .unwrap_or_else(|| {
                        vec![
                            FilterOperatorItem {
                                op: FilterOperator::Equal,
                            },
                            FilterOperatorItem {
                                op: FilterOperator::NotEqual,
                            },
                            FilterOperatorItem {
                                op: FilterOperator::IsNull,
                            },
                            FilterOperatorItem {
                                op: FilterOperator::IsNotNull,
                            },
                        ]
                    });

                let selected_op_index = operator_items.iter().position(|item| item.op == first_op);

                let operator_select_entity = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(operator_items),
                        selected_op_index.map(|i| IndexPath::new(i)),
                        window,
                        cx,
                    )
                });

                // 订阅操作符选择事件
                let row_id_clone_op = row_id.clone();
                cx.subscribe(
                    &operator_select_entity,
                    move |this, _, event: &SelectEvent<SearchableVec<FilterOperatorItem>>, _cx| {
                        let SelectEvent::Confirm(value) = event;
                        if let Some(op) = value {
                            this.update_condition_operator(&row_id_clone_op, *op);
                        }
                    },
                )
                .detach();

                // 创建值输入框
                let value_input_entity =
                    cx.new(|cx| InputState::new(window, cx).placeholder("输入值...".to_string()));

                // 观察值输入变化
                let row_id_clone_val = row_id.clone();
                let value_input_clone = value_input_entity.clone();
                let val_sub = cx.observe(&value_input_entity, move |this, _, cx| {
                    let text = value_input_clone.read(cx).text().to_string();
                    this.update_condition_value(&row_id_clone_val, text);
                    cx.notify();
                });

                parent.children.push(FilterItem::Condition(row));
                self.column_selects
                    .insert(row_id.clone(), column_select_entity);
                self.operator_selects
                    .insert(row_id.clone(), operator_select_entity);
                self.value_inputs.insert(row_id.clone(), value_input_entity);
                self.value_start_inputs
                    .insert(row_id.clone(), value_start_input_entity);
                self.value_end_inputs
                    .insert(row_id.clone(), value_end_input_entity);
                self.value_subscriptions.insert(row_id.clone(), val_sub);
                self.value_subscriptions
                    .insert(format!("{}_start", row_id), val_start_sub);
                self.value_subscriptions
                    .insert(format!("{}_end", row_id), val_end_sub);
                // 添加条件后展开分组（避免用户看不到刚添加的条件）
                self.collapsed_groups.remove(parent_group_id);
                self.sync_filter_state();
                cx.notify();
            }
        }
    }

    fn toggle_group_collapse(&mut self, group_id: &str, cx: &mut Context<Self>) {
        if self.collapsed_groups.contains(group_id) {
            self.collapsed_groups.remove(group_id);
        } else {
            self.collapsed_groups.insert(group_id.to_string());
        }
        cx.notify();
    }

    fn delete_condition(&mut self, id: &str, cx: &mut Context<Self>) {
        delete_condition_from_items(&mut self.root_items, id);
        self.column_selects.remove(id);
        self.operator_selects.remove(id);
        self.value_inputs.remove(id);
        self.value_start_inputs.remove(id);
        self.value_end_inputs.remove(id);
        self.sync_filter_state();
        cx.notify();
    }

    fn delete_group(&mut self, id: &str, cx: &mut Context<Self>) {
        // 先收集所有需要清理的 ID
        let ids_to_remove = self.collect_ids_for_cleanup(id);
        for row_id in &ids_to_remove {
            self.column_selects.remove(row_id);
            self.operator_selects.remove(row_id);
            self.value_inputs.remove(row_id);
            self.value_start_inputs.remove(row_id);
            self.value_end_inputs.remove(row_id);
        }
        delete_group_from_items(&mut self.root_items, id);
        self.collapsed_groups.remove(id);
        self.sync_filter_state();
        cx.notify();
    }

    /// 递归收集分组及其嵌套分组内的所有条件行 ID
    fn collect_ids_for_cleanup(&self, group_id: &str) -> Vec<String> {
        let mut ids = Vec::new();
        if let Some(group) = self.find_group_by_id(&self.root_items, group_id) {
            self.collect_condition_ids_recursive(group, &mut ids);
        }
        ids
    }

    fn find_group_by_id<'a>(&'a self, items: &'a [FilterItem], id: &str) -> Option<&'a GroupRow> {
        for item in items {
            match item {
                FilterItem::Group(group) if group.id == id => return Some(group),
                FilterItem::Group(group) => {
                    if let Some(found) = self.find_group_by_id(&group.children, id) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn collect_condition_ids_recursive(&self, group: &GroupRow, out: &mut Vec<String>) {
        for item in &group.children {
            match item {
                FilterItem::Condition(row) => out.push(row.id.clone()),
                FilterItem::Group(nested) => self.collect_condition_ids_recursive(nested, out),
            }
        }
    }

    fn toggle_group(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(idx) = find_group_mut(&mut self.root_items, id) {
            if let FilterItem::Group(group) = &mut self.root_items[idx] {
                group.enabled = !group.enabled;
            }
        }
        self.sync_filter_state();
        cx.notify();
    }

    fn toggle_group_logic(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(idx) = find_group_mut(&mut self.root_items, id) {
            if let FilterItem::Group(group) = &mut self.root_items[idx] {
                group.logic_operator = match group.logic_operator {
                    LogicOperator::And => LogicOperator::Or,
                    LogicOperator::Or => LogicOperator::And,
                };
            }
        }
        self.sync_filter_state();
        cx.notify();
    }

    fn toggle_condition(&mut self, id: &str, cx: &mut Context<Self>) {
        toggle_condition_in_items(&mut self.root_items, id);
        self.sync_filter_state();
        cx.notify();
    }

    fn update_condition_value(&mut self, id: &str, value: String) {
        if update_condition_value_in_items(&mut self.root_items, id, &value) {
            self.sync_filter_state();
        }
    }

    fn update_condition_value_start(&mut self, id: &str, value: String) {
        if update_condition_value_start_in_items(&mut self.root_items, id, &value) {
            self.sync_filter_state();
        }
    }

    fn update_condition_value_end(&mut self, id: &str, value: String) {
        if update_condition_value_end_in_items(&mut self.root_items, id, &value) {
            self.sync_filter_state();
        }
    }

    fn toggle_condition_logic(&mut self, id: &str, cx: &mut Context<Self>) {
        toggle_condition_logic_in_items(&mut self.root_items, id);
        cx.notify();
    }

    fn handle_apply_click(&self, cx: &mut Context<Self>) {
        cx.emit(FilterEditorEvent::QueryApply);
    }

    /// 收集所有需要渲染的筛选项（嵌套结构）
    fn collect_render_items(&self) -> Vec<RenderItem> {
        let mut items = Vec::new();
        for (idx, item) in self.root_items.iter().enumerate() {
            items.push(self.collect_items_recursive(item, idx, idx == 0));
        }
        items
    }

    fn collect_items_recursive(&self, item: &FilterItem, idx: usize, is_first: bool) -> RenderItem {
        match item {
            FilterItem::Condition(row) => RenderItem::Condition(RenderConditionRow {
                row: row.clone(),
                idx,
            }),
            FilterItem::Group(group) => {
                let mut children = Vec::new();
                for (child_idx, child) in group.children.iter().enumerate() {
                    children.push(self.collect_items_recursive(child, child_idx, child_idx == 0));
                }
                RenderItem::Group {
                    group: group.clone(),
                    children,
                    is_collapsed: self.collapsed_groups.contains(&group.id),
                    is_first,
                }
            }
        }
    }

    /// 渲染条件行
    fn render_condition_row(
        &self,
        cr: &RenderConditionRow,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme();
        let row = &cr.row;
        let row_id = row.id.clone();
        let row_id_for_click = row.id.clone();
        let row_id_for_logic = row.id.clone();
        let row_id_for_delete = row.id.clone();
        let is_enabled = row.enabled;
        let column_select = self.column_selects.get(&row.id);
        let operator_select = self.operator_selects.get(&row.id);
        let value_input = self.value_inputs.get(&row.id);
        let value_start_input = self.value_start_inputs.get(&row.id);
        let value_end_input = self.value_end_inputs.get(&row.id);
        let logic_is_and = matches!(row.logic_operator, LogicOperator::And);

        // 逻辑操作符选择器（根级别的第一条条件不显示）
        let logic_toggle = if cr.idx > 0 {
            gpui::div()
                .text_xs()
                .px_2()
                .py_px()
                .rounded_full()
                .bg(if logic_is_and {
                    theme.success
                } else {
                    theme.warning
                })
                .text_color(theme.primary_foreground)
                .font_weight(gpui::FontWeight::MEDIUM)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.toggle_condition_logic(&row_id_for_logic, cx);
                    }),
                )
                .child(if logic_is_and { "AND" } else { "OR" })
        } else {
            gpui::div().w(px(48.))
        };

        // 值输入区域
        let value_area: gpui::Div = if row.operator.needs_two_values() {
            gpui::div()
                .flex()
                .items_center()
                .gap_1()
                .flex_1()
                .child(
                    gpui::div()
                        .flex_1()
                        .h_7()
                        .when_some(value_start_input, |el, input| {
                            el.child(Input::new(input).small())
                        }),
                )
                .child(
                    gpui::div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .px_1()
                        .child("AND"),
                )
                .child(
                    gpui::div()
                        .flex_1()
                        .h_7()
                        .when_some(value_end_input, |el, input| {
                            el.child(Input::new(input).small())
                        }),
                )
        } else if row.operator.needs_value() {
            gpui::div()
                .flex_1()
                .h_7()
                .when_some(value_input, |el, input| el.child(Input::new(input).small()))
        } else {
            gpui::div().flex_1()
        };

        gpui::div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(if is_enabled {
                theme.secondary
            } else {
                theme.muted
            })
            .opacity(if is_enabled { 1.0 } else { 0.6 })
            .child(logic_toggle)
            .child(
                Checkbox::new(SharedString::from(format!("cond-check-{}", row_id.clone())))
                    .checked(is_enabled)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_condition(&row_id_for_click, cx);
                    })),
            )
            .child(
                gpui::div()
                    .w(px(140.))
                    .h_7()
                    .when_some(column_select, |el, select| {
                        el.child(Select::new(select).small())
                    }),
            )
            .child(
                gpui::div()
                    .w(px(120.))
                    .h_7()
                    .when_some(operator_select, |el, select| {
                        el.child(Select::new(select).small())
                    }),
            )
            .child(value_area)
            .child(
                Button::new(format!("del-{}", row_id.clone()))
                    .small()
                    .ghost()
                    .icon(IconName::Trash)
                    .tooltip("删除条件")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.delete_condition(&row_id_for_delete, cx);
                    })),
            )
    }

    /// 渲染分组（包含标题和子项）
    fn render_group(
        &self,
        group: &GroupRow,
        children: Vec<RenderItem>,
        is_collapsed: bool,
        is_first: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // 提取所有需要的值，避免 borrow checker 问题
        let group_id = group.id.clone();
        let group_id_for_logic = group.id.clone();
        let logic_is_and = matches!(group.logic_operator, LogicOperator::And);
        let group_id_for_collapse = group.id.clone();
        let group_id_for_toggle = group.id.clone();
        let group_id_for_delete = group.id.clone();
        let group_id_for_add_cond = group.id.clone();
        let group_id_for_add_group = group.id.clone();
        let is_enabled = group.enabled;

        // 提取主题颜色
        let theme = cx.theme();
        let border_color = theme.border;
        let background_color = if is_enabled {
            theme.secondary.opacity(0.3)
        } else {
            theme.muted
        };
        let primary_bg = theme.primary;
        let primary_fg = theme.primary_foreground;
        let success_color = theme.success;
        let warning_color = theme.warning;

        // 预渲染子项
        let child_elements: Vec<gpui::AnyElement> = if is_collapsed {
            Vec::new()
        } else {
            children
                .into_iter()
                .map(|item| self.render_item(item, cx))
                .collect()
        };

        gpui::div()
            .flex()
            .flex_col()
            .gap_1()
            .ml_6()
            .border_1()
            .border_color(border_color)
            .rounded_md()
            .p_2()
            .bg(background_color)
            .opacity(if is_enabled { 1.0 } else { 0.6 })
            // 分组标题栏
            .child(
                gpui::div()
                    .flex()
                    .items_center()
                    .gap_2()
                    // 非首项时显示 AND/OR 徽章
                    .when(!is_first, |el| {
                        el.child(
                            gpui::div()
                                .text_xs()
                                .px_2()
                                .py_px()
                                .rounded_full()
                                .bg(if logic_is_and {
                                    success_color
                                } else {
                                    warning_color
                                })
                                .text_color(primary_fg)
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        this.toggle_group_logic(&group_id_for_logic, cx);
                                    }),
                                )
                                .child(if logic_is_and { "AND" } else { "OR" }),
                        )
                    })
                    .child(
                        gpui::div()
                            .text_xs()
                            .px_2()
                            .py_px()
                            .rounded_full()
                            .bg(primary_bg)
                            .text_color(primary_fg)
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child("分组"),
                    )
                    .child(
                        Button::new(format!("toggle-group-{}", group_id.clone()))
                            .small()
                            .ghost()
                            .icon(if is_collapsed {
                                IconName::ChevronRight
                            } else {
                                IconName::ChevronDown
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.toggle_group_collapse(&group_id_for_collapse, cx);
                            })),
                    )
                    .child(
                        Checkbox::new(SharedString::from(format!(
                            "group-check-{}",
                            group_id.clone()
                        )))
                        .checked(is_enabled)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.toggle_group(&group_id_for_toggle, cx);
                        })),
                    )
                    .child(
                        Button::new(format!("add-cond-to-group-{}", group_id.clone()))
                            .small()
                            .ghost()
                            .icon(IconName::Plus)
                            .tooltip("添加条件到分组")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_condition_to_group(&group_id_for_add_cond, window, cx);
                            })),
                    )
                    .child(
                        Button::new(format!("add-group-to-group-{}", group_id.clone()))
                            .small()
                            .ghost()
                            .icon(IconName::Folder)
                            .tooltip("添加子分组")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.add_group_to_group(&group_id_for_add_group, window, cx);
                            })),
                    )
                    .child(
                        Button::new(format!("del-group-{}", group_id.clone()))
                            .small()
                            .ghost()
                            .icon(IconName::Trash)
                            .tooltip("删除分组")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.delete_group(&group_id_for_delete, cx);
                            })),
                    ),
            )
            // 子项容器
            .children(child_elements)
    }

    /// 渲染单个 RenderItem（递归）
    fn render_item(&self, item: RenderItem, cx: &mut Context<Self>) -> gpui::AnyElement {
        match item {
            RenderItem::Condition(cr) => self.render_condition_row(&cr, cx).into_any_element(),
            RenderItem::Group {
                group,
                children,
                is_collapsed,
                is_first,
            } => self
                .render_group(&group, children, is_collapsed, is_first, cx)
                .into_any_element(),
        }
    }
}

/// 渲染项（嵌套结构，组包含其子项）
enum RenderItem {
    Condition(RenderConditionRow),
    Group {
        group: GroupRow,
        children: Vec<RenderItem>,
        is_collapsed: bool,
        is_first: bool,
    },
}

struct RenderConditionRow {
    row: ConditionRow,
    idx: usize,
}

impl Render for VisualFilterBuilder {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_filters = !self.root_items.is_empty();

        // 先收集渲染项，避免在闭包中多次访问 cx
        let render_items = self.collect_render_items();

        // 预渲染所有项（避免在闭包中捕获 cx）
        let rendered_items: Vec<gpui::AnyElement> = render_items
            .into_iter()
            .map(|item| self.render_item(item, cx))
            .collect();

        // 提取主题颜色
        let theme = cx.theme();
        let border_color = theme.border;
        let background_color = theme.background;
        let primary_color = theme.primary;
        let muted_foreground_color = theme.muted_foreground;

        gpui::div()
            .flex()
            .flex_col()
            .w_full()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(border_color)
            .bg(background_color)
            // WHERE 分区
            .child(
                gpui::div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    // 标题栏
                    .child(
                        gpui::div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .w_full()
                            .child(
                                gpui::div().flex().items_center().gap_2().child(
                                    gpui::div()
                                        .text_sm()
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(primary_color)
                                        .child("WHERE"),
                                    // )
                                    // .child(
                                    //     gpui::div()
                                    //         .text_xs()
                                    //         .px_2()
                                    //         .py_px()
                                    //         .rounded_full()
                                    //         .bg(secondary_color)
                                    //         .text_color(primary_color)
                                    //         .font_weight(gpui::FontWeight::MEDIUM)
                                    //         .child(logic_label),
                                ),
                            )
                            .child(
                                gpui::div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Button::new("add-condition")
                                            .small()
                                            .icon(IconName::Plus)
                                            .ghost()
                                            .tooltip("添加条件")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.add_condition(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("add-group")
                                            .small()
                                            .icon(IconName::Folder)
                                            .ghost()
                                            .tooltip("添加分组")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.add_group(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("clear-btn")
                                            .small()
                                            .icon(IconName::Trash)
                                            .ghost()
                                            .tooltip("清除所有条件")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.clear_all(cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("apply-btn")
                                            .small()
                                            .icon(IconName::Check)
                                            .tooltip("应用筛选条件")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.handle_apply_click(cx);
                                            })),
                                    ),
                            ),
                    )
                    // 渲染嵌套的筛选项列表
                    .children(rendered_items)
                    .when(!has_filters, |el| {
                        el.child(
                            gpui::div()
                                .text_sm()
                                .text_color(muted_foreground_color)
                                .px_3()
                                .py_2()
                                .child("点击 + 添加筛选条件"),
                        )
                    }),
            )
    }
}

impl EventEmitter<FilterEditorEvent> for VisualFilterBuilder {}

// ========== 向后兼容的 TableFilterEditor ==========

pub struct TableFilterEditor {
    inner: Entity<VisualFilterBuilder>,
}

impl TableFilterEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let inner = cx.new(|cx| VisualFilterBuilder::new(window, cx));
        let this = Self { inner };
        // 转发 inner 的 FilterEditorEvent
        cx.subscribe(&this.inner, |_this, _, evt: &FilterEditorEvent, cx| {
            cx.emit(evt.clone());
        })
        .detach();
        this
    }

    pub fn get_where_clause(&self, cx: &App) -> String {
        self.inner.read(cx).get_where_clause()
    }

    pub fn get_order_by_clause(&self, cx: &App) -> String {
        self.inner.read(cx).get_order_by_clause()
    }

    pub fn set_schema(&mut self, schema: TableSchema, cx: &mut Context<Self>) {
        self.inner
            .update(cx, |inner, cx| inner.set_schema(schema, cx));
    }

    pub fn add_sort_column(&mut self, column: &str, direction: ColumnSort, cx: &mut Context<Self>) {
        self.inner
            .update(cx, |inner, cx| inner.add_sort_column(column, direction, cx));
    }
}

impl Render for TableFilterEditor {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.inner.clone()
    }
}

impl EventEmitter<FilterEditorEvent> for TableFilterEditor {}

// ========== 测试（保留）==========
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_schema() -> TableSchema {
        TableSchema {
            columns: vec![
                ColumnInfo {
                    name: "name".into(),
                    data_type: "VARCHAR".into(),
                    is_nullable: true,
                    is_primary_key: false,
                    default_value: None,
                    comment: None,
                    charset: None,
                    collation: None,
                },
                ColumnInfo {
                    name: "age".into(),
                    data_type: "INT".into(),
                    is_nullable: false,
                    is_primary_key: false,
                    default_value: None,
                    comment: None,
                    charset: None,
                    collation: None,
                },
                ColumnInfo {
                    name: "created_at".into(),
                    data_type: "TIMESTAMP".into(),
                    is_nullable: false,
                    is_primary_key: false,
                    default_value: None,
                    comment: None,
                    charset: None,
                    collation: None,
                },
            ],
        }
    }

    fn labels_for(text: &str, current_word: &str) -> Vec<String> {
        let zero = lsp_types::Position::new(0, 0);
        suggest_items(&sample_schema(), current_word, Range::new(zero, zero), text)
            .into_iter()
            .map(|item| item.label)
            .collect()
    }

    #[test]
    fn suggests_columns_at_condition_start() {
        let labels = labels_for("AND ", "");
        assert!(labels.starts_with(&["name".into(), "age".into(), "created_at".into()]));
    }

    #[test]
    fn suggests_operators_after_column() {
        let labels = labels_for("name ", "");
        assert!(labels.contains(&"= ''".into()));
        assert!(labels.contains(&"LIKE '%%'".into()));
    }

    #[test]
    fn suggests_string_values_after_like() {
        let labels = labels_for("name LIKE ", "");
        assert_eq!(labels.first().map(String::as_str), Some("'%...%'"));
    }

    #[test]
    fn suggests_logic_after_complete_condition() {
        let labels = labels_for("name = 'Alice' ", "");
        assert_eq!(labels.first().map(String::as_str), Some("AND"));
        assert_eq!(labels.get(1).map(String::as_str), Some("OR"));
    }

    #[test]
    fn suggests_is_null_variants() {
        let labels = labels_for("name IS ", "");
        assert_eq!(labels, vec!["NULL".to_string(), "NOT NULL".to_string()]);
    }

    #[test]
    fn suggests_not_operators_for_string_columns() {
        let labels = labels_for("name NOT ", "");
        assert_eq!(labels, vec!["LIKE".to_string(), "IN".to_string()]);
    }

    #[test]
    fn suggests_between_end_value_after_and() {
        let labels = labels_for("age BETWEEN 1 AND ", "");
        assert_eq!(labels.first().map(String::as_str), Some("0"));
    }
}

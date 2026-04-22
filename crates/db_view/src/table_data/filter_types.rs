//! 数据筛选构建器类型定义与 SQL 生成器

use db::ColumnInfo;
use one_ui::edit_table::ColumnSort;
use tracing;

/// 操作符分组类别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorCategory {
    /// 比较运算：等于、不等于
    Comparison,
    /// 数值范围：大于、小于、BETWEEN
    Range,
    /// 模式匹配：LIKE、NOT LIKE
    Pattern,
    /// 列表运算：IN、NOT IN
    List,
    /// 空值判断：IS NULL、IS NOT NULL
    Null,
}

impl OperatorCategory {
    pub fn i18n_key(&self) -> &'static str {
        match self {
            Self::Comparison => "Filter.category_comparison",
            Self::Range => "Filter.category_range",
            Self::Pattern => "Filter.category_pattern",
            Self::List => "Filter.category_list",
            Self::Null => "Filter.category_null",
        }
    }
}

/// 筛选操作符
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterOperator {
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterOrEqual,
    LessOrEqual,
    Like,
    NotLike,
    In,
    NotIn,
    IsNull,
    IsNotNull,
    Between,
}

impl FilterOperator {
    /// 转换为 SQL 操作符字符串
    pub fn to_sql(&self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::NotEqual => "!=",
            Self::GreaterThan => ">",
            Self::LessThan => "<",
            Self::GreaterOrEqual => ">=",
            Self::LessOrEqual => "<=",
            Self::Like => "LIKE",
            Self::NotLike => "NOT LIKE",
            Self::In => "IN",
            Self::NotIn => "NOT IN",
            Self::IsNull => "IS NULL",
            Self::IsNotNull => "IS NOT NULL",
            Self::Between => "BETWEEN",
        }
    }

    /// 操作符是否需要值输入
    pub fn needs_value(&self) -> bool {
        !matches!(self, Self::IsNull | Self::IsNotNull)
    }

    /// 操作符是否需要两个值（BETWEEN）
    pub fn needs_two_values(&self) -> bool {
        matches!(self, Self::Between)
    }

    /// 操作符是否需要列表值（IN / NOT IN）
    pub fn needs_list_value(&self) -> bool {
        matches!(self, Self::In | Self::NotIn)
    }

    /// 获取操作符显示标签
    pub fn label(&self) -> &'static str {
        match self {
            Self::Equal => "=",
            Self::NotEqual => "!=",
            Self::GreaterThan => ">",
            Self::LessThan => "<",
            Self::GreaterOrEqual => ">=",
            Self::LessOrEqual => "<=",
            Self::Like => "LIKE",
            Self::NotLike => "NOT LIKE",
            Self::In => "IN",
            Self::NotIn => "NOT IN",
            Self::IsNull => "IS NULL",
            Self::IsNotNull => "IS NOT NULL",
            Self::Between => "BETWEEN",
        }
    }

    /// 获取操作符分组类别
    pub fn category(&self) -> OperatorCategory {
        match self {
            Self::Equal | Self::NotEqual => OperatorCategory::Comparison,
            Self::GreaterThan | Self::LessThan | Self::GreaterOrEqual | Self::LessOrEqual | Self::Between => {
                OperatorCategory::Range
            }
            Self::Like | Self::NotLike => OperatorCategory::Pattern,
            Self::In | Self::NotIn => OperatorCategory::List,
            Self::IsNull | Self::IsNotNull => OperatorCategory::Null,
        }
    }

    /// 获取操作符说明的翻译键名
    pub fn description_key(&self) -> &'static str {
        match self {
            Self::Equal => "Filter.operator_equal",
            Self::NotEqual => "Filter.operator_not_equal",
            Self::GreaterThan => "Filter.operator_greater_than",
            Self::LessThan => "Filter.operator_less_than",
            Self::GreaterOrEqual => "Filter.operator_greater_or_equal",
            Self::LessOrEqual => "Filter.operator_less_or_equal",
            Self::Like => "Filter.operator_like",
            Self::NotLike => "Filter.operator_not_like",
            Self::In => "Filter.operator_in",
            Self::NotIn => "Filter.operator_not_in",
            Self::IsNull => "Filter.operator_is_null",
            Self::IsNotNull => "Filter.operator_is_not_null",
            Self::Between => "Filter.operator_between",
        }
    }
}

/// 筛选条件的值
#[derive(Debug, Clone)]
pub enum FilterValue {
    /// 单个值（用于 =, !=, >, < 等）
    Single(String),
    /// 列表值（用于 IN, NOT IN），逗号分隔
    List(String),
    /// 范围值（用于 BETWEEN），两个值用 AND 分隔
    Range { start: String, end: String },
}

impl Default for FilterValue {
    fn default() -> Self {
        Self::Single(String::new())
    }
}

impl FilterValue {
    /// 转换为 SQL 字面量
    /// 对于 IN 和 BETWEEN，调用方需要负责加括号等格式
    pub fn to_sql(&self, _operator: FilterOperator) -> String {
        match self {
            FilterValue::Single(v) => {
                if v.is_empty() {
                    return "'".to_string();
                }
                // 检测是否已经是 SQL 表达式（函数调用、数字、NULL 等）
                let v = v.trim();
                if v.eq_ignore_ascii_case("NULL")
                    || v.starts_with(|c: char| c.is_ascii_digit())
                    || v == "TRUE"
                    || v == "FALSE"
                    || v.starts_with('(')
                    || v.contains('(')
                {
                    v.to_string()
                } else {
                    format!("'{}'", escape_sql_string(v))
                }
            }
            FilterValue::List(items) => {
                if items.is_empty() {
                    return "()".to_string();
                }
                let items: Vec<String> = items
                    .split(',')
                    .map(|s| {
                        let s = s.trim();
                        if s.eq_ignore_ascii_case("NULL") {
                            s.to_uppercase()
                        } else if s.is_empty() {
                            "''".to_string()
                        } else if s.parse::<f64>().is_ok() {
                            s.to_string()
                        } else {
                            format!("'{}'", escape_sql_string(s))
                        }
                    })
                    .collect();
                format!("({})", items.join(", "))
            }
            FilterValue::Range { start, end } => {
                let start_sql = if start.trim().eq_ignore_ascii_case("NULL") {
                    "NULL".to_string()
                } else if start.trim().parse::<f64>().is_ok() {
                    start.trim().to_string()
                } else {
                    format!("'{}'", escape_sql_string(start.trim()))
                };
                let end_sql = if end.trim().eq_ignore_ascii_case("NULL") {
                    "NULL".to_string()
                } else if end.trim().parse::<f64>().is_ok() {
                    end.trim().to_string()
                } else {
                    format!("'{}'", escape_sql_string(end.trim()))
                };
                format!("{} AND {}", start_sql, end_sql)
            }
        }
    }
}

/// 转义 SQL 字符串中的单引号
fn escape_sql_string(s: &str) -> String {
    s.replace('\'', "''")
}

/// 排序方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    pub fn to_sql(&self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }
}

/// 排序条件
#[derive(Debug, Clone)]
pub struct SortCondition {
    pub column: String,
    pub direction: SortDirection,
}

impl SortCondition {
    pub fn new(column: String, direction: SortDirection) -> Self {
        Self { column, direction }
    }

    pub fn to_sql(&self) -> String {
        format!("{} {}", self.column, self.direction.to_sql())
    }
}

/// 筛选条件项（叶子节点）
#[derive(Debug, Clone)]
pub struct ConditionItem {
    pub column: String,
    pub operator: FilterOperator,
    pub value: FilterValue,
    pub enabled: bool,
    /// 连接到此条件的逻辑操作符（仅用于 SQL 生成，视觉显示由 UI 层处理）
    pub logic_operator: LogicOperator,
}

#[cfg(test)]
impl ConditionItem {
    /// 转换为 SQL WHERE 片段（无前导逻辑操作符）
    pub fn to_sql(&self) -> Option<String> {
        condition_sql_fragment(self)
    }
}

fn condition_sql_fragment(condition: &ConditionItem) -> Option<String> {
    if !condition.enabled {
        return None;
    }

    let col = &condition.column;
    let op = condition.operator;

    match op {
        FilterOperator::IsNull => Some(format!("{} IS NULL", col)),
        FilterOperator::IsNotNull => Some(format!("{} IS NOT NULL", col)),
        _ => {
            let value_sql = condition.value.to_sql(op);
            Some(format!("{} {} {}", col, op.to_sql(), value_sql))
        }
    }
}

/// 逻辑操作符（用于分组内各条件之间）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicOperator {
    And,
    Or,
}

impl LogicOperator {
    pub fn to_sql(&self) -> &'static str {
        match self {
            Self::And => "AND",
            Self::Or => "OR",
        }
    }
}

/// 子节点（可以是条件或嵌套分组）
#[derive(Debug, Clone)]
pub enum FilterChild {
    Condition(ConditionItem),
    Group(Box<FilterGroup>),
}

/// 筛选分组（包含多个子条件或嵌套分组）
#[derive(Debug, Clone)]
pub struct FilterGroup {
    pub enabled: bool,
    pub logic_operator: LogicOperator,
    pub children: Vec<FilterChild>,
}

impl FilterGroup {
    pub fn new(logic_operator: LogicOperator) -> Self {
        Self {
            enabled: true,
            logic_operator,
            children: Vec::new(),
        }
    }

    /// 添加条件
    pub fn add_condition(&mut self, condition: ConditionItem) {
        self.children.push(FilterChild::Condition(condition));
    }

    /// 添加嵌套分组
    pub fn add_group(&mut self, group: FilterGroup) {
        self.children.push(FilterChild::Group(Box::new(group)));
    }
    /// 转换为 SQL WHERE 片段
    /// 每个条件的 logic_operator 表示它如何连接到前一个条件
    pub fn to_sql(&self) -> Option<String> {
        // 如果分组被禁用，整个分组不生成 SQL
        if !self.enabled || self.children.is_empty() {
            return None;
        }

        let mut result = String::new();
        let mut is_first = true;

        for child in &self.children {
            match child {
                FilterChild::Condition(c) => {
                    let Some(cond_sql) = condition_sql_fragment(c) else {
                        continue;
                    };
                    if is_first {
                        result = cond_sql;
                        is_first = false;
                    } else {
                        // 使用当前条件的 logic_operator 连接到前一个
                        result = format!("{} {} {}", result, c.logic_operator.to_sql(), cond_sql);
                    }
                }
                FilterChild::Group(g) => {
                    if let Some(s) = g.to_sql() {
                        if is_first {
                            result = s;
                            is_first = false;
                        } else {
                            // 嵌套分组使用自己的 logic_operator
                            result = format!("{} {} {}", result, g.logic_operator.to_sql(), s);
                        }
                    }
                }
            }
        }

        if result.is_empty() {
            return None;
        }

        // 如果只有1个条件，不需要括号
        if self
            .children
            .iter()
            .filter(|c| match c {
                FilterChild::Condition(c) => c.enabled,
                FilterChild::Group(g) => g.to_sql().is_some(),
            })
            .count()
            == 1
        {
            return Some(result);
        }

        Some(format!("({})", result))
    }
}

/// 筛选状态（根分组 + 排序条件）
#[derive(Debug, Clone)]
pub struct FilterState {
    pub root: FilterGroup,
    pub sorts: Vec<SortCondition>,
}

impl Default for FilterState {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterState {
    pub fn new() -> Self {
        Self {
            root: FilterGroup::new(LogicOperator::And),
            sorts: Vec::new(),
        }
    }

    /// 转换为完整 WHERE 子句（不含 WHERE 关键字）
    pub fn to_where_clause(&self) -> String {
        let sql = self.root.to_sql().unwrap_or_default();
        tracing::debug!(
            "[FilterState] to_where_clause: {} conditions, WHERE=\"{}\"",
            self.root.children.len(),
            sql
        );
        sql
    }

    /// 转换为 ORDER BY 子句（不含 ORDER BY 关键字）
    pub fn to_order_by_clause(&self) -> String {
        if self.sorts.is_empty() {
            tracing::debug!("[FilterState] to_order_by_clause: (no sorts)");
            return String::new();
        }
        let sql = self
            .sorts
            .iter()
            .map(|s| s.to_sql())
            .collect::<Vec<_>>()
            .join(", ");
        tracing::info!(
            "[FilterState] to_order_by_clause: {} sorts, ORDER BY=\"{}\"",
            self.sorts.len(),
            sql
        );
        sql
    }

    /// 应用表头点击排序：表头只支持单列三态排序。
    pub fn apply_header_sort(&mut self, column: &str, direction: ColumnSort) {
        self.sorts.clear();

        let sort_dir = match direction {
            ColumnSort::Ascending => SortDirection::Asc,
            ColumnSort::Descending => SortDirection::Desc,
            ColumnSort::Default => return,
        };

        self.sorts
            .push(SortCondition::new(column.to_string(), sort_dir));
    }
}

/// 生成简单 UUID（用于 React/DOM 风格的 key）
pub fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", nanos)
}

/// 检测是否为字符串类型
pub fn is_string_type(data_type: &str) -> bool {
    let dt = data_type.to_uppercase();
    dt.contains("CHAR")
        || dt.contains("TEXT")
        || dt.contains("VARCHAR")
        || dt.contains("LONGTEXT")
        || dt.contains("MEDIUMTEXT")
        || dt.contains("TINYTEXT")
        || dt.contains("STRING")
        || dt.contains("JSON")
}

/// 检测是否为数值类型
pub fn is_numeric_type(data_type: &str) -> bool {
    let dt = data_type.to_uppercase();
    dt.contains("INT")
        || dt.contains("BIGINT")
        || dt.contains("SMALLINT")
        || dt.contains("TINYINT")
        || dt.contains("DECIMAL")
        || dt.contains("FLOAT")
        || dt.contains("DOUBLE")
        || dt.contains("NUMERIC")
        || dt.contains("NUMBER")
        || dt.contains("REAL")
        || dt.contains("SERIAL")
}

/// 检测是否为日期时间类型
pub fn is_datetime_type(data_type: &str) -> bool {
    let dt = data_type.to_uppercase();
    dt.contains("DATE")
        || dt.contains("TIME")
        || dt.contains("YEAR")
        || dt.contains("TIMESTAMP")
        || dt.contains("DATETIME")
}

/// 根据列类型返回适合的操作符
pub fn operators_for_column(column: &ColumnInfo) -> Vec<FilterOperator> {
    let data_type = column.data_type.as_str();
    if is_string_type(data_type) {
        vec![
            FilterOperator::Equal,
            FilterOperator::NotEqual,
            FilterOperator::Like,
            FilterOperator::NotLike,
            FilterOperator::In,
            FilterOperator::NotIn,
            FilterOperator::IsNull,
            FilterOperator::IsNotNull,
        ]
    } else if is_numeric_type(data_type) {
        vec![
            FilterOperator::Equal,
            FilterOperator::NotEqual,
            FilterOperator::GreaterThan,
            FilterOperator::LessThan,
            FilterOperator::GreaterOrEqual,
            FilterOperator::LessOrEqual,
            FilterOperator::Between,
            FilterOperator::In,
            FilterOperator::NotIn,
            FilterOperator::IsNull,
            FilterOperator::IsNotNull,
        ]
    } else if is_datetime_type(data_type) {
        vec![
            FilterOperator::Equal,
            FilterOperator::NotEqual,
            FilterOperator::GreaterThan,
            FilterOperator::LessThan,
            FilterOperator::GreaterOrEqual,
            FilterOperator::LessOrEqual,
            FilterOperator::Between,
            FilterOperator::IsNull,
            FilterOperator::IsNotNull,
        ]
    } else {
        vec![
            FilterOperator::Equal,
            FilterOperator::NotEqual,
            FilterOperator::IsNull,
            FilterOperator::IsNotNull,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use one_ui::edit_table::ColumnSort;

    #[test]
    fn test_single_condition_sql() {
        let cond = ConditionItem {
            column: "name".to_string(),
            operator: FilterOperator::Equal,
            value: FilterValue::Single("Alice".to_string()),
            enabled: true,
            logic_operator: LogicOperator::And,
        };
        assert_eq!(cond.to_sql(), Some("name = 'Alice'".to_string()));
    }

    #[test]
    fn test_is_null_condition() {
        let cond = ConditionItem {
            column: "name".to_string(),
            operator: FilterOperator::IsNull,
            value: FilterValue::default(),
            enabled: true,
            logic_operator: LogicOperator::And,
        };
        assert_eq!(cond.to_sql(), Some("name IS NULL".to_string()));
    }

    #[test]
    fn test_disabled_condition() {
        let cond = ConditionItem {
            column: "name".to_string(),
            operator: FilterOperator::Equal,
            value: FilterValue::Single("Bob".to_string()),
            enabled: false,
            logic_operator: LogicOperator::And,
        };
        assert_eq!(cond.to_sql(), None);
    }

    #[test]
    fn test_filter_group_and() {
        let mut group = FilterGroup::new(LogicOperator::And);
        group.enabled = true;
        group.add_condition(ConditionItem {
            column: "age".to_string(),
            operator: FilterOperator::GreaterThan,
            value: FilterValue::Single("18".to_string()),
            enabled: true,
            logic_operator: LogicOperator::And,
        });
        group.add_condition(ConditionItem {
            column: "name".to_string(),
            operator: FilterOperator::Like,
            value: FilterValue::Single("A%".to_string()),
            enabled: true,
            logic_operator: LogicOperator::And,
        });
        assert_eq!(
            group.to_sql(),
            Some("(age > 18 AND name LIKE 'A%')".to_string())
        );
    }

    #[test]
    fn test_filter_group_or() {
        let mut group = FilterGroup::new(LogicOperator::Or);
        group.enabled = true;
        group.add_condition(ConditionItem {
            column: "status".to_string(),
            operator: FilterOperator::Equal,
            value: FilterValue::Single("active".to_string()),
            enabled: true,
            logic_operator: LogicOperator::Or,
        });
        group.add_condition(ConditionItem {
            column: "status".to_string(),
            operator: FilterOperator::Equal,
            value: FilterValue::Single("pending".to_string()),
            enabled: true,
            logic_operator: LogicOperator::Or,
        });
        assert_eq!(
            group.to_sql(),
            Some("(status = 'active' OR status = 'pending')".to_string())
        );
    }

    #[test]
    fn test_filter_group_mixed_logic() {
        // 验证混合 logic_operator：每个条件使用自己的 logic_operator
        let mut group = FilterGroup::new(LogicOperator::And);
        group.enabled = true;
        group.add_condition(ConditionItem {
            column: "age".to_string(),
            operator: FilterOperator::GreaterThan,
            value: FilterValue::Single("18".to_string()),
            enabled: true,
            logic_operator: LogicOperator::And, // 第1个条件的 logic_operator 不使用
        });
        group.add_condition(ConditionItem {
            column: "name".to_string(),
            operator: FilterOperator::Like,
            value: FilterValue::Single("A%".to_string()),
            enabled: true,
            logic_operator: LogicOperator::Or, // 第2个条件用 OR
        });
        group.add_condition(ConditionItem {
            column: "status".to_string(),
            operator: FilterOperator::Equal,
            value: FilterValue::Single("active".to_string()),
            enabled: true,
            logic_operator: LogicOperator::And, // 第3个条件用 AND
        });
        assert_eq!(
            group.to_sql(),
            Some("(age > 18 OR name LIKE 'A%' AND status = 'active')".to_string())
        );
    }

    #[test]
    fn test_nested_group() {
        let mut inner = FilterGroup::new(LogicOperator::Or);
        inner.enabled = true;
        inner.add_condition(ConditionItem {
            column: "name".to_string(),
            operator: FilterOperator::Equal,
            value: FilterValue::Single("Alice".to_string()),
            enabled: true,
            logic_operator: LogicOperator::Or,
        });
        inner.add_condition(ConditionItem {
            column: "name".to_string(),
            operator: FilterOperator::Equal,
            value: FilterValue::Single("Bob".to_string()),
            enabled: true,
            logic_operator: LogicOperator::Or,
        });

        let mut outer = FilterGroup::new(LogicOperator::And);
        outer.enabled = true;
        outer.add_condition(ConditionItem {
            column: "age".to_string(),
            operator: FilterOperator::GreaterThan,
            value: FilterValue::Single("18".to_string()),
            enabled: true,
            logic_operator: LogicOperator::And,
        });
        outer.add_group(inner);

        let sql = outer.to_sql().unwrap();
        assert!(sql.contains("age > 18"));
        assert!(sql.contains("(name = 'Alice' OR name = 'Bob')"));
    }

    #[test]
    fn test_in_operator() {
        let cond = ConditionItem {
            column: "status".to_string(),
            operator: FilterOperator::In,
            value: FilterValue::List("active, pending".to_string()),
            enabled: true,
            logic_operator: LogicOperator::And,
        };
        assert_eq!(
            cond.to_sql(),
            Some("status IN ('active', 'pending')".to_string())
        );
    }

    #[test]
    fn test_between_operator() {
        let cond = ConditionItem {
            column: "age".to_string(),
            operator: FilterOperator::Between,
            value: FilterValue::Range {
                start: "18".to_string(),
                end: "30".to_string(),
            },
            enabled: true,
            logic_operator: LogicOperator::And,
        };
        assert_eq!(cond.to_sql(), Some("age BETWEEN 18 AND 30".to_string()));
    }

    #[test]
    fn test_sort_to_sql() {
        let sort = SortCondition::new("created_at".to_string(), SortDirection::Desc);
        assert_eq!(sort.to_sql(), "created_at DESC");
    }

    #[test]
    fn test_filter_state_full() {
        let mut state = FilterState::new();
        state.root.add_condition(ConditionItem {
            column: "name".to_string(),
            operator: FilterOperator::Like,
            value: FilterValue::Single("A%".to_string()),
            enabled: true,
            logic_operator: LogicOperator::And,
        });
        state
            .sorts
            .push(SortCondition::new("id".to_string(), SortDirection::Asc));

        assert_eq!(state.to_where_clause(), "name LIKE 'A%'");
        assert_eq!(state.to_order_by_clause(), "id ASC");
    }

    #[test]
    fn header_sort_replaces_existing_sorts_with_single_column() {
        let mut state = FilterState::new();
        state
            .sorts
            .push(SortCondition::new("name".to_string(), SortDirection::Asc));
        state.sorts.push(SortCondition::new(
            "created_at".to_string(),
            SortDirection::Desc,
        ));

        state.apply_header_sort("updated_at", ColumnSort::Descending);

        assert_eq!(state.to_order_by_clause(), "updated_at DESC");
    }

    #[test]
    fn header_sort_default_clears_all_sort_conditions() {
        let mut state = FilterState::new();
        state
            .sorts
            .push(SortCondition::new("name".to_string(), SortDirection::Asc));

        state.apply_header_sort("name", ColumnSort::Default);

        assert!(state.sorts.is_empty());
        assert_eq!(state.to_order_by_clause(), "");
    }

    #[test]
    fn test_sql_string_escape() {
        let cond = ConditionItem {
            column: "name".to_string(),
            operator: FilterOperator::Equal,
            value: FilterValue::Single("O'Brien".to_string()),
            enabled: true,
            logic_operator: LogicOperator::And,
        };
        assert_eq!(cond.to_sql(), Some("name = 'O''Brien'".to_string()));
    }
}

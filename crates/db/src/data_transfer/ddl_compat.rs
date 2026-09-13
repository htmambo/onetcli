//! 跨方言列定义双向转换机制。
//!
//! 采用「源方言解析 → 中间类型模型 → 目标方言渲染」的架构，覆盖
//! MySQL / PostgreSQL / SQLite 的全部 6 个转换方向；预检测
//! （`precheck.rs`）复用同一模型做兼容性判定，保证预检结论与实际建表一致。
//!
//! 约束：类型宽度/长度在中间模型中尽量保留；无法表达的目标侧语义
//! （如 PG 无符号整型、SQLite 无原生 JSON）由 `column_verdict`
//! 给出有损/不支持结论，供预检提示。

use one_core::storage::DatabaseType;
use rust_i18n::t;

use crate::types::ColumnInfo;

/// 列类型的中间表示
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ColumnKind {
    /// 整型，按存储字节数区分：1=tinyint 2=smallint 3=mediumint 4=int 8=bigint
    Int(u8),
    Boolean,
    Float,
    /// 定点数，携带 (精度, 小数位)
    Decimal(Option<(u32, u32)>),
    /// 变长字符串，可携带最大长度
    VarText(Option<u32>),
    /// 长文本（MySQL text 系 / PG text）
    Text,
    Blob,
    Json,
    Uuid,
    Date,
    Time,
    /// 无时区时间戳（MySQL datetime / PG timestamp）
    DateTime,
    /// 带时区时间戳（PG timestamptz）
    TimestampTz,
    /// 数组（PG 专有，其它方言无原生对应）
    Array(Box<ColumnKind>),
    /// 无法归类的类型，按原样透传（预检会给警告）
    Other(String),
}

/// 归一化后的列类型（含无符号标记）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ColumnSpec {
    pub kind: ColumnKind,
    pub unsigned: bool,
}

/// 把源方言的类型字符串解析为中间模型
pub(crate) fn normalize_type(data_type: &str, source: DatabaseType) -> ColumnSpec {
    let t = data_type.trim().to_ascii_lowercase();
    let unsigned = t.contains("unsigned");
    let t = t.replace(" unsigned", "").replace(" zerofill", "");
    let (t, array) = split_array(&t);
    let kind = match source {
        DatabaseType::PostgreSQL => normalize_pg(&t, array),
        DatabaseType::SQLite => normalize_sqlite(&t, array),
        _ => normalize_mysql(&t, array, unsigned),
    };
    ColumnSpec { kind, unsigned }
}

/// 识别 PG 数组后缀 `[]` / 前缀 `_`，返回去数组化的类型与数组元素
fn split_array(t: &str) -> (String, Option<ColumnKind>) {
    if let Some(head) = t.strip_suffix("[]") {
        return (head.to_string(), Some(inner_kind(head)));
    }
    if let Some(head) = t.strip_prefix('_') {
        return (head.to_string(), Some(inner_kind(head)));
    }
    (t.to_string(), None)
}

fn inner_kind(t: &str) -> ColumnKind {
    normalize_type(t, DatabaseType::PostgreSQL).kind
}

/// 从类型串中拆出基础名与括号参数
fn split_args(t: &str) -> (String, Vec<u32>) {
    let Some(open) = t.find('(') else {
        return (t.trim().to_string(), Vec::new());
    };
    let base = t[..open].trim().to_string();
    let args = t[open + 1..]
        .trim_end_matches(')')
        .split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .collect();
    (base, args)
}

fn normalize_mysql(t: &str, array: Option<ColumnKind>, unsigned: bool) -> ColumnKind {
    if let Some(elem) = array {
        return ColumnKind::Array(Box::new(elem));
    }
    let (base, args) = split_args(t);
    // 无符号值域提升到最小的可容纳有符号标准宽度（{1,2,3,4} -> {2,4,4,8}）：
    // tinyint u(0..255) -> smallint(2); smallint u(0..65535) -> int(4);
    // mediumint u(0..16777215) -> int(4); int u(0..2^32-1) -> bigint(8);
    // bigint u(0..2^64-1) 仍归 bigint(8)，由 verdict 提示「超 bigint 上限」。
    // 倍增只对 2 的幂成立；mediumint(3) 必须显式归位到 4，避免 Int(6) 非法宽度。
    let promoted_width = |n: u8| {
        if unsigned {
            match n {
                1 => 2,
                2 | 3 => 4,
                4 => 8,
                _ => 8, // 防御性兜底；MySQL 整型字节数域为 {1,2,3,4}
            }
        } else {
            n
        }
    };
    match base.as_str() {
        // tinyint(1) 一律归 Int(1)：MySQL 实际可存 -128..127，归 Boolean 会损坏 2..127 的状态码列
        "tinyint" => ColumnKind::Int(promoted_width(1)),
        "smallint" => ColumnKind::Int(promoted_width(2)),
        "mediumint" => ColumnKind::Int(promoted_width(3)),
        "int" | "integer" => ColumnKind::Int(promoted_width(4)),
        "bigint" => ColumnKind::Int(8),
        "year" => ColumnKind::Int(2),
        "boolean" | "bool" => ColumnKind::Boolean,
        "float" | "double" => ColumnKind::Float,
        "decimal" | "numeric" => ColumnKind::Decimal(decimal_args(&args)),
        "varchar" | "char" | "nchar" | "nvarchar" => ColumnKind::VarText(args.first().copied()),
        "tinytext" | "mediumtext" | "longtext" | "text" => ColumnKind::Text,
        "binary" | "varbinary" | "tinyblob" | "mediumblob" | "longblob" | "blob" => {
            ColumnKind::Blob
        }
        "json" => ColumnKind::Json,
        "enum" | "set" => ColumnKind::Text,
        "date" => ColumnKind::Date,
        "time" => ColumnKind::Time,
        "datetime" | "timestamp" => ColumnKind::DateTime,
        other => ColumnKind::Other(other.to_string()),
    }
}

fn normalize_pg(t: &str, array: Option<ColumnKind>) -> ColumnKind {
    if let Some(elem) = array {
        return ColumnKind::Array(Box::new(elem));
    }
    // 多词类型先归一
    let t = t
        .replace("character varying", "varchar")
        .replace("timestamp with time zone", "timestamptz")
        .replace("timestamp without time zone", "timestamp");
    let (base, args) = split_args(&t);
    match base.as_str() {
        "smallint" | "int2" => ColumnKind::Int(2),
        "int" | "integer" | "int4" => ColumnKind::Int(4),
        "bigint" | "int8" | "serial" | "bigserial" => ColumnKind::Int(8),
        "boolean" | "bool" => ColumnKind::Boolean,
        "real" | "float4" | "float8" => ColumnKind::Float,
        "numeric" | "decimal" | "money" => ColumnKind::Decimal(decimal_args(&args)),
        "varchar" | "char" | "character" => ColumnKind::VarText(args.first().copied()),
        "text" => ColumnKind::Text,
        "bytea" => ColumnKind::Blob,
        "json" | "jsonb" => ColumnKind::Json,
        "uuid" => ColumnKind::Uuid,
        "date" => ColumnKind::Date,
        "time" | "timetz" => ColumnKind::Time,
        "timestamp" => ColumnKind::DateTime,
        "timestamptz" => ColumnKind::TimestampTz,
        "inet" | "cidr" | "macaddr" | "interval" | "xml" => ColumnKind::Other(base),
        other => ColumnKind::Other(other.to_string()),
    }
}

fn normalize_sqlite(t: &str, array: Option<ColumnKind>) -> ColumnKind {
    if let Some(elem) = array {
        return ColumnKind::Array(Box::new(elem));
    }
    let (base, args) = split_args(t);
    // SQLite 列类型是亲和性建议，按关键字归类；无法识别的按 TEXT 处理
    if base.contains("int") {
        ColumnKind::Int(8)
    } else if base.contains("char") || base.contains("clob") || base.contains("text") {
        ColumnKind::VarText(args.first().copied())
    } else if base.contains("blob") || base.is_empty() {
        ColumnKind::Blob
    } else if base.contains("real") || base.contains("floa") || base.contains("doub") {
        ColumnKind::Float
    } else if base.contains("bool") {
        ColumnKind::Boolean
    } else if base.contains("decimal") || base.contains("numeric") {
        ColumnKind::Decimal(decimal_args(&args))
    } else if base.contains("date") || base.contains("time") {
        // SQLite 无原生日期类型，按习惯存 TEXT
        ColumnKind::Text
    } else {
        ColumnKind::Other(base)
    }
}

fn decimal_args(args: &[u32]) -> Option<(u32, u32)> {
    match args {
        [p, s] => Some((*p, *s)),
        [p] => Some((*p, 0)),
        _ => None,
    }
}

/// 把中间模型渲染为目标方言的类型串
pub(crate) fn render_type(spec: &ColumnSpec, target: DatabaseType) -> String {
    if target == DatabaseType::SQLite {
        return render_sqlite(spec);
    }
    let kind = &spec.kind;
    match target {
        DatabaseType::MySQL => render_mysql(kind),
        DatabaseType::PostgreSQL => render_pg(spec),
        _ => render_sqlite(spec),
    }
}

fn render_mysql(kind: &ColumnKind) -> String {
    match kind {
        ColumnKind::Int(n) => match n {
            1 => "tinyint".to_string(),
            2 => "smallint".to_string(),
            3 => "mediumint".to_string(),
            8 => "bigint".to_string(),
            _ => "int".to_string(),
        },
        ColumnKind::Boolean => "tinyint(1)".to_string(),
        ColumnKind::Float => "double".to_string(),
        // 精度未指定（如 PG numeric 无参数）时按 (65,30) 兜底，避免 MySQL 默认 (10,0) 静默截断
        ColumnKind::Decimal(None) => "decimal(65,30)".to_string(),
        ColumnKind::Decimal(p) => render_decimal("decimal", *p),
        ColumnKind::VarText(len) => render_var_text_mysql(*len),
        ColumnKind::Text => "text".to_string(),
        ColumnKind::Blob => "blob".to_string(),
        ColumnKind::Json | ColumnKind::Array(_) => "json".to_string(),
        ColumnKind::Uuid => "varchar(36)".to_string(),
        ColumnKind::Date => "date".to_string(),
        ColumnKind::Time => "time".to_string(),
        ColumnKind::DateTime | ColumnKind::TimestampTz => "datetime".to_string(),
        ColumnKind::Other(name) => name.clone(),
    }
}

fn render_pg(spec: &ColumnSpec) -> String {
    match &spec.kind {
        ColumnKind::Int(n) => match n {
            1 | 2 => "smallint".to_string(),
            3 | 4 => "integer".to_string(),
            _ => "bigint".to_string(),
        },
        ColumnKind::Boolean => "boolean".to_string(),
        ColumnKind::Float => "double precision".to_string(),
        ColumnKind::Decimal(None) => "numeric(65,30)".to_string(),
        ColumnKind::Decimal(p) => render_decimal("numeric", *p),
        ColumnKind::VarText(len) => match len {
            Some(n) => format!("varchar({n})"),
            None => "text".to_string(),
        },
        ColumnKind::Text => "text".to_string(),
        ColumnKind::Uuid => "uuid".to_string(),
        ColumnKind::Blob => "bytea".to_string(),
        ColumnKind::Json => "jsonb".to_string(),
        ColumnKind::Date => "date".to_string(),
        ColumnKind::Time => "time".to_string(),
        ColumnKind::DateTime => "timestamp".to_string(),
        ColumnKind::TimestampTz => "timestamptz".to_string(),
        ColumnKind::Array(elem) => {
            let elem_spec = ColumnSpec {
                kind: (**elem).clone(),
                unsigned: false,
            };
            format!("{}[]", render_pg(&elem_spec))
        }
        ColumnKind::Other(name) => name.clone(),
    }
}

fn render_sqlite(spec: &ColumnSpec) -> String {
    match &spec.kind {
        ColumnKind::Int(_) | ColumnKind::Boolean => "integer".to_string(),
        ColumnKind::Float => "real".to_string(),
        ColumnKind::Decimal(_) => "numeric".to_string(),
        // SQLite 无原生日期时间/JSON/UUID 类型，TEXT 亲和性最稳妥
        ColumnKind::VarText(_)
        | ColumnKind::Text
        | ColumnKind::Json
        | ColumnKind::Uuid
        | ColumnKind::Date
        | ColumnKind::Time
        | ColumnKind::DateTime
        | ColumnKind::TimestampTz
        | ColumnKind::Array(_) => "text".to_string(),
        ColumnKind::Blob => "blob".to_string(),
        ColumnKind::Other(name) => name.clone(),
    }
}

fn render_decimal(keyword: &str, args: Option<(u32, u32)>) -> String {
    match args {
        Some((p, s)) => format!("{keyword}({p},{s})"),
        None => keyword.to_string(),
    }
}

fn render_var_text_mysql(len: Option<u32>) -> String {
    match len {
        Some(n) if n <= 16383 => format!("varchar({n})"),
        // 超出 MySQL varchar 上限时降级为 text
        _ => "text".to_string(),
    }
}

/// 目标方言的默认值表达式（函数/关键字映射；字面量原样保留）
///
/// 序列 / 表达式默认值（nextval / currval / gen_random / uuid_generate 等）不可移植，
/// 直接返回 None，由预检 verdict 提示「自增/默认值语义丢失」。
pub(crate) fn translate_default(default: &str, target: DatabaseType) -> Option<String> {
    let d = default.trim();
    if d.is_empty() {
        return None;
    }
    let lower = d.to_ascii_lowercase();
    // MySQL 8 表达式默认值 DEFAULT (uuid()) 会带一层或多层括号，且括号内可能有空白；
    // 循环剥离成对括号 + 容忍内部空白，堵住 "( uuid() )" 类绕过。
    // normalized 仅用于检测不可移植前缀，输出 DDL 必须使用原始 d 保留大小写与原义。
    let mut normalized: &str = &lower;
    while let Some(inner) = normalized
        .strip_prefix('(')
        .and_then(|s| s.trim().strip_suffix(')'))
    {
        normalized = inner.trim();
    }
    // 紧凑视图：剔除所有空白（封堵 "uuid ()" / "nextval ( 's' )" 这类函数名与括号
    // 之间的空白绕过）。SQL 引擎对 "uuid ()" 与 "uuid()" 一视同仁。仅用于检测，
    // 输出仍走原始 d。引号包裹的字面量（"'uuid()'"）以单引号起始不会被前缀表误伤。
    let compact: String = normalized.chars().filter(|c| !c.is_whitespace()).collect();
    const NON_PORTABLE_DEFAULT_PREFIXES: &[&str] = &[
        "nextval(",
        "currval(",
        "gen_random_uuid",
        "uuid_generate",
        // MySQL / Oracle / SQL Server 等价物
        "uuid(",
        "sys_guid(",
        "newid(",
        "newsequentialid(",
    ];
    if NON_PORTABLE_DEFAULT_PREFIXES
        .iter()
        .any(|prefix| compact.starts_with(prefix))
    {
        return None;
    }
    // 时间类函数/关键字统一为目标方言的当前时间戳
    let is_now = lower == "now()"
        || lower == "current_timestamp"
        || lower == "current_timestamp()"
        || lower.starts_with("current_timestamp(")
        || lower == "getdate()";
    if is_now {
        return Some("CURRENT_TIMESTAMP".to_string());
    }
    let is_today = lower == "curdate()"
        || lower == "current_date"
        || lower == "current_date()"
        || lower == "today()";
    if is_today {
        return Some("CURRENT_DATE".to_string());
    }
    // PG 布尔字面量 → MySQL/SQLite 用 1/0（tinyint/integer）
    if matches!(lower.as_str(), "true" | "false")
        && matches!(target, DatabaseType::MySQL | DatabaseType::SQLite)
    {
        return Some(if lower == "true" { "1" } else { "0" }.to_string());
    }
    Some(d.to_string())
}

/// 对外入口：把源列列表整体转换为目标方言形态（保持列序）
pub fn translate_columns_for_target(
    columns: Vec<ColumnInfo>,
    source: DatabaseType,
    target: DatabaseType,
) -> Vec<ColumnInfo> {
    columns
        .into_iter()
        .map(|mut col| {
            let spec = normalize_type(&col.data_type, source);
            col.data_type = render_type(&spec, target);
            if let Some(d) = col.default_value.clone() {
                col.default_value = translate_default(&d, target);
            }
            col
        })
        .collect()
}

/// 兼容性结论（预检与建表共用一套模型）
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TypeVerdict {
    Compatible,
    /// 有精度/语义损失，附说明（已本地化）
    Lossy(String),
    /// 目标库无对应类型，附说明（已本地化）
    Unsupported(String),
}

pub(crate) fn column_verdict(spec: &ColumnSpec, target: DatabaseType) -> TypeVerdict {
    if spec.unsigned {
        return TypeVerdict::Lossy(t!("DataTransfer.precheck_unsigned_lossy").to_string());
    }
    match (&spec.kind, target) {
        (ColumnKind::Array(_), DatabaseType::PostgreSQL) => TypeVerdict::Compatible,
        (ColumnKind::Array(_), _) => {
            TypeVerdict::Unsupported(t!("DataTransfer.precheck_array_unsupported").to_string())
        }
        (ColumnKind::Uuid, DatabaseType::PostgreSQL) => TypeVerdict::Compatible,
        (ColumnKind::Uuid, _) => {
            TypeVerdict::Lossy(t!("DataTransfer.precheck_uuid_lossy").to_string())
        }
        (ColumnKind::Boolean, DatabaseType::MySQL) => {
            TypeVerdict::Lossy(t!("DataTransfer.precheck_boolean_mysql").to_string())
        }
        (ColumnKind::Json, DatabaseType::SQLite) => {
            TypeVerdict::Lossy(t!("DataTransfer.precheck_json_sqlite").to_string())
        }
        (ColumnKind::TimestampTz, DatabaseType::MySQL) => {
            TypeVerdict::Lossy(t!("DataTransfer.precheck_tstz_mysql").to_string())
        }
        (ColumnKind::TimestampTz, DatabaseType::SQLite) => {
            TypeVerdict::Lossy(t!("DataTransfer.precheck_tstz_sqlite").to_string())
        }
        (ColumnKind::Decimal(None), _) => {
            TypeVerdict::Lossy(t!("DataTransfer.precheck_decimal_precision_fallback").to_string())
        }
        (ColumnKind::Other(_), DatabaseType::SQLite) => TypeVerdict::Compatible,
        (ColumnKind::Other(_), _) => {
            TypeVerdict::Lossy(t!("DataTransfer.precheck_other_unsupported").to_string())
        }
        _ => TypeVerdict::Compatible,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(t: &str, source: DatabaseType) -> ColumnSpec {
        normalize_type(t, source)
    }

    #[test]
    fn mysql_to_pg() {
        let m = DatabaseType::MySQL;
        let p = DatabaseType::PostgreSQL;
        for (src, want) in [
            ("bigint(20)", "bigint"),
            ("int(11)", "integer"),
            // tinyint(1) 不再归一 Boolean：保持 Int(1) 避免 -128..127 中的状态码列被映射成 boolean 后写失败
            ("tinyint(1)", "smallint"),
            ("tinyint(4)", "smallint"),
            ("mediumint", "integer"),
            ("varchar(50)", "varchar(50)"),
            ("double", "double precision"),
            ("decimal(10,2)", "numeric(10,2)"),
            ("datetime", "timestamp"),
            ("longblob", "bytea"),
            ("enum('a','b')", "text"),
            ("json", "jsonb"),
            // unsigned 升档到更宽有符号整型：int unsigned(0..2^32-1) -> bigint
            ("int unsigned", "bigint"),
        ] {
            assert_eq!(render_type(&spec(src, m), p), want, "MySQL {src} -> PG");
        }
    }

    #[test]
    fn pg_to_mysql() {
        let p = DatabaseType::PostgreSQL;
        let m = DatabaseType::MySQL;
        for (src, want) in [
            ("uuid", "varchar(36)"),
            ("jsonb", "json"),
            ("boolean", "tinyint(1)"),
            ("bytea", "blob"),
            ("timestamptz", "datetime"),
            ("character varying(64)", "varchar(64)"),
            ("numeric(8,3)", "decimal(8,3)"),
            ("serial", "bigint"),
            ("text", "text"),
        ] {
            assert_eq!(render_type(&spec(src, p), m), want, "PG {src} -> MySQL");
        }
    }

    #[test]
    fn mysql_to_sqlite() {
        let m = DatabaseType::MySQL;
        let s = DatabaseType::SQLite;
        for (src, want) in [
            ("bigint(20)", "integer"),
            ("varchar(50)", "text"),
            ("timestamp", "text"),
            ("tinyint(1)", "integer"),
            ("blob", "blob"),
        ] {
            assert_eq!(render_type(&spec(src, m), s), want, "MySQL {src} -> SQLite");
        }
    }

    #[test]
    fn sqlite_to_mysql_and_pg() {
        let s = DatabaseType::SQLite;
        assert_eq!(render_type(&spec("TEXT", s), DatabaseType::MySQL), "text");
        assert_eq!(
            render_type(&spec("INTEGER", s), DatabaseType::MySQL),
            "bigint"
        );
        assert_eq!(render_type(&spec("REAL", s), DatabaseType::MySQL), "double");
        assert_eq!(
            render_type(&spec("DOUBLE", s), DatabaseType::PostgreSQL),
            "double precision"
        );
    }

    #[test]
    fn pg_array_roundtrip() {
        let p = DatabaseType::PostgreSQL;
        assert_eq!(render_type(&spec("text[]", p), DatabaseType::MySQL), "json");
        assert_eq!(
            render_type(&spec("text[]", p), DatabaseType::SQLite),
            "text"
        );
        assert_eq!(render_type(&spec("text[]", p), p), "text[]");
        rust_i18n::set_locale("zh-CN");
        assert_eq!(
            column_verdict(&spec("text[]", p), DatabaseType::MySQL),
            TypeVerdict::Unsupported(t!("DataTransfer.precheck_array_unsupported").to_string())
        );
    }

    #[test]
    fn defaults_translate() {
        assert_eq!(
            translate_default("current_timestamp()", DatabaseType::SQLite).as_deref(),
            Some("CURRENT_TIMESTAMP")
        );
        assert_eq!(
            translate_default("now()", DatabaseType::PostgreSQL).as_deref(),
            Some("CURRENT_TIMESTAMP")
        );
        assert_eq!(
            translate_default("CURRENT_TIMESTAMP(3)", DatabaseType::SQLite).as_deref(),
            Some("CURRENT_TIMESTAMP")
        );
        assert_eq!(
            translate_default("'abc'", DatabaseType::MySQL).as_deref(),
            Some("'abc'")
        );
        assert_eq!(
            translate_default("true", DatabaseType::MySQL).as_deref(),
            Some("1")
        );
        assert_eq!(
            translate_default("false", DatabaseType::PostgreSQL).as_deref(),
            Some("false")
        );
        assert_eq!(
            translate_default("0", DatabaseType::PostgreSQL).as_deref(),
            Some("0")
        );
        // 序列 / 函数型默认值：丢弃（不可移植）
        assert_eq!(
            translate_default("nextval('seq')", DatabaseType::MySQL),
            None
        );
        assert_eq!(
            translate_default("currval('seq')", DatabaseType::SQLite),
            None
        );
        assert_eq!(
            translate_default("gen_random_uuid()", DatabaseType::MySQL),
            None
        );
        assert_eq!(
            translate_default("uuid_generate_v4()", DatabaseType::SQLite),
            None
        );
    }

    #[test]
    fn decimal_without_precision_uses_safe_fallback() {
        // PG numeric 无精度 → MySQL decimal(65,30) 兜底，避免 MySQL 默认 (10,0) 静默截断
        assert_eq!(
            render_type(
                &normalize_type("numeric", DatabaseType::PostgreSQL),
                DatabaseType::MySQL
            ),
            "decimal(65,30)"
        );
        assert_eq!(
            render_type(
                &normalize_type("numeric", DatabaseType::PostgreSQL),
                DatabaseType::PostgreSQL
            ),
            "numeric(65,30)"
        );
        rust_i18n::set_locale("zh-CN");
        assert_eq!(
            column_verdict(
                &normalize_type("numeric", DatabaseType::PostgreSQL),
                DatabaseType::MySQL
            ),
            TypeVerdict::Lossy(t!("DataTransfer.precheck_decimal_precision_fallback").to_string())
        );
    }

    #[test]
    fn tinyint_one_keeps_int_semantics() {
        // tinyint(1) 归 Int(1)，避免被 PG boolean 拒收 -128..127 范围外的值
        let m = DatabaseType::MySQL;
        let p = DatabaseType::PostgreSQL;
        assert_eq!(render_type(&spec("tinyint(1)", m), m), "tinyint");
        assert_eq!(render_type(&spec("tinyint(1)", m), p), "smallint");
        assert_eq!(
            column_verdict(&spec("tinyint(1)", m), p),
            TypeVerdict::Compatible
        );
    }

    #[test]
    fn verdicts_flag_lossy_cases() {
        let m = DatabaseType::MySQL;
        let p = DatabaseType::PostgreSQL;
        rust_i18n::set_locale("zh-CN");
        assert_eq!(
            column_verdict(&spec("uuid", p), m),
            TypeVerdict::Lossy(t!("DataTransfer.precheck_uuid_lossy").to_string())
        );
        assert_eq!(
            column_verdict(&spec("bigint(20) unsigned", m), p),
            TypeVerdict::Lossy(t!("DataTransfer.precheck_unsigned_lossy").to_string())
        );
        assert_eq!(
            column_verdict(&spec("json", m), DatabaseType::SQLite),
            TypeVerdict::Lossy(t!("DataTransfer.precheck_json_sqlite").to_string())
        );
        assert_eq!(column_verdict(&spec("int", m), p), TypeVerdict::Compatible);
    }

    #[test]
    fn unsigned_promotes_to_wider_signed() {
        // 无符号侧：tinyint u(0..255) -> smallint; smallint u(0..65535) -> int;
        // mediumint u(0..16777215) -> int; int u(0..2^32-1) -> bigint；
        // bigint u 仍归 bigint，由 verdict 提示「超 bigint 上限」。
        // 有符号侧：mediumint -> integer（小宽度归位），锁定 Int(3) 合法性。
        let m = DatabaseType::MySQL;
        let p = DatabaseType::PostgreSQL;
        assert_eq!(render_type(&spec("tinyint unsigned", m), p), "smallint");
        assert_eq!(render_type(&spec("smallint unsigned", m), p), "integer");
        assert_eq!(render_type(&spec("mediumint unsigned", m), p), "integer");
        assert_eq!(render_type(&spec("int unsigned", m), p), "bigint");
        assert_eq!(render_type(&spec("bigint unsigned", m), p), "bigint");
        // 有符号侧回归
        assert_eq!(render_type(&spec("mediumint", m), p), "integer");
        assert_eq!(render_type(&spec("tinyint", m), p), "smallint");
        assert_eq!(render_type(&spec("smallint", m), p), "smallint");
        assert_eq!(render_type(&spec("int", m), p), "integer");
        assert_eq!(render_type(&spec("bigint", m), p), "bigint");
        // 变体断言而非 locale 文案：避免 set_locale 与并行测试竞态
        assert!(matches!(
            column_verdict(&spec("bigint unsigned", m), p),
            TypeVerdict::Lossy(_)
        ));
    }

    #[test]
    fn defaults_block_whitespace_between_name_and_parens() {
        // SQL 引擎对 "uuid ()" 与 "uuid()" 一视同仁；检测必须同等宽容
        for d in [
            "uuid ()",
            "UUID  ()",
            "nextval ( 's' )",
            "newid\t()",
            "sys_guid\n()",
            "( uuid () )",
        ] {
            assert_eq!(
                translate_default(d, DatabaseType::SQLite),
                None,
                "{d} should be blocked"
            );
        }
        // 引号包裹的字面量不应被误伤
        assert!(translate_default("'uuid()'", DatabaseType::SQLite).is_some());
        // 普通字符串字面量同样不应被拦
        assert!(translate_default("'hello world'", DatabaseType::MySQL).is_some());
    }

    #[test]
    fn defaults_block_whitespace_and_nested_parens() {
        // 括号剥离需容忍空白与多层嵌套，避免 "( uuid() )" 类绕过
        for d in [
            "( uuid() )",
            "((NEWID()))",
            "(  SYS_GUID( )  )",
            "  (   newsequentialid()   )  ",
        ] {
            assert_eq!(
                translate_default(d, DatabaseType::SQLite),
                None,
                "{d} should be blocked"
            );
        }
    }

    #[test]
    fn defaults_block_extra_dialects() {
        // 拦截 MySQL/Oracle/SQL Server 方向的不可移植默认值
        assert_eq!(translate_default("UUID()", DatabaseType::PostgreSQL), None);
        assert_eq!(
            translate_default("(UUID())", DatabaseType::PostgreSQL),
            None
        );
        assert_eq!(translate_default("sys_guid()", DatabaseType::MySQL), None);
        assert_eq!(translate_default("newid()", DatabaseType::PostgreSQL), None);
        assert_eq!(
            translate_default("newsequentialid()", DatabaseType::MySQL),
            None
        );
    }
}

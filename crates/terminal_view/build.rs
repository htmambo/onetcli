//! Build script for terminal_view.
//!
//! 1. Watches locales directory for i18n changes
//! 2. Parses tabby-community-color-schemes and generates Rust theme code

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=locales");

    let schemes_dir = find_schemes_dir();
    if let Some(dir) = schemes_dir {
        println!("cargo:rerun-if-changed={}", dir.display());
        let themes = parse_all_schemes(&dir);
        let output = generate_rust_code(&themes);
        let out_dir = env::var("OUT_DIR").unwrap();
        let dest_path = Path::new(&out_dir).join("tabby_themes.rs");
        fs::write(&dest_path, &output).unwrap();
    }
}

/// 尝试查找 tabby 方案目录
fn find_schemes_dir() -> Option<std::path::PathBuf> {
    // 从 crate 源码目录向上搜索
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").ok()?;
    let crate_src = Path::new(&manifest_dir).join("src");

    for ancestor in crate_src.ancestors() {
        let candidate = ancestor.join("tabby").join("tabby-community-color-schemes").join("schemes");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }

    // 从 OUT_DIR 向上搜索（用于 workspace 构建场景）
    let out_dir = env::var("OUT_DIR").ok()?;
    for ancestor in Path::new(&out_dir).ancestors() {
        let candidate = ancestor.join("tabby").join("tabby-community-color-schemes").join("schemes");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }

    None
}

/// 方案元数据
#[derive(Debug)]
struct SchemeMeta {
    name: String,
    foreground: String,
    background: String,
    cursor: String,
    colors: [String; 16],
}

/// 解析单个方案文件
fn parse_scheme(path: &Path) -> Option<SchemeMeta> {
    let content = fs::read_to_string(path).ok()?;
    let name = path.file_name()?.to_str()?.to_string();

    // 第一遍：收集所有 #define 宏定义
    let mut macros: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("#define ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 2 {
                macros.insert(parts[0].to_string(), parts[1].to_string());
            }
        }
    }

    // 第二遍：解析键值对，将宏引用替换为实际值
    let mut foreground = String::new();
    let mut background = String::new();
    let mut cursor = String::new();
    let mut colors: [String; 16] = [
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    ];

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('!') || line.is_empty() || line.starts_with("#define") {
            continue;
        }

        let resolve = |raw: &str| -> String {
            let raw = raw.trim();
            if is_valid_hex(raw) {
                raw.to_string()
            } else {
                macros.get(raw).cloned().unwrap_or_else(|| raw.to_string())
            }
        };

        if let Some(val) = line.strip_prefix("*.foreground:") {
            foreground = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.background:") {
            background = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.cursorColor:") {
            cursor = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color0:") {
            colors[0] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color1:") {
            colors[1] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color2:") {
            colors[2] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color3:") {
            colors[3] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color4:") {
            colors[4] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color5:") {
            colors[5] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color6:") {
            colors[6] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color7:") {
            colors[7] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color8:") {
            colors[8] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color9:") {
            colors[9] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color10:") {
            colors[10] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color11:") {
            colors[11] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color12:") {
            colors[12] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color13:") {
            colors[13] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color14:") {
            colors[14] = resolve(val);
        } else if let Some(val) = line.strip_prefix("*.color15:") {
            colors[15] = resolve(val);
        }
    }

    // 过滤掉缺少必要颜色的方案
    if foreground.is_empty()
        || background.is_empty()
        || colors[0].is_empty()
        || !is_valid_hex(&foreground)
        || !is_valid_hex(&background)
    {
        return None;
    }
    // 验证所有 ANSI 颜色都是有效的十六进制值
    for color in &colors {
        if !color.is_empty() && !is_valid_hex(color) {
            return None;
        }
    }
    let cursor = if is_valid_hex(&cursor) {
        cursor
    } else {
        foreground.clone()
    };

    Some(SchemeMeta {
        name,
        foreground,
        background,
        cursor,
        colors,
    })
}

/// 解析所有方案
fn parse_all_schemes(dir: &Path) -> Vec<SchemeMeta> {
    let mut schemes = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                if let Some(meta) = parse_scheme(&entry.path()) {
                    schemes.push(meta);
                }
            }
        }
    }
    schemes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    schemes
}

/// 判断是否为暗色方案（通过背景色亮度）
fn is_dark(hex: &str) -> bool {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return true;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f32;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f32;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f32;
    let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    l < 128.0
}

/// 检查是否为有效的十六进制颜色值（不是宏变量名）
fn is_valid_hex(hex: &str) -> bool {
    let hex = hex.trim().trim_start_matches('#');
    hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// 生成 Rust 代码
fn generate_rust_code(schemes: &[SchemeMeta]) -> String {
    let mut out = String::new();
    out.push_str("// Generated by build.rs - DO NOT EDIT\n\n");

    for scheme in schemes {
        let fn_name = sanitize_name(&scheme.name);
        let variant = if is_dark(&scheme.background) {
            "ThemeVariant::Dark"
        } else {
            "ThemeVariant::Light"
        };

        out.push_str(&format!(
            "/// {name} (from tabby-community-color-schemes)\n",
            name = scheme.name
        ));
        out.push_str(&format!(
            r#"pub fn {fn_name}() -> TerminalTheme {{
    let palette = AnsiPalette {{
        color0:  rgb(0x{colors0}),  // black
        color1:  rgb(0x{colors1}),  // red
        color2:  rgb(0x{colors2}),  // green
        color3:  rgb(0x{colors3}),  // yellow
        color4:  rgb(0x{colors4}),  // blue
        color5:  rgb(0x{colors5}),  // magenta
        color6:  rgb(0x{colors6}),  // cyan
        color7:  rgb(0x{colors7}),  // white
        color8:  rgb(0x{colors8}),   // bright black
        color9:  rgb(0x{colors9}),   // bright red
        color10: rgb(0x{colors10}),  // bright green
        color11: rgb(0x{colors11}),  // bright yellow
        color12: rgb(0x{colors12}),  // bright blue
        color13: rgb(0x{colors13}),  // bright magenta
        color14: rgb(0x{colors14}),  // bright cyan
        color15: rgb(0x{colors15}),  // bright white
    }};
    TerminalTheme::with_palette(
        "{name}",
        {variant},
        rgb(0x{fg}).into(),
        rgb(0x{bg}).into(),
        rgb(0x{cursor}).into(),
        rgb(0x{cursor}).into(),
        palette,
    )
}}
"#,
            fn_name = fn_name,
            name = scheme.name,
            variant = variant,
            fg = hex_to_rgb(&scheme.foreground),
            bg = hex_to_rgb(&scheme.background),
            cursor = hex_to_rgb(&scheme.cursor),
            colors0 = hex_to_rgb(&scheme.colors[0]),
            colors1 = hex_to_rgb(&scheme.colors[1]),
            colors2 = hex_to_rgb(&scheme.colors[2]),
            colors3 = hex_to_rgb(&scheme.colors[3]),
            colors4 = hex_to_rgb(&scheme.colors[4]),
            colors5 = hex_to_rgb(&scheme.colors[5]),
            colors6 = hex_to_rgb(&scheme.colors[6]),
            colors7 = hex_to_rgb(&scheme.colors[7]),
            colors8 = hex_to_rgb(&scheme.colors[8]),
            colors9 = hex_to_rgb(&scheme.colors[9]),
            colors10 = hex_to_rgb(&scheme.colors[10]),
            colors11 = hex_to_rgb(&scheme.colors[11]),
            colors12 = hex_to_rgb(&scheme.colors[12]),
            colors13 = hex_to_rgb(&scheme.colors[13]),
            colors14 = hex_to_rgb(&scheme.colors[14]),
            colors15 = hex_to_rgb(&scheme.colors[15]),
        ));
        out.push('\n');
    }

    out.push_str("/// All tabby schemes combined\n");
    out.push_str("pub fn tabby_all() -> Vec<TerminalTheme> {\n");
    out.push_str("    vec![\n");
    for scheme in schemes {
        let fn_name = sanitize_name(&scheme.name);
        out.push_str(&format!("        {fn_name}(),\n"));
    }
    out.push_str("    ]\n");
    out.push_str("}\n");

    out
}

fn sanitize_name(name: &str) -> String {
    let base: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    // 合并连续的下划线
    let base = base.split('_').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("_");
    // Rust 标识符不能以数字开头
    if base.is_empty() || base.chars().next().unwrap().is_ascii_digit() {
        format!("tabby_{}", base)
    } else {
        base
    }
}

fn hex_to_rgb(hex: &str) -> String {
    let hex = hex.trim_start_matches('#').trim();
    if hex.len() < 6 {
        return "000000".to_string();
    }
    format!("{}{}{}", &hex[0..2], &hex[2..4], &hex[4..6])
}

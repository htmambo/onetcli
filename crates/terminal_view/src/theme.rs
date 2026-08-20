//! 终端主题配置
//!
//! 提供终端的颜色、字体、字号等外观设置
//!
//! ## 配色系统设计
//!
//! 本模块采用语义化配色系统，确保所有颜色组合具有足够的对比度：
//! - `background` / `foreground`: 主要背景和文字，对比度 >= 7:1
//! - `muted` / `muted_foreground`: 次要区域背景和文字，对比度 >= 4.5:1
//! - `accent` / `accent_foreground`: 强调色背景和文字，对比度 >= 4.5:1
//!
//! 颜色使用规则：
//! - 在 `background` 上使用 `foreground` 或 `muted_foreground`
//! - 在 `muted` 上使用 `foreground` 或 `muted_foreground`
//! - 在 `accent` 上使用 `accent_foreground`

use gpui::{Hsla, Pixels, Rgba, SharedString, rgb};
use gpui_component::{Theme as UiTheme, level_surface_color};

pub const FOLLOW_APP_THEME_NAME: &str = "App Theme";

/// 终端主题配色类型
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ThemeVariant {
    /// 暗色主题
    Dark,
    /// 亮色主题
    Light,
    /// 中性配色，两种模式均可选
    Neutral,
}

impl ThemeVariant {
    /// 判断是否为暗色变体
    pub fn is_dark(&self) -> bool {
        matches!(self, Self::Dark)
    }

    /// 判断当前变体是否与给定模式匹配
    pub(crate) fn matches(&self, mode_is_dark: bool) -> bool {
        match self {
            Self::Dark => mode_is_dark,
            Self::Light => !mode_is_dark,
            Self::Neutral => true,
        }
    }
}

/// 默认字体大小
pub const DEFAULT_FONT_SIZE: f32 = 13.0;
/// 最小字体大小
pub const MIN_FONT_SIZE: f32 = 8.0;
/// 最大字体大小
pub const MAX_FONT_SIZE: f32 = 32.0;
/// 默认行高比例
pub const DEFAULT_LINE_HEIGHT_SCALE: f32 = 1.4;
/// 最小行高比例
pub const MIN_LINE_HEIGHT_SCALE: f32 = 1.0;
/// 最大行高比例
pub const MAX_LINE_HEIGHT_SCALE: f32 = 2.5;

/// ANSI 16 色调色板（color0-color15）
///
/// 用于终端程序输出着色（ls --color, git diff, bat 等）。
/// 通过 OSC 4 序列注入到终端。
#[derive(Clone, Debug)]
pub struct AnsiPalette {
    pub color0: Rgba,  // black
    pub color1: Rgba,  // red
    pub color2: Rgba,  // green
    pub color3: Rgba,  // yellow
    pub color4: Rgba,  // blue
    pub color5: Rgba,  // magenta
    pub color6: Rgba,  // cyan
    pub color7: Rgba,  // white
    pub color8: Rgba,  // bright black
    pub color9: Rgba,  // bright red
    pub color10: Rgba, // bright green
    pub color11: Rgba, // bright yellow
    pub color12: Rgba, // bright blue
    pub color13: Rgba, // bright magenta
    pub color14: Rgba, // bright cyan
    pub color15: Rgba, // bright white
}

impl PartialEq for AnsiPalette {
    fn eq(&self, other: &Self) -> bool {
        self.color0 == other.color0
            && self.color1 == other.color1
            && self.color2 == other.color2
            && self.color3 == other.color3
            && self.color4 == other.color4
            && self.color5 == other.color5
            && self.color6 == other.color6
            && self.color7 == other.color7
            && self.color8 == other.color8
            && self.color9 == other.color9
            && self.color10 == other.color10
            && self.color11 == other.color11
            && self.color12 == other.color12
            && self.color13 == other.color13
            && self.color14 == other.color14
            && self.color15 == other.color15
    }
}

impl Eq for AnsiPalette {}

impl Default for AnsiPalette {
    fn default() -> Self {
        Self {
            color0: rgb(0x00_00_00),
            color1: rgb(0xcc_00_00),
            color2: rgb(0x4e_c9_b0),
            color3: rgb(0xc5_8a_23),
            color4: rgb(0x56_6d_c5),
            color5: rgb(0xc6_00_8b),
            color6: rgb(0xce_8e_4f),
            color7: rgb(0xe5_e5_e5),
            color8: rgb(0x66_66_66),
            color9: rgb(0xff_66_66),
            color10: rgb(0x99_ff_66),
            color11: rgb(0xff_ff_66),
            color12: rgb(0x66_66_ff),
            color13: rgb(0xff_66_ff),
            color14: rgb(0x66_ff_ff),
            color15: rgb(0xff_ff_ff),
        }
    }
}

impl AnsiPalette {
    /// 生成 OSC 4 序列，将调色板应用到终端
    ///
    /// 格式：\x1b]4;{index};rgb:{r}/{g}/{b}\x07
    /// 这是设置 ANSI 调色板的标准 VT100/xterm 方式。
    pub fn to_osc4_sequence(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16 * 32);
        for (i, color) in self.to_array().iter().enumerate() {
            let r = (color.r * 255.0) as u8;
            let g = (color.g * 255.0) as u8;
            let b = (color.b * 255.0) as u8;
            // OSC 4: set color {i} to rgb:{r}/{g}/{b}
            buf.extend_from_slice(b"\x1b]4;");
            buf.extend_from_slice(i.to_string().as_bytes());
            buf.extend_from_slice(b";rgb:");
            buf.extend_from_slice(format!("{:02x}/{:02x}/{:02x}", r, g, b).as_bytes());
            buf.push(b'\x07'); // ST (String Terminator)
        }
        buf
    }

    /// 转换为 Rgba 数组
    fn to_array(&self) -> [Rgba; 16] {
        [
            self.color0,
            self.color1,
            self.color2,
            self.color3,
            self.color4,
            self.color5,
            self.color6,
            self.color7,
            self.color8,
            self.color9,
            self.color10,
            self.color11,
            self.color12,
            self.color13,
            self.color14,
            self.color15,
        ]
    }
}

/// 终端主题配色（用于侧边栏等 UI 组件）
///
/// 所有颜色对都经过对比度验证，确保可读性：
/// - `background` + `foreground`: 主要内容
/// - `background` + `muted_foreground`: 次要内容
/// - `muted` + `foreground`: 卡片/列表项上的主要内容
/// - `muted` + `muted_foreground`: 卡片/列表项上的次要内容
/// - `accent` + `accent_foreground`: 按钮/选中状态
#[derive(Clone, Debug)]
pub struct TerminalColors {
    /// 主背景色
    pub background: Hsla,
    /// 主前景色（在 background 上使用）
    pub foreground: Hsla,
    /// 次要背景色（卡片、列表项、悬停状态）
    pub muted: Hsla,
    /// 次要前景色（次要文字、标签、占位符）
    pub muted_foreground: Hsla,
    /// 边框色
    pub border: Hsla,
    /// 强调背景色（按钮、选中项）
    pub accent: Hsla,
    /// 强调前景色（在 accent 背景上使用）
    pub accent_foreground: Hsla,
}

/// 终端主题配置
#[derive(Clone, Debug)]
pub struct TerminalTheme {
    /// 主题名称
    pub name: &'static str,
    /// 配色类型（影响过滤显示逻辑）
    pub variant: ThemeVariant,
    /// 前景色（文字颜色）
    pub foreground: Hsla,
    /// 背景色
    pub background: Hsla,
    /// 光标颜色
    pub cursor: Hsla,
    /// 选中区域颜色
    pub selection: Hsla,
    /// ANSI 16 色调色板
    pub ansi_palette: AnsiPalette,
    /// 主字体
    pub font_family: SharedString,
    /// 字体大小
    pub font_size: Pixels,
    /// 备用字体列表
    pub font_fallbacks: Vec<SharedString>,
    /// 行高比例
    pub line_height_scale: f32,
}

impl PartialEq for TerminalTheme {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.foreground == other.foreground
            && self.background == other.background
            && self.cursor == other.cursor
            && self.selection == other.selection
            && self.font_family == other.font_family
            && self.font_size == other.font_size
            && self.line_height_scale == other.line_height_scale
            && self.ansi_palette == other.ansi_palette
    }
}

/// 获取当前操作系统的默认等宽字体
pub fn default_monospace_font() -> &'static str {
    if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        // Linux 和其他系统
        "DejaVu Sans Mono"
    }
}

/// 默认备用字体列表（按优先级排序，跨平台兼容）
pub fn default_font_fallbacks() -> Vec<SharedString> {
    if cfg!(target_os = "macos") {
        vec![
            "Monaco".into(),
            "SF Mono".into(),
            "Courier New".into(),
            "Apple Color Emoji".into(),
            "Apple Symbols".into(),
            "Noto Sans Mono CJK SC".into(),
            "Source Han Mono SC".into(),
            "PingFang SC".into(),
            "PingFang TC".into(),
            "Hiragino Sans GB".into(),
            "JetBrains Mono".into(),
        ]
    } else if cfg!(target_os = "windows") {
        vec![
            "Cascadia Mono".into(),
            "Courier New".into(),
            "JetBrains Mono".into(),
            "Lucida Console".into(),
            "Segoe UI Emoji".into(),
            "Noto Sans Mono CJK SC".into(),
            "Source Han Mono SC".into(),
            "Microsoft YaHei".into(),
            "SimSun".into(),
        ]
    } else {
        // Linux 和其他系统
        vec![
            "Ubuntu Mono".into(),
            "Liberation Mono".into(),
            "Courier New".into(),
            "JetBrains Mono".into(),
            "Noto Color Emoji".into(),
            "Noto Sans Mono CJK SC".into(),
            "Source Han Mono SC".into(),
            "Noto Sans CJK SC".into(),
            "WenQuanYi Micro Hei".into(),
        ]
    }
}

impl TerminalTheme {
    pub fn follow_app(theme: &UiTheme) -> Self {
        let editor_background = theme
            .highlight_theme
            .style
            .editor_background
            .unwrap_or_else(|| theme.input_background());
        // Apply level_surface_color to make terminal background transparent like other UI surfaces.
        // Level 1 = lightest/most transparent (root background level).
        let editor_background = level_surface_color(
            editor_background,
            theme.window_blur_enabled,
            theme.backdrop_opacity,
            1,
        );
        let editor_foreground = theme
            .highlight_theme
            .style
            .editor_foreground
            .unwrap_or(theme.foreground);
        let cursor = if theme.primary.a > 0.0 {
            theme.primary
        } else {
            editor_foreground
        };
        let variant = if theme.mode.is_dark() {
            ThemeVariant::Dark
        } else {
            ThemeVariant::Light
        };

        let ansi_palette = AnsiPalette {
            color0: hsla_to_rgba(adjust_lightness(
                editor_background,
                if theme.mode.is_dark() { -0.08 } else { 0.08 },
            )),
            color1: hsla_to_rgba(theme.base.red),
            color2: hsla_to_rgba(theme.base.green),
            color3: hsla_to_rgba(theme.base.yellow),
            color4: hsla_to_rgba(theme.base.blue),
            color5: hsla_to_rgba(theme.base.magenta),
            color6: hsla_to_rgba(theme.base.cyan),
            color7: hsla_to_rgba(editor_foreground),
            color8: hsla_to_rgba(adjust_lightness(
                editor_background,
                if theme.mode.is_dark() { 0.18 } else { -0.18 },
            )),
            color9: hsla_to_rgba(prefer_light_variant(theme.base.red_light, theme.base.red)),
            color10: hsla_to_rgba(prefer_light_variant(
                theme.base.green_light,
                theme.base.green,
            )),
            color11: hsla_to_rgba(prefer_light_variant(
                theme.base.yellow_light,
                theme.base.yellow,
            )),
            color12: hsla_to_rgba(prefer_light_variant(theme.base.blue_light, theme.base.blue)),
            color13: hsla_to_rgba(prefer_light_variant(
                theme.base.magenta_light,
                theme.base.magenta,
            )),
            color14: hsla_to_rgba(prefer_light_variant(theme.base.cyan_light, theme.base.cyan)),
            color15: hsla_to_rgba(adjust_lightness(
                editor_foreground,
                if theme.mode.is_dark() { 0.08 } else { -0.08 },
            )),
        };

        Self::with_palette(
            FOLLOW_APP_THEME_NAME,
            variant,
            editor_foreground,
            editor_background,
            cursor,
            theme.selection,
            ansi_palette,
        )
    }

    pub fn is_follow_app(&self) -> bool {
        self.name == FOLLOW_APP_THEME_NAME
    }

    /// 获取所有可用主题
    pub fn all() -> Vec<Self> {
        let mut themes = vec![
            Self::midnight(),
            Self::daylight(),
            Self::ink(),
            Self::paper(),
            Self::ocean(),
            Self::obsidian(),
            Self::lotus(),
            Self::neon_blue(),
            Self::matrix(),
            Self::crimson(),
        ];

        // 按名称字母顺序排列
        themes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        themes
    }

    /// 创建带有默认 ANSI 调色板的主题（用于内置主题）
    fn with_default_font(
        name: &'static str,
        variant: ThemeVariant,
        foreground: Hsla,
        background: Hsla,
        cursor: Hsla,
        selection: Hsla,
    ) -> Self {
        Self {
            name,
            variant,
            foreground,
            background,
            cursor,
            selection,
            ansi_palette: AnsiPalette::default(),
            font_family: default_monospace_font().into(),
            font_size: gpui::px(DEFAULT_FONT_SIZE),
            font_fallbacks: default_font_fallbacks(),
            line_height_scale: DEFAULT_LINE_HEIGHT_SCALE,
        }
    }

    /// 创建带有自定义 ANSI 调色板的主题
    fn with_palette(
        name: &'static str,
        variant: ThemeVariant,
        foreground: Hsla,
        background: Hsla,
        cursor: Hsla,
        selection: Hsla,
        ansi_palette: AnsiPalette,
    ) -> Self {
        Self {
            name,
            variant,
            foreground,
            background,
            cursor,
            selection,
            ansi_palette,
            font_family: default_monospace_font().into(),
            font_size: gpui::px(DEFAULT_FONT_SIZE),
            font_fallbacks: default_font_fallbacks(),
            line_height_scale: DEFAULT_LINE_HEIGHT_SCALE,
        }
    }

    /// 暗夜主题（深灰背景，浅灰文字）
    pub fn midnight() -> Self {
        Self::with_default_font(
            "midnight",
            ThemeVariant::Dark,
            rgb(0xE4E4E4).into(),
            rgb(0x1E1E1E).into(),
            rgb(0xFFFFFF).into(),
            rgb(0x3D3D3D).into(),
        )
    }

    /// 明亮主题（白色背景，深灰文字）
    pub fn daylight() -> Self {
        Self::with_default_font(
            "daylight",
            ThemeVariant::Light,
            rgb(0x2E3436).into(),
            rgb(0xFFFFFF).into(),
            rgb(0x000000).into(),
            rgb(0xD3D7CF).into(),
        )
    }

    /// 墨黑主题（近黑背景，米色文字）
    pub fn ink() -> Self {
        Self::with_default_font(
            "ink",
            ThemeVariant::Dark,
            rgb(0xCECDC3).into(),
            rgb(0x100F0F).into(),
            rgb(0xDA702C).into(),
            rgb(0x282726).into(),
        )
    }

    /// 纸白主题（米白背景，深色文字）
    pub fn paper() -> Self {
        Self::with_default_font(
            "paper",
            ThemeVariant::Light,
            rgb(0x100F0F).into(),
            rgb(0xFFFCF0).into(),
            rgb(0xDA702C).into(),
            rgb(0xE6E4D9).into(),
        )
    }

    /// 海浪主题（深蓝灰背景，暖米色文字）
    pub fn ocean() -> Self {
        Self::with_default_font(
            "ocean",
            ThemeVariant::Dark,
            rgb(0xDCD7BA).into(),
            rgb(0x1F1F28).into(),
            rgb(0xC8C093).into(),
            rgb(0x2D4F67).into(),
        )
    }

    /// 黑曜主题（深棕黑背景，灰绿文字）
    pub fn obsidian() -> Self {
        Self::with_default_font(
            "obsidian",
            ThemeVariant::Dark,
            rgb(0xC5C9C5).into(),
            rgb(0x181616).into(),
            rgb(0xC8C093).into(),
            rgb(0x2D4F67).into(),
        )
    }

    /// 莲白主题（米黄背景，深灰紫文字）
    pub fn lotus() -> Self {
        Self::with_default_font(
            "lotus",
            ThemeVariant::Light,
            rgb(0x545464).into(),
            rgb(0xF2ECBC).into(),
            rgb(0x43436C).into(),
            rgb(0xB6D7A8).into(),
        )
    }

    /// 霓蓝主题（深蓝黑背景，青蓝文字）
    pub fn neon_blue() -> Self {
        Self::with_default_font(
            "neon_blue",
            ThemeVariant::Dark,
            rgb(0x00D9FF).into(),
            rgb(0x0A0E14).into(),
            rgb(0xFFFFFF).into(),
            rgb(0x1A3A52).into(),
        )
    }

    /// 矩阵主题（近黑背景，亮绿文字，Matrix 风格）
    pub fn matrix() -> Self {
        Self::with_default_font(
            "matrix",
            ThemeVariant::Dark,
            rgb(0x00FF41).into(),
            rgb(0x0D0D0D).into(),
            rgb(0xFFFFFF).into(),
            rgb(0x1A3A1A).into(),
        )
    }

    /// 赤红主题（深红黑背景，亮红文字）
    pub fn crimson() -> Self {
        Self::with_default_font(
            "crimson",
            ThemeVariant::Dark,
            rgb(0xFF5555).into(),
            rgb(0x1A0A0A).into(),
            rgb(0xFFFFFF).into(),
            rgb(0x4A1A1A).into(),
        )
    }

    /// 根据名称查找主题
    pub fn find_by_name(name: &str) -> Option<Self> {
        Self::all().into_iter().find(|t| t.name == name)
    }

    /// 设置字体大小（会限制在最小和最大值之间）
    pub fn with_font_size(mut self, size: f32) -> Self {
        let clamped = size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        self.font_size = gpui::px(clamped);
        self
    }

    /// 设置主字体
    pub fn with_font_family(mut self, family: impl Into<SharedString>) -> Self {
        self.font_family = family.into();
        self
    }

    /// 设置备用字体列表
    pub fn with_font_fallbacks(mut self, fallbacks: Vec<SharedString>) -> Self {
        self.font_fallbacks = fallbacks;
        self
    }

    /// 设置行高比例
    pub fn with_line_height_scale(mut self, scale: f32) -> Self {
        self.line_height_scale = scale.clamp(MIN_LINE_HEIGHT_SCALE, MAX_LINE_HEIGHT_SCALE);
        self
    }

    /// 调整终端主背景透明度，用于接入窗口毛玻璃效果。
    pub fn with_surface_opacity(mut self, opacity: f32) -> Self {
        self.background.a = opacity.clamp(0.0, 1.0);
        self
    }

    /// 在支持或模拟毛玻璃时，为终端主背景增加更接近 frosted glass 的材质感。
    pub fn with_material_tint(mut self, blur_enabled: bool) -> Self {
        if !blur_enabled {
            return self;
        }

        if self.is_dark() {
            // 深色主题：略微增加亮度，降低饱和度，模拟毛玻璃效果
            self.background.s *= 0.85;
            self.background.l = (self.background.l + 0.08).min(1.0);
        } else {
            self.background.s *= 0.72;
            self.background.l = (self.background.l + 0.08).min(1.0);
        }

        self
    }

    /// 获取计算后的行高
    pub fn line_height(&self) -> Pixels {
        self.font_size * self.line_height_scale
    }

    /// 判断是否为深色主题
    pub fn is_dark(&self) -> bool {
        // 根据背景色亮度判断
        self.background.l < 0.5
    }

    /// 获取用于 UI 组件的配色
    ///
    /// 该方法根据主题的基础颜色生成一套完整的 UI 配色，
    /// 所有颜色组合都保证足够的对比度以确保可读性。
    pub fn colors(&self) -> TerminalColors {
        let is_dark = self.is_dark();

        // 计算 muted 背景色（卡片、列表项等）
        let muted = if is_dark {
            // 深色主题：muted 比背景稍亮
            Hsla {
                h: self.background.h,
                s: self.background.s,
                l: (self.background.l + 0.06).min(0.25),
                a: 1.0,
            }
        } else {
            // 浅色主题：muted 比背景稍暗
            Hsla {
                h: self.background.h,
                s: self.background.s.min(0.1),
                l: (self.background.l - 0.06).max(0.85),
                a: 1.0,
            }
        };

        // 计算 muted_foreground（次要文字）
        // 关键：必须与 background 和 muted 都有足够对比度
        let muted_foreground = if is_dark {
            // 深色主题：使用中等亮度的灰色
            // 确保在深色背景上可读
            Hsla {
                h: self.foreground.h,
                s: self.foreground.s * 0.3,
                l: 0.55, // 固定中等亮度，确保在深色背景上可读
                a: 1.0,
            }
        } else {
            // 浅色主题：使用较深的灰色
            // 确保在浅色背景上可读
            Hsla {
                h: self.foreground.h,
                s: self.foreground.s * 0.3,
                l: 0.45, // 固定中等亮度，确保在浅色背景上可读
                a: 1.0,
            }
        };

        // 计算边框色
        let border = if is_dark {
            Hsla {
                h: self.background.h,
                s: self.background.s,
                l: (self.background.l + 0.12).min(0.35),
                a: 1.0,
            }
        } else {
            Hsla {
                h: self.background.h,
                s: self.background.s.min(0.1),
                l: (self.background.l - 0.15).max(0.75),
                a: 1.0,
            }
        };

        // 计算强调色前景（在 accent 背景上使用的文字颜色）
        // 根据 accent 的亮度决定使用深色还是浅色文字
        let accent_foreground = if self.cursor.l > 0.5 {
            // accent 是亮色，使用深色文字
            Hsla {
                h: self.cursor.h,
                s: self.cursor.s * 0.2,
                l: 0.1, // 深色文字
                a: 1.0,
            }
        } else {
            // accent 是暗色，使用亮色文字
            Hsla {
                h: self.cursor.h,
                s: self.cursor.s * 0.1,
                l: 0.95, // 亮色文字
                a: 1.0,
            }
        };

        TerminalColors {
            background: self.background,
            foreground: self.foreground,
            muted,
            muted_foreground,
            border,
            accent: self.cursor,
            accent_foreground,
        }
    }

    /// 获取可用的等宽字体列表（按操作系统优化排序）
    pub fn available_monospace_fonts() -> Vec<&'static str> {
        if cfg!(target_os = "macos") {
            vec![
                "Menlo", // macOS 默认
                "Monaco",
                "SF Mono",
                "Courier New",
                // 跨平台字体（需要安装）
                "Fira Code",
                "JetBrains Mono",
                "Source Code Pro",
                "Cascadia Code",
                "Hack",
                "IBM Plex Mono",
            ]
        } else if cfg!(target_os = "windows") {
            vec![
                "Consolas", // Windows 默认
                "Cascadia Mono",
                "Cascadia Code",
                "Courier New",
                "Lucida Console",
                // 跨平台字体（需要安装）
                "Fira Code",
                "JetBrains Mono",
                "Source Code Pro",
                "Hack",
                "IBM Plex Mono",
            ]
        } else {
            // Linux 和其他系统
            vec![
                "DejaVu Sans Mono", // Linux 常见默认
                "Ubuntu Mono",
                "Liberation Mono",
                "Courier New",
                // 跨平台字体（需要安装）
                "Fira Code",
                "JetBrains Mono",
                "Source Code Pro",
                "Cascadia Code",
                "Hack",
                "IBM Plex Mono",
            ]
        }
    }

    /// 获取可用的字体大小预设列表
    pub fn available_font_sizes() -> Vec<f32> {
        vec![
            8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 18.0, 20.0, 22.0, 24.0,
        ]
    }

    /// 获取可用的行高比例预设列表
    pub fn available_line_height_scales() -> Vec<f32> {
        vec![1.0, 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.8, 2.0, 2.2, 2.5]
    }
}

fn hsla_to_rgba(color: Hsla) -> Rgba {
    color.into()
}

fn adjust_lightness(mut color: Hsla, delta: f32) -> Hsla {
    color.l = (color.l + delta).clamp(0.0, 1.0);
    color
}

fn prefer_light_variant(light: Hsla, fallback: Hsla) -> Hsla {
    if light == Hsla::transparent_black() {
        fallback
    } else {
        light
    }
}

#[cfg(test)]
mod tests {
    use super::{FOLLOW_APP_THEME_NAME, TerminalTheme, default_font_fallbacks};
    use gpui_component::Theme as UiTheme;

    #[test]
    fn 终端主背景透明度可被单独调整() {
        let theme = TerminalTheme::ocean();
        let tuned = theme.clone().with_surface_opacity(0.72);

        assert_eq!(tuned.background.a, 0.72);
        assert_eq!(tuned.foreground, theme.foreground);
        assert_eq!(tuned.cursor, theme.cursor);
        assert_eq!(tuned.selection, theme.selection);
        assert_eq!(tuned.ansi_palette, theme.ansi_palette);
    }

    #[test]
    fn 终端主背景透明度会被限制在合法范围() {
        let theme = TerminalTheme::ocean();

        assert_eq!(theme.clone().with_surface_opacity(-0.5).background.a, 0.0);
        assert_eq!(theme.with_surface_opacity(1.5).background.a, 1.0);
    }

    #[test]
    fn 毛玻璃材质会调整终端背景色调() {
        let theme = TerminalTheme::ocean();
        let tinted = theme.clone().with_material_tint(true);

        assert!(tinted.background.l > theme.background.l);
        assert!(tinted.background.s < theme.background.s);
    }

    #[test]
    fn 跟随应用主题生成的终端主题使用固定名称() {
        let theme = TerminalTheme::follow_app(&UiTheme::default());

        assert_eq!(theme.name, FOLLOW_APP_THEME_NAME);
        assert!(theme.is_follow_app());
    }

    #[test]
    fn 内置终端主题已加入主题列表() {
        for theme_name in [
            "midnight", "daylight", "ocean", "obsidian", "crimson", "matrix",
        ] {
            assert!(
                TerminalTheme::find_by_name(theme_name).is_some(),
                "expected terminal theme `{theme_name}` to exist"
            );
        }
    }

    #[test]
    fn terminal_default_fallbacks_include_monospace_cjk_fonts_first() {
        let fallbacks = default_font_fallbacks()
            .into_iter()
            .map(|font| font.to_string())
            .collect::<Vec<_>>();

        let noto_mono = fallbacks
            .iter()
            .position(|font| font == "Noto Sans Mono CJK SC")
            .expect("Noto Sans Mono CJK SC should be a terminal fallback");
        let source_han_mono = fallbacks
            .iter()
            .position(|font| font == "Source Han Mono SC")
            .expect("Source Han Mono SC should be a terminal fallback");

        for ui_font in ["PingFang SC", "Microsoft YaHei", "Noto Sans CJK SC"] {
            if let Some(ui_index) = fallbacks.iter().position(|font| font == ui_font) {
                assert!(noto_mono < ui_index);
                assert!(source_han_mono < ui_index);
            }
        }
    }
}

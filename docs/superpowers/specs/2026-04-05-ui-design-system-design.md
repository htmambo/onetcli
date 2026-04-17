# OnetCLI UI 设计系统设计方案

> 版本: v1.0
> 日期: 2026-04-05
> 状态: 待评审
> 分支: feat/ui-design-system

## 1. 概述

### 1.1 目标

为 OnetCLI 建立统一的设计系统，覆盖核心页面（Home、Settings、数据库视图），保持现有暗色毛玻璃（macOS 风格）基调，整理并标准化设计 token、语义层、组件规范和布局模式。

### 1.2 范围

**纳入范围：**
- Home 页面及其子组件
- Settings 设置页面
- 数据库视图（PostgreSQL、MySQL、SQLite、MSSQL、Oracle、ClickHouse、DuckDB）
- 连接表单窗口
- 通用 UI 组件库（`crates/ui`）
- 主题系统（`crates/ui/src/theme/`）

**暂不纳入（后续迭代）：**
- Terminal 终端视图
- Redis 视图
- MongoDB 视图
- SFTP 视图

### 1.3 设计原则

1. **保持现有风格** — 不做大的风格转变，维持暗色毛玻璃 macOS 风格
2. **渐进式改动** — 先统一不一致，再逐步完善规范
3. **Token 驱动** — 所有设计决策通过 token 定义，组件引用 token 而非硬编码值
4. **语义优先** — 组件使用语义化名称（如 `content_bg`），而非底层 token（如 `rgba(30,30,30,0.8)`）

---

## 2. 现状分析

### 2.1 已发现的不一致

| 问题类别 | 具体问题 | 位置 |
|---------|---------|------|
| 侧边栏宽度不统一 | Home 255px / 数据库 400px / 聊天侧边栏 420px | `main/src/home/`, `crates/db_view/src/layout.rs`, `crates/core/src/layout.rs` |
| 间距硬编码 | 各处使用不同数值（36px/44px/48px 等） | 多个 view 文件 |
| 颜色 token 冗余 | 100+ token 中存在语义重复（如 `tab_bar` 和 `sidebar` 颜色相近但命名不同） | `crates/ui/src/theme/theme_color.rs` |
| 两套主题系统 | 通用 UI 主题 vs Terminal 专属主题，glass 实现分离 | `crates/ui/src/theme/`, `crates/terminal_view/src/theme.rs` |
| 命名不一致 | Sidebar/Dock/Panel 三种称呼混用 | 多个模块 |
| 圆角不统一 | 不同组件使用不同的 border-radius | 组件库各处 |
| 字体大小分散 | 没有统一的字体 token，组件内硬编码 | 多个组件文件 |

### 2.2 现有资产

- **颜色 Token**: 100+ 个，分布在 `ThemeColor` 结构体中
- **组件库**: 70+ 个可复用组件
- **布局系统**: DockArea（Split/Tabs/Panel/Tiles）+ Resizable Sidebar
- **毛玻璃系统**: `glass_bg()` 系列函数，支持平台差异化

---

## 3. 设计 Token 系统

### 3.1 目录结构

```
crates/ui/src/tokens/
├── mod.rs              # 模块入口，统一导出
├── color/
│   ├── mod.rs
│   ├── primitives.rs   # 原始色值（RGB/HSLA）
│   ├── semantic.rs      # 语义化颜色 token
│   └── modes.rs        # light/dark 模式变量
├── spacing.rs          # 间距 token (px 值)
├── typography.rs       # 字体 family/size/weight
├── radius.rs          # 圆角 token
├── shadow.rs          # 阴影 token
└── motion.rs           # 动画时长/缓动函数
```

### 3.2 颜色 Token

#### 3.2.1 原始色值（Primitives）

不直接被组件使用，仅作为语义 token 的基础值。

```rust
// primitives.rs
pub struct ColorPrimitives {
    // 背景色系
    pub bg_base: Hsla,      // #1a1a1a / #ffffff
    pub bg_elevated: Hsla,  // #242424 / #f5f5f5
    pub bg_surface: Hsla,    // #2d2d2d / #ebebeb

    // 边框色系
    pub border_subtle: Hsla, // #3a3a3a / #e0e0e0
    pub border_default: Hsla,// #4a4a4a / #d0d0d0

    // 文字色系
    pub text_primary: Hsla,  // #ffffff / #1a1a1a
    pub text_secondary: Hsla,// #a0a0a0 / #666666
    pub text_muted: Hsla,    // #707070 / #999999

    // 语义色
    pub primary: Hsla,       // #3b82f6 (蓝)
    pub accent: Hsla,        // #8b5cf6 (紫)
    pub success: Hsla,        // #22c55e (绿)
    pub warning: Hsla,       // #f59e0b (橙)
    pub danger: Hsla,        // #ef4444 (红)
    pub info: Hsla,          // #06b6d4 (青)
}
```

#### 3.2.2 语义化颜色（Semantic）

供组件使用，名称表达用途而非具体色值。

```rust
// semantic.rs
pub struct SemanticColors {
    // === 背景层 ===
    pub glass_base: Hsla,        // 毛玻璃基础层（最高透明度）
    pub glass_elevated: Hsla,    // 浮起的毛玻璃层
    pub glass_content: Hsla,     // 内容区背景
    pub glass_surface: Hsla,     // 卡片/面板背景
    pub glass_sidebar: Hsla,     // 侧边栏背景

    // === 边框 ===
    pub border_subtle: Hsla,      // 细线分隔
    pub border_default: Hsla,     // 标准边框
    pub border_strong: Hsla,      // 强调边框

    // === 文字 ===
    pub text_primary: Hsla,       // 主要文字
    pub text_secondary: Hsla,    // 次要文字
    pub text_muted: Hsla,        // 辅助文字
    pub text_link: Hsla,         // 链接文字
    pub text_link_hover: Hsla,    // 链接悬停

    // === 交互状态 ===
    pub interactive_primary: Hsla,
    pub interactive_hover: Hsla,
    pub interactive_active: Hsla,
    pub interactive_disabled: Hsla,

    // === 语义色 ===
    pub semantic_success: Hsla,
    pub semantic_warning: Hsla,
    pub semantic_danger: Hsla,
    pub semantic_info: Hsla,

    // === 特殊区域 ===
    pub title_bar: Hsla,
    pub tab_bar: Hsla,
    pub toolbar: Hsla,
    pub scrollbar: Hsla,
}
```

#### 3.2.3 现有 Token 清理计划

从 `theme_color.rs` 的 100+ token 中，合并冗余并映射到新语义 token：

| 旧 Token | 新 Token | 合并原因 |
|---------|---------|---------|
| `background` | `glass_base` | 合并 |
| `secondary` | `glass_surface` | 合并 |
| `sidebar` | `glass_sidebar` | 重命名 |
| `tab` | `tab_bar` | 合并 |
| `tab_bar` | 保留 | 独立语义 |
| `popover` | `glass_elevated` | 合并 |
| `overlay` | 保留 | 独立语义 |
| `border` | `border_default` | 重命名 |
| `input_border` | 合并到 form 组件内 | 组件内部 |
| `foreground` | `text_primary` | 重命名 |
| `muted_foreground` | `text_muted` | 重命名 |
| `link`/`link_hover`/`link_active` | `text_link*` | 重命名 |
| `primary`/`primary_hover`/`primary_active` | `interactive_*` | 重组 |
| `danger`/`success`/`warning`/`info` | `semantic_*` | 重命名 |
| `list`/`list_hover`/`list_active` | 合并到 list 组件内 | 组件内部 |
| `table`/`table_hover`/`table_active` | 合并到 table 组件内 | 组件内部 |
| `chart_1`..`chart_5` | 保留 | 图表专用 |
| `bullish`/`bearish` | 保留 | 图表专用 |
| `scrollbar*` | 保留 | 独立语义 |

### 3.3 间距 Token

```rust
// spacing.rs
#[derive(Clone, Copy)]
pub enum Spacing {
    // 基础间距
    Unit1  = 4,    // 4px
    Unit2  = 8,    // 8px
    Unit3  = 12,   // 12px
    Unit4  = 16,   // 16px
    Unit5  = 20,   // 20px
    Unit6  = 24,   // 24px
    Unit8  = 32,   // 32px
    Unit10 = 40,   // 40px
    Unit12 = 48,   // 48px
    Unit16 = 64,   // 64px

    // 布局间距
    PanelGap = 1,   // 面板间隙 1px
    SectionGap = 16,// 内容区块间隙
    PagePadding = 16,// 页面边距

    // 侧边栏
    SidebarWidth = 255,  // 统一侧边栏宽度（原有多种）
    SidebarMinWidth = 200,
    SidebarMaxWidth = 400,

    // 工具栏
    ToolbarHeight = 36,   // 统一工具栏高度
    ToolbarButtonSize = 28,

    // 标题栏
    TitleBarHeight = 38,
}

// 实现 IntoElement 的 gap() 扩展方法
impl Spacing {
    pub fn px(self) -> Pixels { Pixels(self.0 as f32) }
}
```

### 3.4 字体 Token

```rust
// typography.rs
pub struct TypographyTokens {
    // 字体族
    pub font_family_ui: SharedString,      // "SF Pro Text", "Segoe UI", sans-serif
    pub font_family_mono: SharedString,   // "SF Mono", "Cascadia Code", monospace

    // 字号
    pub font_size_2xs: Pixels, // 10px — 辅助说明
    pub font_size_xs: Pixels,  // 11px — 次要标签
    pub font_size_sm: Pixels,  // 12px — 表格内容
    pub font_size_md: Pixels,  // 13px — 正文（默认）
    pub font_size_lg: Pixels,  // 14px — 标题
    pub font_size_xl: Pixels,  // 16px — 页面标题
    pub font_size_2xl: Pixels, // 18px — 大标题

    // 行高
    pub line_height_tight: f32,  // 1.2 — 标题
    pub line_height_normal: f32, // 1.5 — 正文
    pub line_height_relaxed: f32,// 1.75 — 长文本

    // 字重
    pub font_weight_normal: u32,  // 400
    pub font_weight_medium: u32,  // 500
    pub font_weight_semibold: u32,// 600
    pub font_weight_bold: u32,    // 700
}
```

### 3.5 圆角 Token

```rust
// radius.rs
#[derive(Clone, Copy)]
pub enum Radius {
    None = 0,
    Sm   = 4,    // 4px — 小按钮、标签
    Md   = 6,    // 6px — 输入框、下拉框（默认）
    Lg   = 8,    // 8px — 卡片、面板
    Xl   = 12,   // 12px — 模态框
    Full = 9999, // 全圆角 — 头像、徽章
}

impl Theme {
    pub fn radius(&self) -> RadiusTokens { RadiusTokens { ... } }
}
```

### 3.6 阴影 Token

```rust
// shadow.rs
pub struct ShadowTokens {
    pub none: (),
    pub sm: ShadowStyle { blur: 4, offset: (0, 1), color: ... },  // 小阴影
    pub md: ShadowStyle { blur: 8, offset: (0, 2), color: ... }, // 中阴影
    pub lg: ShadowStyle { blur: 16, offset: (0, 4), color: ... },// 大阴影
    pub xl: ShadowStyle { blur: 24, offset: (0, 8), color: ... },// 浮层阴影
}
```

### 3.7 动效 Token

```rust
// motion.rs
pub struct MotionTokens {
    pub duration_instant: Duration,  // 50ms
    pub duration_fast: Duration,     // 150ms — 悬停反馈
    pub duration_normal: Duration,   // 250ms — 展开/收起
    pub duration_slow: Duration,    // 400ms — 页面过渡

    pub easing_default: Easing,     // cubic-bezier(0.4, 0, 0.2, 1)
    pub easing_decelerate: Easing,   // cubic-bezier(0, 0, 0.2, 1)
    pub easing_accelerate: Easing,   // cubic-bezier(0.4, 0, 1, 1)
    pub easing_bounce: Easing,       // 自定义弹性曲线
}
```

---

## 4. 语义层设计

### 4.1 设计原则

语义层的核心理念：**组件不直接使用原始 token，而是使用表达意图的名称**。

```
组件代码: bg(theme().semantic.glass_surface)
而非:     bg(Hsla::new(0.0, 0.0, 0.18, 0.84))
```

### 4.2 区域语义

| 区域名称 | 用途 | 对应 Token |
|---------|------|----------|
| `glass_base` | 窗口最底层背景 | glass_base |
| `glass_elevated` | 浮起的面板（如 Popover） | glass_elevated |
| `glass_content` | 主要内容区 | glass_content |
| `glass_surface` | 可交互的卡片/面板 | glass_surface |
| `glass_sidebar` | 侧边栏背景 | glass_sidebar |

### 4.3 组件语义颜色命名规范

| 场景 | Token 命名模式 | 示例 |
|------|--------------|------|
| 背景 | `{area}_bg` | `toolbar_bg`, `header_bg` |
| 边框 | `{area}_border` | `card_border`, `input_border` |
| 文字 | `{state}_text` | `primary_text`, `muted_text` |
| 图标 | `{state}_icon` | `default_icon`, `muted_icon` |
| 交互 | `{action}_{state}` | `button_hover`, `button_active` |

---

## 5. 组件规范

### 5.1 基础组件模板

#### 5.1.1 页面布局模板

```rust
// 所有核心页面统一使用以下布局结构：
v_flex()
    .size_full()
    .bg(theme().semantic.glass_base)
    .child(TitleBar)          // 标题栏（可选）
    .child(
        h_flex()
            .size_full()
            .child(Sidebar)          // 侧边栏（可选）
            .child(MainContent)       // 主内容区
    )
```

#### 5.1.2 内容区块模板

```rust
// 内容区块标准结构：
div()
    .w_full()
    .p(Spacing::Unit4.px())
    .bg(theme().semantic.glass_surface)
    .rounded(Radius::Lg)
    .border_1()
    .border_color(theme().semantic.border_subtle)
```

#### 5.1.3 工具栏模板

```rust
// 工具栏标准结构：
div()
    .h(px(36.0))                        // 固定高度
    .px(Spacing::Unit2.px())            // 8px 水平内边距
    .bg(theme().semantic.toolbar_bg)    // 工具栏背景
    .border_b_1()
    .border_color(theme().semantic.border_subtle)
    .flex_row()
    .items_center()
    .gap(Spacing::Unit1.px())           // 4px 按钮间距
```

#### 5.1.4 列表项模板

```rust
// 列表项标准结构：
div()
    .px(Spacing::Unit3.px())            // 12px
    .py(Spacing::Unit2.px())            // 8px
    .w_full()
    .cursor_pointer()
    .rounded(Radius::Md)
    .transition()
    // 状态样式通过 cx.theme() 动态计算
```

### 5.2 组件开发规范

1. **所有间距使用 Spacing Token**
2. **所有颜色使用 Semantic Token**
3. **圆角统一使用 Radius Token**
4. **阴影使用 Shadow Token**
5. **禁止在组件内硬编码像素值**

---

## 6. 布局规范

### 6.1 统一侧边栏宽度

| 侧边栏类型 | 统一宽度 | 最小宽度 | 最大宽度 |
|-----------|---------|---------|---------|
| Home 侧边栏 | 255px | 200px | 400px |
| 数据库对象树 | 250px | 150px | 500px |
| 聊天侧边栏 | 360px | 320px | 600px |

> 注：不同功能区的侧边栏宽度可以不同，但同类型侧边栏必须统一宽度。

### 6.2 统一面板间距

```rust
pub const PANEL_GAP: Pixels = px(1.0);      // 面板间隙
pub const PANEL_MIN_SIZE: Pixels = px(100.0); // 面板最小尺寸
```

### 6.3 统一标题栏高度

```rust
pub const TITLE_BAR_HEIGHT: Pixels = px(38.0);
```

---

## 7. 实施计划

### 7.1 阶段一：设计 Token 落地（第 1-2 周）

1. 创建 `crates/ui/src/tokens/` 目录结构
2. 实现所有 Token 定义（color/spacing/typography/radius/shadow/motion）
3. 建立语义层映射（旧 token → 新 token）
4. 创建 Token 迁移辅助函数（向后兼容）
5. 单元测试覆盖

**交付物：**
- `crates/ui/src/tokens/` 模块
- `crates/ui/src/theme/semantic/` 模块

**关键文件：**
- `crates/ui/src/tokens/mod.rs`
- `crates/ui/src/tokens/color/primitives.rs`
- `crates/ui/src/tokens/color/semantic.rs`
- `crates/ui/src/tokens/spacing.rs`
- `crates/ui/src/tokens/typography.rs`
- `crates/ui/src/tokens/radius.rs`
- `crates/ui/src/tokens/shadow.rs`
- `crates/ui/src/tokens/motion.rs`

### 7.2 阶段二：统一现有不一致（第 3-4 周）

1. 统一侧边栏宽度常量
2. 统一工具栏高度
3. 统一间距使用（替换硬编码值）
4. 统一圆角使用（替换硬编码值）
5. 统一字体大小使用

**交付物：**
- `crates/core/src/layout.rs` 统一常量
- 组件库中所有硬编码值的替换

**关键文件：**
- `crates/core/src/layout.rs`
- `crates/ui/src/` 各组件

### 7.3 阶段三：文档与规范（第 5 周）

1. 编写设计规范文档
2. 更新 README/开发文档
3. 添加组件开发指南

**交付物：**
- `docs/design-system.md` 完整设计规范文档
- `docs/component-guide.md` 组件开发指南

### 7.4 阶段四：核心页面应用（第 6-8 周）

1. Home 页面应用新设计系统
2. Settings 页面应用新设计系统
3. 数据库视图应用新设计系统
4. 连接表单应用新设计系统

**交付物：**
- `main/src/home/` 新设计系统适配
- `main/src/settings/` 新设计系统适配
- `crates/db_view/` 新设计系统适配

---

## 8. 难点与风险

### 8.1 难点

| 难点 | 描述 | 应对策略 |
|------|------|---------|
| **向后兼容** | 现有代码广泛使用 `cx.theme().colors.xxx`，直接替换会导致大量破坏性改动 | 提供过渡期兼容层，保留旧 token 访问，内部映射到新 token |
| **毛玻璃系统差异** | macOS/Windows/Linux 的 glass 效果实现不同 | Token 层抽象平台差异，语义 token 统一访问，内部根据平台返回不同底层值 |
| **两套主题系统合并** | UI 主题和 Terminal 主题目前独立 | 短期保持独立，长期逐步统一共享 token |
| **组件库历史包袱** | 部分组件使用了不一致的样式值 | 通过 Clippy lint 检测硬编码值，逐步替换 |

### 8.2 风险

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| **范围蔓延** | 核心页面重构时发现更多需要统一的地方 | 严格控制范围，标记范围外工作到后续迭代 |
| **性能影响** | 引入额外的 Token 间接访问层可能影响渲染性能 | Token 访问设计为零成本抽象（编译时内联） |
| **分支冲突** | 其他分支同时修改主题相关代码 | 与其他开发者协调，避免同时修改同一文件 |
| **Token 覆盖不全** | 设计初期可能遗漏某些场景的 token | 先梳理现有 token，再补充缺失，避免过度设计 |

---

## 9. 验证机制

### 9.1 Lint 检测

新增 Clippy lint，禁止组件内硬编码设计值：

```rust
// 在 crates/ui/clippy.toml 中配置
disallowed-methods = [
    { path = "gpui::px", reason = "use Spacing token instead" },
    { path = "Hsla::new", reason = "use Semantic colors instead" },
]
```

### 9.2 自动化测试

- Token 值单元测试（确保 light/dark 模式切换正确）
- 组件样式回归测试（截图对比或 CSS 规则检测）

### 9.3 人工审查

- 新增/修改组件时，Reviewer 检查是否使用 Token 而非硬编码
- 设计规范文档评审

---

## 10. 附录

### 10.1 Token 迁移映射表（完整版）

详见 `crates/ui/src/tokens/migration.rs`

### 10.2 术语表

| 术语 | 定义 |
|------|------|
| Design Token | 设计决策的原子化存储（如颜色值、间距值） |
| Semantic Token | 语义化的 token（如 `glass_surface`），表达用途而非具体值 |
| Primitive Token | 原始 token（如 `#1a1a1a`），不直接被组件使用 |
| Glass Effect | 毛玻璃效果，即半透明 + 模糊的背景 |
| Spacing Token | 间距 token，统一管理内边距、外边距、间距 |

### 10.3 参考资料

- [Material Design 3 Token System](https://m3.material.io/foundations/design-tokens)
- [Spectrum (Adobe Design System)](https://spectrum.adobe.com/design-fundamentals/tokens/)
- [Radix Primitives Design Tokens](https://www.radix-ui.com/docs/primitives/docs/theming)

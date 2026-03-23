## 项目上下文摘要（titlebar-double-click）
生成时间：2026-03-23 13:31:00 +0800

### 1. 相似实现分析
- **实现1**: `crates/ui/src/title_bar.rs:252-299`
  - 模式：通用标题栏组件在 macOS 上直接把双击委托给 `window.titlebar_double_click()`
  - 可复用：标题栏双击绑定点、拖动链路、`WindowControlArea::Drag`
  - 需注意：这里是主标题栏入口，修复后会影响所有使用 `TitleBar::new()` 的窗口

- **实现2**: `crates/core/src/tab_container.rs:1540-1571`
  - 模式：Tab 容器顶部条单独复刻了一套标题栏双击和拖动逻辑
  - 可复用：双击绑定点和拖动行为与 `TitleBar` 基本一致
  - 需注意：若只修 `TitleBar`，主工作区顶部条仍然会失效

- **实现3**: `/Users/hoping/.cargo/git/checkouts/zed-a70e2ad075855582/8b5328c/crates/gpui/src/platform/mac/window.rs:1564-1600`
  - 模式：`gpui` 的 macOS 平台实现通过 `AppleActionOnDoubleClick` 决定双击动作
  - 可复用：动作语义映射（None/Minimize/Maximize/Fill）
  - 需注意：当前实现未兼容本机实际存在的 `AppleMiniaturizeOnDoubleClick`

### 2. 项目约定
- **命名约定**: 扩展行为优先放入 `WindowExt`，供多个 UI 组件复用
- **文件组织**: 通用窗口辅助逻辑位于 `crates/ui/src/window_ext.rs`
- **代码风格**: 以最小改动修复问题，避免复制平台判断逻辑到多个组件
- **导入方式**: 组件侧通常通过 `WindowExt as _` 或直接引入 trait 方法

### 3. 可复用组件清单
- `crates/ui/src/window_ext.rs`: 窗口扩展入口，适合放标题栏双击兼容逻辑
- `crates/ui/src/title_bar.rs`: 通用标题栏组件
- `crates/core/src/tab_container.rs`: 主工作区顶部条
- `gpui::Window::zoom_window` / `gpui::Window::minimize_window`: 现成平台动作

### 4. 测试策略
- **测试框架**: Rust 内置单元测试 + `cargo check`
- **测试模式**:
  - 对偏好解析逻辑做纯单元测试
  - 对整体项目做编译校验，确保调用链不破坏
- **参考范围**: `crates/ui` 现有 `#[cfg(test)]` 模式

### 5. 依赖和集成点
- **外部依赖**: macOS `defaults` 命令读取 `NSGlobalDomain`
- **内部依赖**: `TitleBar` 与 `TabContainer` 均调用 `WindowExt`
- **集成方式**: 在 `WindowExt` 内统一解析系统偏好，组件只调用一个入口

### 6. 技术选型理由
- **为什么用这个方案**: 不改上游 `gpui` 依赖即可在本仓库内修复；同时覆盖两个标题栏入口
- **优势**: 改动面小；不影响 Linux/Windows；与现有窗口动作 API 保持一致
- **风险**: 读取 `defaults` 会在双击时启动一次系统命令，但触发频率极低，可接受

### 7. 关键风险点
- **系统兼容**: 新旧 macOS 可能分别使用 `AppleActionOnDoubleClick` 与 `AppleMiniaturizeOnDoubleClick`
- **行为边界**: `Fill` 当前按 `Zoom` 处理，与 `gpui` 现有策略保持一致
- **工具说明**: 当前会话没有 `desktop-commander`、`context7`、`github.search_code`，本次使用本地源码检索和系统命令完成分析与验证

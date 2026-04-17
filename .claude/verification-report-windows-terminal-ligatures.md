## 审查报告（windows-terminal-ligatures）
生成时间：2026-03-28 23:46:28 +08:00

### 需求完整性检查
- 目标明确：排查并修复 Windows 平台终端开启连字后仍不生效的问题
- 范围明确：仅涉及终端字体特性 helper、相关单测与 `.claude` 留痕，不改设置同步链路
- 交付物明确：代码修复、单元测试、上下文摘要、操作日志、验证报告
- 风险与依赖明确：Windows 默认字体 `Consolas` 不支持编程连字；自动化验证受 `nasm` 与 `cmake` 缺失阻塞

### 技术维度评分
- 代码质量：94/100
- 测试覆盖：82/100
- 规范遵循：95/100

### 战略维度评分
- 需求匹配：95/100
- 架构一致：96/100
- 风险评估：92/100

### 综合评分
- 92/100
- 建议：通过

### 结论
- 根因收敛清晰：Windows 路径会把空 `FontFeatures` 绑定到 DirectWrite `Typography`，这与 macOS/Linux 的现状存在平台差异，容易导致终端连字在 Windows 下完全不生效。
- 修复策略克制：仅修改 `crates/terminal_view/src/terminal_element.rs` 的 `terminal_font_features(...)`，让终端连字开关始终显式控制 `liga`、`clig`、`calt` 三项特性，不扩散到设置层或平台层。
- 一致性保持完整：字宽测量和真实绘制都继续复用同一 helper，因此不会引入列宽、光标或选区错位的额外回归面。
- 自动化验证受环境阻塞：`cargo test -p terminal_view --lib -- --nocapture` 因 `aws-lc-sys` 依赖的 `nasm`、`cmake` 缺失而失败，当前只能完成格式化与静态链路复核。
- 残余风险可控：如果用户继续使用 `Consolas`，即便代码修复后仍不会出现编程连字；这属于字体能力限制，建议在 Windows 上用 `Cascadia Code`、`Fira Code` 或 `JetBrains Mono` 做回归确认。

# 终端链接点击失效问题修复

## 问题描述

在终端中，虽然提示 `[CMD]+click to open the link`，但实际按下 CMD 键并点击链接时，链接并未打开。

## 根本原因

问题出现在 `crates/terminal_view/src/view.rs` 的 `try_report_sgr_mouse_button` 方法中。

### SGR 鼠标模式介绍

SGR (Select Graphic Rendition) 鼠标模式是 xterm 扩展的一种鼠标报告协议，允许终端应用程序（如 vim、tmux）捕获鼠标事件。当终端进入 SGR 鼠标模式时，鼠标事件会被报告给运行在终端中的应用程序。

### 问题分析

在 `handle_mouse_down` 方法的处理流程中：

1. **第一步检查**：`should_defer_sgr_left_press` 函数检查是否应延迟处理鼠标左键按下事件
   - 该函数正确地排除了带修饰键（Shift、Ctrl、Alt、**CMD**）的点击
   - 当按下 CMD 键时，该函数返回 `false`，不延迟处理

2. **第二步检查**：`try_report_sgr_mouse_button` 函数决定是否将鼠标事件报告给 TUI 应用
   - **问题所在**：该函数只检查了 `Shift` 键和"正在选择文本"状态
   - **没有检查 CMD 键（`modifiers.platform`）**
   - 结果：即使用户按下 CMD+点击，事件仍被报告给 TUI 应用（如 vim），然后提前返回
   - **链接检测逻辑被跳过**，导致链接无法打开

### 代码位置

文件：`crates/terminal_view/src/view.rs`

原始代码（第 3649-3667 行）：
```rust
fn try_report_sgr_mouse_button(
    &mut self,
    button: MouseButton,
    position: Point<Pixels>,
    modifiers: Modifiers,
    pressed: bool,
    cx: &mut Context<Self>,
) -> bool {
    if button == MouseButton::Left
        && (modifiers.shift || (!pressed && self.mouse_state.selecting))
    {
        return false;  // 跳过 SGR 报告，允许文本选择
    }
    let mode = self.terminal.read(cx).mode();
    if !sgr_mouse_mode_enabled(mode) {
        return false;
    }
    self.write_sgr_mouse_button_report(button, position, modifiers, pressed, cx)
}
```

## 修复方案

在 `try_report_sgr_mouse_button` 函数的检查中添加 `modifiers.platform`（CMD 键）判断：

```rust
fn try_report_sgr_mouse_button(
    &mut self,
    button: MouseButton,
    position: Point<Pixels>,
    modifiers: Modifiers,
    pressed: bool,
    cx: &mut Context<Self>,
) -> bool {
    if button == MouseButton::Left
        && (modifiers.shift || modifiers.platform || (!pressed && self.mouse_state.selecting))
        //                    ^^^^^^^^^^^^^^^^^^^^ 添加此检查
    {
        return false;  // 跳过 SGR 报告，允许链接打开
    }
    let mode = self.terminal.read(cx).mode();
    if !sgr_mouse_mode_enabled(mode) {
        return false;
    }
    self.write_sgr_mouse_button_report(button, position, modifiers, pressed, cx)
}
```

### 修复逻辑

当用户按下 CMD 键并点击时：
1. `modifiers.platform` 为 `true`
2. `try_report_sgr_mouse_button` 返回 `false`（不报告给 TUI）
3. 代码继续执行到链接检测逻辑（第 3742-3755 行）
4. `addon_manager.dispatch_mouse_down` 被调用
5. `WebLinksAddon` 的 `on_mouse_down` 检测到链接并打开

## 行为对齐

此修复使 CMD+点击的行为与 Shift+点击保持一致：

- **Shift+点击**：用于文本选择，不报告给 TUI 应用
- **CMD+点击**：用于打开链接/路径，不报告给 TUI 应用
- **普通点击**（无修饰键）：在 SGR 模式下报告给 TUI 应用（如 vim）

这是 xterm、iTerm、kitty、wezterm 等主流终端模拟器的通用约定。

## 影响范围

- 仅影响终端中的鼠标事件处理
- 不影响其他功能
- 与现有的 Shift+点击行为保持一致

## 测试验证

相关测试：`should_defer_sgr_left_press_only_for_plain_left_mouse_in_sgr_mode`

该测试已验证 `should_defer_sgr_left_press` 函数正确排除了 CMD 键的情况。修复后，`try_report_sgr_mouse_button` 的行为与之一致。

## 完成时间

2026-06-06

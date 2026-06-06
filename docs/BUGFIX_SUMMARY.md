# Bug 修复：终端链接 CMD+点击失效

## 问题
终端中显示提示 `[CMD]+click to open the link`，但实际 CMD+点击链接时无响应。

## 根本原因
在 SGR 鼠标模式下，`try_report_sgr_mouse_button` 方法未检查 `modifiers.platform`（CMD 键），导致 CMD+点击事件被错误地报告给 TUI 应用（如 vim），链接检测逻辑被跳过。

## 修复内容
**文件**: `crates/terminal_view/src/view.rs`  
**位置**: 第 3657-3660 行

**修改前**:
```rust
if button == MouseButton::Left
    && (modifiers.shift || (!pressed && self.mouse_state.selecting))
{
    return false;
}
```

**修改后**:
```rust
if button == MouseButton::Left
    && (modifiers.shift || modifiers.platform || (!pressed && self.mouse_state.selecting))
{
    return false;
}
```

## 修复原理
添加 `modifiers.platform` 检查后，当用户按下 CMD+点击时：
1. `try_report_sgr_mouse_button` 返回 `false`（不报告给 TUI）
2. 事件继续传递到链接检测逻辑
3. `WebLinksAddon` 成功检测并打开链接

这与 Shift+点击（用于文本选择）的行为保持一致，符合主流终端模拟器的约定。

## 详细分析
参见 `docs/Analysis/TERMINAL_LINK_CLICK_FIX.md`

## 状态
✅ 代码已修复  
✅ 编译通过  
⏳ 等待实际测试验证

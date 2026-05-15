# 终端进程检测设置开关实现计划

**状态**: ✅ 已完成 (完成时间: 2026-05-15)

## 任务目标

在终端设置中添加一个开关，允许用户控制是否在退出终端时检查运行中的进程。

## 问题背景

当前实现了基于超时的 SSH 进程检测机制（30秒超时），但在某些情况下仍可能产生误报。用户需要一个明确的开关来完全禁用进程检测功能。

## 详细任务分解

### 1. ✅ 数据模型层修改
**文件**: `crates/terminal_view/src/settings.rs`

**改动内容**:
- 在 `TerminalSettings` 结构体中添加新字段：
  ```rust
  pub check_running_processes_on_exit: bool,
  ```
- 在 `Default` 实现中设置默认值为 `true`（保持现有行为）
- 确保字段可以正确序列化/反序列化

**预期效果**: 设置数据模型支持新的开关字段

### 2. ✅ 事件系统扩展
**文件**: `crates/terminal_view/src/sidebar/settings_panel.rs`

**改动内容**:
- 在 `SettingsPanelEvent` 枚举中添加新事件：
  ```rust
  CheckRunningProcessesChanged(bool),
  ```

**预期效果**: 事件系统可以传递开关状态变化

### 3. ✅ UI 界面添加
**文件**: `crates/terminal_view/src/sidebar/settings_panel.rs`

**改动内容**:
- 在安全设置部分（safety section，约 913-1028 行）添加新的开关组件
- 开关标签：`退出时检测运行中的进程`
- 开关说明：`关闭后将不再检查终端是否有进程在运行，可能导致意外关闭正在运行的任务`
- 绑定到 `settings.check_running_processes_on_exit`
- 状态变化时发出 `CheckRunningProcessesChanged` 事件

**预期效果**: 用户可以在设置界面看到并操作此开关

### 4. ✅ 事件处理逻辑
**文件**: `crates/terminal_view/src/sidebar/settings_panel.rs`

**改动内容**:
- 在 `SettingsPanel` 的事件处理逻辑中添加对 `CheckRunningProcessesChanged` 的处理
- 调用 `update_settings` 更新设置值

**预期效果**: 开关操作能正确更新设置

### 5. ✅ Terminal 逻辑集成
**文件**: `crates/terminal/src/terminal.rs`

**改动内容**:
- 修改 `has_running_processes()` 方法，在检查进程前先读取设置
- 如果 `check_running_processes_on_exit` 为 `false`，直接返回 `false`（不检查）
- 需要通过某种方式将设置传递给 Terminal（可能需要在 TerminalView 层传递）

**预期效果**: 当开关关闭时，终端退出不再检查进程

### 6. ✅ 测试验证
**测试内容**:
- 编译通过
- 打开设置面板，确认新开关显示正常
- 开关默认为开启状态
- 关闭开关后，终端退出时不再检查进程
- 开启开关后，终端退出时恢复检查进程
- 设置持久化正常（重启应用后设置保持）

**预期效果**: 功能完整可用，无回归问题

## 技术细节

### 设置传递方案
由于 `Terminal` 位于 `crates/terminal`，而 `TerminalSettings` 位于 `crates/terminal_view`，需要考虑如何传递设置：

**方案 A**: 在 `has_running_processes()` 调用时传递参数
- 优点：解耦清晰
- 缺点：需要修改调用链

**方案 B**: 在 Terminal 结构体中添加字段存储此设置
- 优点：调用简单
- 缺点：需要在设置变化时同步更新

**推荐方案**: 方案 A，在调用 `has_running_processes()` 时传递设置值

## 风险评估

1. **低风险**: 数据模型和事件系统的修改是标准的扩展操作
2. **中风险**: Terminal 逻辑集成需要确保不影响现有的超时机制
3. **低风险**: UI 添加遵循现有模式，风险可控

## 实施顺序

按照自底向上的顺序实施：
1. 数据模型层（settings.rs）
2. 事件系统（settings_panel.rs 事件枚举）
3. UI 界面（settings_panel.rs UI 部分）
4. 事件处理（settings_panel.rs 处理逻辑）
5. Terminal 集成（terminal.rs）
6. 测试验证

## 验收标准

- [ ] 编译无警告
- [ ] 设置界面显示新开关
- [ ] 开关默认为开启状态
- [ ] 关闭开关后终端退出不检查进程
- [ ] 开启开关后终端退出检查进程
- [ ] 设置持久化正常
- [ ] 无回归问题

## 相关文件

- `crates/terminal_view/src/settings.rs`
- `crates/terminal_view/src/sidebar/settings_panel.rs`
- `crates/terminal/src/terminal.rs`
- `docs/Analysis/TERMINAL_CLOSE_PROCESS_CHECK.md`（背景分析文档）

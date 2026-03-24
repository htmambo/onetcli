## 项目上下文摘要（auth-error-dialog-auto-close）
生成时间：2026-03-24 21:07:51 +0800

### 1. 相似实现分析
- 实现1：`main/src/home_tab.rs:3038`
  - 模式：在 `render` 阶段消费一次性状态 `self.auth_error.take()`，再用 `window.defer` 延迟打开错误弹窗
  - 可复用：`window.defer` 作为“避免在 render 中直接改窗口状态”的既有模式
  - 风险点：错误弹窗确认回调里又立即打开新弹窗，可能与当前弹窗关闭时序冲突
- 实现2：`main/src/auth.rs:502`
  - 模式：登录弹窗通过 `show_auth_dialog` 统一构建，提交成功后由 `HomePage::verify_otp` 接管后续状态
  - 可复用：认证失败后仍复用现有 `show_login_dialog` 入口，不新增旁路弹窗
  - 风险点：登录弹窗本身使用 `window.open_dialog`，如果在错误弹窗尚未关闭时重开，会形成嵌套栈
- 实现3：`crates/ui/src/dialog.rs:323`
  - 模式：确认按钮点击时先执行 `on_ok`，再调用 `window.close_dialog(cx)`
  - 可复用：按钮关闭行为和返回 `bool` 的校验约定
  - 风险点：`on_ok` 若先打开新弹窗，`close_dialog` 会关闭当前栈顶而不是原错误弹窗

### 2. 项目约定
- 命名约定：Rust 使用 `snake_case`，类型和组件入口使用 `PascalCase`
- 文件组织：页面状态集中在 `main/src/home_tab.rs`，认证弹窗构造集中在 `main/src/auth.rs`
- 窗口状态修改：渲染期间通过 `window.defer` 延迟执行
- 错误处理：异步认证失败写入 `self.auth_error`，由 `render` 消费并提示

### 3. 可复用组件清单
- `main/src/home_tab.rs::show_login_dialog`：统一选择 OTP/密码认证弹窗
- `main/src/auth.rs::show_auth_dialog`：OTP 登录弹窗构造
- `crates/ui/src/dialog.rs::Dialog::on_ok`：确认按钮关闭协议

### 4. 测试策略
- 测试框架：Rust `cargo test`
- 当前可用验证：针对 `main` 包执行编译级验证，确保闭包捕获与窗口 API 调用无回归
- 回归重点：错误弹窗点击“确定”后应关闭当前错误弹窗，并重新打开登录弹窗

### 5. 依赖和集成点
- 触发入口：`HomePage::verify_otp` / `HomePage::authenticate_with_password`
- 中间状态：`self.auth_error`
- UI 集成点：`HomePage::render` 中的错误弹窗和 `show_login_dialog`
- 底层行为：`Window::open_dialog` / `Window::close_dialog` 的对话框栈管理

### 6. 技术选型理由
- 选择最小修复：把“重新打开登录弹窗”延迟到当前确认事件完成之后
- 优势：不改通用对话框框架，不扩大影响面，直接修正当前失败路径
- 风险：仓库当前缺少直接覆盖该 UI 时序的自动化测试，需要先用编译和行为推理兜底

### 7. 关键风险点
- 时序问题：在 `on_ok` 内立即打开新弹窗会抢占栈顶
- 边界条件：实体被销毁时 `view.update` 需要允许安全失败
- 测试缺口：暂未找到覆盖此类弹窗栈时序的现成自动化用例

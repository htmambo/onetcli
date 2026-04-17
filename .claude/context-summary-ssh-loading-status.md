## 项目上下文摘要（SSH 连接加载状态展示）
生成时间：2026-03-27 10:30:05 +0800

### 1. 相似实现分析
- **实现1**: `crates/terminal/src/terminal.rs:301`
  - 模式：`Terminal::new_ssh` 创建异步连接任务，状态仅用 `ConnectionState::Connecting` / `Connected` / `Disconnected` 表达。
  - 可复用：`spawn_ssh_connect`、`handle_ssh_result`、`TerminalModelEvent::Wakeup`。
  - 需注意：现有状态粒度太粗，UI 只能知道“正在连接”，不知道具体阶段。

- **实现2**: `crates/terminal_view/src/view.rs:1785`
  - 模式：SSH 会话遮罩通过 `connection_state()` 决定图标、标题和副标题。
  - 可复用：现成的遮罩容器、重连按钮和图标切换逻辑。
  - 需注意：副标题固定为 `SshSession.establishing`，适合替换为更细粒度的动态状态文本。

- **实现3**: `crates/terminal_view/src/ssh_form_window.rs:858`
  - 模式：测试连接时设置 `is_testing = true`，异步执行 `RusshClient::connect`，完成后回填 `test_result`。
  - 可复用：`cx.spawn` + `Tokio::spawn_result` 的异步结果回流方式。
  - 需注意：测试期间目前只有按钮文案变化，没有阶段进度提示。

- **实现4**: `main/src/update.rs:268`
  - 模式：界面层保存 `status_message`，异步任务通过回调持续更新 UI 文案。
  - 可复用：将“连接阶段”映射为可渲染文本并持续刷新。
  - 需注意：状态文本与最终错误信息要分开存储，避免互相覆盖。

### 2. 项目约定
- **命名约定**: Rust 类型使用 `PascalCase`，函数与字段使用 `snake_case`，状态枚举用 `Enum::Variant`。
- **文件组织**: 底层连接逻辑放在 `crates/ssh`、终端状态机在 `crates/terminal`、视图渲染在 `crates/terminal_view`。
- **导入顺序**: 先标准库，再第三方库，再工作区 crate；同组内大致按字母和语义聚合。
- **代码风格**: 中文注释为主，优先小函数和显式状态字段，不通过魔法字符串隐式表达状态。

### 3. 可复用组件清单
- `crates/terminal/src/terminal.rs`: `spawn_ssh_connect`、`handle_ssh_result`、`connection_state()`
- `crates/terminal_view/src/view.rs`: `render_connection_overlay`
- `crates/terminal_view/src/ssh_form_window.rs`: `on_test`、测试结果展示区域
- `main/src/update.rs`: `status_message` 持续刷新模式

### 4. 测试策略
- **测试框架**: `cargo test`
- **测试模式**: 现有以 crate 内单元测试为主
- **参考文件**:
  - `crates/terminal/src/terminal.rs:994`
  - `crates/ssh/src/ssh.rs:449`
- **覆盖要求**:
  - 连接阶段状态文案的顺序和回退逻辑
  - 无跳板机 / 有跳板机 / 有代理的分支映射
  - 现有 SSH 初始化命令相关测试不回归

### 5. 依赖和集成点
- **外部依赖**: `russh`、`tokio`、`gpui`
- **内部依赖**:
  - `terminal` 依赖 `ssh`
  - `terminal_view` 依赖 `terminal` 和 `ssh`
- **集成方式**: 底层异步任务通过 `cx.spawn` / `Tokio::spawn` 回流到 GPUI 线程更新状态
- **配置来源**: `StoredConnection::to_ssh_params()` 生成 `SshConnectConfig`

### 6. 技术选型理由
- **为什么用这个方案**: 在 `crates/ssh` 统一产出连接阶段，能同时服务终端会话和测试连接，避免在 UI 层猜测底层进度。
- **优势**: 改动集中、复用现有异步回流模式、不会破坏现有连接接口的职责边界。
- **劣势和风险**: 需要跨 crate 增加状态类型与回调，若处理不当会引入额外的线程间同步复杂度。

### 7. 关键风险点
- **并发问题**: 连接阶段更新来自 Tokio 线程，必须通过现有 UI 更新通道回流，不能直接跨线程改 GPUI 状态。
- **边界条件**: 连接失败很快返回时，测试状态和错误信息不能相互覆盖；重连时要重置上一轮状态。
- **性能瓶颈**: 状态事件频率很低，主要是阶段切换，不应引入额外性能压力。
- **安全考虑**: 本次只展示连接阶段，不输出密码、私钥路径或其它敏感认证内容。

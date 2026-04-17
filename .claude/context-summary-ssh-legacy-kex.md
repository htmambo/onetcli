## 项目上下文摘要（SSH 旧版 KEX 兼容开关）
生成时间：2026-03-27 11:46:19 +0800

### 1. 相似实现分析
- **实现1**: `crates/ssh/src/ssh.rs:233`
  - 模式：`RusshClient::connect_with_progress(...)` 在进入连接前构造统一的 `russh::client::Config`，随后被直连、代理、跳板机三条链路复用。
  - 可复用：统一配置入口最适合承载旧版 KEX 开关，避免分别在多条连接路径硬编码算法列表。
  - 需注意：一个连接配置同时影响目标机和跳板机握手，字段命名必须表达“整条 SSH 链路”的含义。

- **实现2**: `crates/terminal_view/src/ssh_form_window.rs:317`
  - 模式：SSH 表单使用布尔状态字段保存开关，并在 `build_ssh_params()` / `build_ssh_connect_config()` 中把 UI 状态落到持久化参数与运行时参数。
  - 可复用：`enable_jump_server`、`enable_proxy`、`sync_enabled` 这类 checkbox + 持久化字段的模式，可直接复用于旧版 KEX 兼容开关。
  - 需注意：测试连接与保存连接必须复用同一份参数构造逻辑，否则会出现“测试能连但实际连接不生效”的分叉。

- **实现3**: `crates/terminal/src/terminal.rs:345`、`crates/terminal_view/src/sidebar/file_manager_panel.rs:375`、`crates/sftp_view/src/lib.rs:479`
  - 模式：仓库里存在多处 `SshParams -> SshConnectConfig` 的显式转换，终端、侧边文件管理器和 SFTP 视图各自组装运行时 SSH 参数。
  - 可复用：这些转换点都是兼容开关必须覆盖的集成点。
  - 需注意：只改终端入口会导致“终端能连、SFTP 仍失败”的行为不一致。

- **实现4**: `crates/sftp/src/russh_impl.rs:594`
  - 模式：SFTP 连接使用与终端不同的 `russh::client::Config` 构造逻辑，并额外设置 `window_size`、`maximum_packet_size`、`nodelay`。
  - 可复用：可以复用 SSH crate 统一产出的基础配置，再叠加 SFTP 自己的吞吐相关参数。
  - 需注意：不能把 SFTP 的专属窗口参数回写到终端侧，否则会扩大变更范围。

### 2. 项目约定
- **命名约定**: 连接配置布尔字段使用 `enable_xxx`，UI 状态和存储字段尽量保持同名，减少转换歧义。
- **文件组织**: 持久化参数在 `crates/core/src/storage/models.rs`，运行时 SSH 建连在 `crates/ssh` / `crates/sftp`，UI 表单在 `crates/terminal_view`。
- **导入顺序**: 标准库、第三方 crate、工作区 crate 分组保持不变，不为单个字段改动打散既有结构。
- **代码风格**: 优先扩展现有配置对象与表单，不新增第二套“兼容模式”对象。

### 3. 可复用组件清单
- `crates/ssh/src/ssh.rs`: 统一的 `SshConnectConfig`、`RusshClient::connect_with_progress(...)`
- `crates/terminal_view/src/ssh_form_window.rs`: 布尔开关 UI 模式、测试连接与保存连接共用的参数构造
- `crates/terminal/src/terminal.rs`: 终端入口的 `SshParams -> SshConnectConfig` 转换
- `crates/sftp/src/russh_impl.rs`: SFTP 专属的窗口参数配置

### 4. 测试策略
- **测试框架**: `cargo test`
- **测试模式**:
  - `ssh` crate 单元测试验证 KEX 列表构造
  - `one-core` 定向测试验证存储字段默认值与仓库更新路径
  - `terminal` / `terminal_view` / `sftp` / `sftp_view` / `db` 编译测试验证链路未断
- **参考文件**:
  - `crates/ssh/src/ssh.rs:686`
  - `crates/core/src/storage/models.rs:1707`
  - `crates/core/src/storage/repository.rs:1528`

### 5. 依赖和集成点
- **外部依赖**: `russh 0.57.0`
- **内部依赖**:
  - `terminal` / `sftp_view` / `terminal_view` 依赖 `ssh`
  - `sftp` 依赖 `ssh` 并追加 SFTP 特有配置
  - `one-core` 保存 SSH 参数，供终端和文件管理入口反序列化复用
- **集成方式**: UI checkbox -> `SshParams.enable_legacy_kex` -> `SshConnectConfig.enable_legacy_kex` -> `russh::client::Config.preferred.kex`

### 6. 技术选型理由
- **为什么用这个方案**: 目标是“按连接开启兼容模式”，最小成本方案就是把布尔字段贯穿现有配置链路，并仅在底层构造 `russh` 首选算法列表时生效。
- **优势**: 默认连接行为不变；开启后仅追加旧算法，不抢占现代算法优先级；终端和 SFTP 行为保持一致。
- **劣势和风险**: 旧算法本身较弱，只能作为定向兼容开关，不适合默认全局开启。

### 7. 关键风险点
- **并发问题**: 无新增并发模型，风险主要在多入口配置遗漏。
- **边界条件**: 旧连接记录缺少新字段时必须默认 `false`，否则旧数据反序列化会失败。
- **性能瓶颈**: 仅多一次小型算法列表构造，可忽略。
- **安全考虑**: 兼容模式默认关闭，且旧算法追加在列表末尾，仅用于必要时回退。

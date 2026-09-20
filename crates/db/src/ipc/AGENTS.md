# IPC 外部驱动子系统

本目录实现数据库外部驱动的宿主侧：驱动是独立子进程，与宿主经本地 socket
（Unix domain socket / Windows 命名管道，由 `interprocess` crate 抽象）
跑 JSON-RPC。

> 注意：产品层面已决定收敛到内置 MySQL/PostgreSQL/SQLite（见
> `docs/plans/2026-09-19-remove-external-db-drivers.md`，分支
> `refactor/remove-external-db-drivers`）。本目录随该分支合入后删除；
> 在此之前的安全规则仍需遵守。

## 模块分工

- `registry.rs`：manifest（`driver.json`）加载、校验、注册表
- `client.rs`：驱动子进程生命周期（spawn / kill_on_drop）+ socket 客户端
- `protocol.rs`：JSON-RPC 请求的 params 构造（连接参数在此序列化）
- `connection.rs` / `plugin.rs`：`ExternalDbConnection` / `ExternalDatabasePlugin` 对接 db 抽象
- `display.rs`：驱动在 UI 层的展示信息

## 握手流程

1. `IpcDriverRegistry::load_default()` 扫 `~/.config/omnihub/ipc-drivers/`，
   `load_manifest` 解析 `driver.json` 并调 `IpcDriverManifest::validate()`
   （id/name/command/socket name 非空 + B4 env 敏感字段门禁）。
2. 连接时 `client.rs` 生成一次性 socket 名，经环境变量
   `OMNIHUB_IPC_SOCKET`（`SOCKET_ENV_VAR`）透传给子进程——
   **这是唯一允许注入的 env**，其余 env 须由 manifest `env_from_config`
   显式声明且经 B4 校验。
3. 子进程启动后监听该 socket；宿主 connect 后发 JSON-RPC 请求
   （`IpcRequest`，length-delimited framing 见 `ipc` crate）。
4. 连接参数（含 `password`）经 socket 协议载荷传输
   （`protocol.rs::connection_config_params`）。

## 安全规则（B4，硬门禁）

- **密码 / 私钥等敏感字段只允许走 socket 协议载荷，禁止经环境变量注入子进程。**
  env 会被 `/proc/<pid>/environ` 暴露给同用户其他进程，并遗传给孙进程。
- 两道防线：
  1. `registry.rs::validate()` 拒绝 `env_from_config` 映射敏感路径的 manifest
     （加载即失败）；
  2. `client.rs::env_pairs_from_connection_config` 防御纵深跳过 + warn
     （兜住 `from_drivers` 等绕过校验的构造路径）。
- 敏感判定复用 `one_core::storage::models::is_sensitive_field`，与落库加密
  名单同一来源；新增敏感字段时无需在本目录重复维护名单。
- `env_from_config` 的合法用途：非敏感运行时参数（如 `extra_params.jdk_home`）。

## 验证

- `cargo test -p db --lib ipc`（81 个测试）
- B4 专项：`cargo test -p db --lib env_from_config`（5 个测试）

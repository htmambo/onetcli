## 项目上下文摘要（delete-license-module）
生成时间：2026-03-24 16:04:47 +0800

### 1. 相似实现分析
- **实现1**: `main/src/main.rs:1`
  - 模式：主程序通过 `mod` 声明加载顶层模块，再由 `onetcli_app::init` 串起各模块初始化
  - 可复用：模块移除时应同时删除 `mod license;`
  - 需注意：只删文件不删 `mod` 声明会直接导致编译失败

- **实现2**: `main/src/onetcli_app.rs:140`
  - 模式：应用启动时按顺序初始化 `one_core`、UI、认证和其他全局服务
  - 可复用：删除运行时模块时，要同步清理启动接入点
  - 需注意：当前账号登录依赖 `crate::auth::init(cx)`，必须保留；`crate::license::init(cx)` 可删除

- **实现3**: `crates/core/src/cloud_sync/client.rs:1`
  - 模式：云同步能力统一通过 `CloudApiClient` trait 暴露，`SupabaseClient` 提供具体实现
  - 可复用：删除授权逻辑时要同步收缩 trait，避免保留无用订阅接口
  - 需注意：`get_subscription()` 和 `SubscriptionInfo` 只剩云同步实现内部依赖，删掉不会影响登录和同步

- **实现4**: `crates/core/src/lib.rs:1`
  - 模式：`one-core` 通过 `pub mod` 控制公共模块出口
  - 可复用：删除 `license` 目录后必须同步删掉 `pub mod license;`
  - 需注意：否则外部 crate 仍会尝试导出已删除模块

- **实现5**: `Cargo.toml:1`
  - 模式：工作区成员通过根 `Cargo.toml` 统一声明
  - 可复用：删除 `crates/license_tool` 这类独立工具时，要先从 workspace members 中摘除
  - 需注意：`license_tool` 依赖 `one_core::license`，如果不一起处理，工作区会残留失效成员

### 2. 项目约定
- **命名约定**: 运行时模块在 `main/src/*`，公共核心模块在 `crates/core/src/*`，独立工具通过工作区成员管理
- **文件组织**: 初始化链路集中在 `main/src/main.rs` 和 `main/src/onetcli_app.rs`
- **代码风格**: 采用删除式变更，直接清除无用接口和工作区成员，不做向后兼容包装

### 3. 可复用组件清单
- `crate::auth::init(cx)`：保留账号登录初始化
- `CloudApiClient` / `SupabaseClient`：保留登录和云同步实现，但去掉订阅接口
- `one_core::init(cx)`：保留核心模块初始化，不引入新入口

### 4. 测试策略
- `cargo check -p one-core`
- `cargo check -p main`
- `cargo test -p one-core cloud_sync:: --lib`
- `rg` 检索残留引用：`one_core::license`、`crate::license`、`get_subscription()`、`license_tool`

### 5. 依赖和集成点
- `main` 依赖 `one-core`，删除公共模块需同时处理两个 crate
- `crates/license_tool` 依赖 `one_core::license`，必须与核心模块一起移除
- `auth` 与 `cloud_sync` 依赖关系独立于 License，可保留

### 6. 技术选型理由
- **为什么直接删除 `license` 目录而不是继续保留兼容层**: 用户已明确要求彻底删除授权模块，不再保留名义接口
- **为什么同时删 `get_subscription()`**: 该接口只为 License 服务，业务侧已不再使用
- **为什么删 `license_tool`**: 它是离线 License 工具，已不符合“只保留账号登录”的目标

### 7. 关键风险点
- **模块出口风险**: `mod license;` 和 `pub mod license;` 任何一处残留都会打断构建
- **工作区风险**: `license_tool` 若不移除，会因 `one_core::license` 被删除而失效
- **接口收缩风险**: `CloudApiClient` 和 `SupabaseClient` 必须同时删 `get_subscription()`，否则 trait/impl 会不匹配

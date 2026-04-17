## 项目上下文摘要（certificate-management）
生成时间：2026-03-25 13:05:00 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/storage/models.rs` + `crates/core/src/storage/repository.rs`
  - 模式：本地实体统一走 `models.rs` 定义、`Repository` 仓储实现、`migration` 建表和 `storage::init` 注册。
  - 可复用：证书实体可以直接沿用 `Workspace` / `StoredConnection` 的字段组织、时间戳和同步字段约定。
  - 需注意：连接参数里的敏感字段依赖 `password/passphrase` 命名自动加解密，新增证书表则要自行在仓储层处理敏感列。

- **实现2**: `crates/core/src/cloud_sync/workspace_sync.rs` + `crates/core/src/cloud_sync/generic_sync.rs`
  - 模式：简单同步类型通过实现 `SyncTypeHandler` 接入 `generic_sync`，不需要复制 `connection_sync.rs` 那套定制流程。
  - 可复用：证书是独立数据实体，最适合复用 `workspace` 的同步模式。
  - 需注意：`PendingCloudDeletionRepository` 当前默认只区分 `connection/workspace`，需要补成可按 `entity_type` 泛化查询。

- **实现3**: `crates/db_view/src/common/db_connection_form.rs`、`crates/terminal_view/src/ssh_form_window.rs`、`crates/redis_view/src/redis_form_window.rs`、`crates/mongodb_view/src/mongo_form_window.rs`
  - 模式：数据库表单走动态字段，SSH/Redis/Mongo 表单走显式输入框；保存点最终都落到 `StoredConnection.params`。
  - 可复用：证书选择应在表单层保存引用信息，同时把当前证书内容展开为连接参数快照。
  - 需注意：若只保存纯本地 ID，会在跨设备同步时丢失关联，因此需要同时保存本地 ID 和云端 ID 引用信息。

- **实现4**: `main/src/home_tab.rs` + `crates/core/src/popup_window.rs`
  - 模式：桌面端独立管理能力通常通过 `open_popup_window` 打开弹窗窗口。
  - 可复用：证书管理可以作为独立弹窗，由主页入口和各连接表单共同打开。
  - 需注意：连接表单在证书弹窗保存后需要刷新可选列表，适合补一个轻量级全局通知器。

### 2. 项目约定
- **命名约定**: Rust 类型使用语义化名词，数据库枚举字段走字符串存储并提供 `from_str` / `Display`
- **文件组织**: 数据模型在 `crates/core/src/storage`，同步类型在 `crates/core/src/cloud_sync`，桌面窗口在各视图 crate 与 `main/src/home_tab.rs`
- **导入顺序**: 先标准库 / 第三方，再 `one_core` / 本地模块
- **代码风格**: 倾向增量扩展既有仓储、同步桥接和 popup 模式，不新造第二套状态管理

### 3. 可复用组件清单
- `crates/core/src/storage/repository.rs::ConnectionRepository`
- `crates/core/src/cloud_sync/workspace_sync.rs::WorkspaceSyncType`
- `crates/core/src/cloud_sync/service.rs::prepare_workspace_sync_data_upload`
- `crates/core/src/popup_window.rs::open_popup_window`
- `crates/core/src/connection_notifier.rs::emit_connection_event`

### 4. 测试策略
- **测试框架**: Rust `cargo check` / `cargo test`
- **测试模式**: 以编译验证和现有单元测试为主，必要时补充核心模型测试
- **参考命令**:
  - `cargo fmt --all`
  - `cargo check -p main`
  - `cargo test -p main --no-run`
- **覆盖要求**: 至少覆盖存储层结构变更、同步链路编译、桌面窗口编译和表单引用链路

### 5. 依赖和集成点
- **外部依赖**: `rusqlite`、`serde`、`gpui`、`gpui-component`
- **内部依赖**:
  - 证书实体依赖 `StorageManager` / `Repository`
  - 证书同步依赖 `CloudSyncService` / `SyncEngine`
  - 表单选择依赖本地证书仓储与 popup 管理窗口
- **集成方式**: 证书数据独立存储并同步；连接参数内保存“引用 + 当前快照”；证书更新时回写所有引用连接
- **配置来源**: 不新增环境变量，继续使用本地 SQLite 和现有 sync_server 配置

### 6. 技术选型理由
- **为什么用这个方案**: “引用 + 快照”可以在保留现有连接运行链路的前提下实现统一管理和跨设备同步关联
- **优势**: 运行态改动小；连接仍可直接使用；证书修改后可统一回写连接；云同步仍能保留关联
- **劣势和风险**: 连接和证书会短期存在重复凭据；需要在证书保存、删除、同步回调时维护引用一致性

### 7. 关键风险点
- **引用一致性风险**: 证书获得 `cloud_id`、跨设备下载或删除时，需要同步刷新连接里的引用信息
- **表单复杂度风险**: 数据库表单是动态字段，证书选择和字段禁用逻辑要避免破坏现有渲染
- **同步顺序风险**: 证书同步应在连接同步前执行，确保证书更新能推动连接快照一起上传
- **工具约束**: 规范要求优先使用 `desktop-commander`、`context7`、`github.search_code`、`sequential-thinking`，但当前会话未提供这些工具；本次以本地源码检索和 Rust 构建命令替代并留痕

## 项目上下文摘要（app-settings-sync）
生成时间：2026-03-26 01:53:48 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/cloud_sync/workspace_sync.rs`
  - 模式：通过 `SyncTypeHandler` 接入 `generic_sync`，本地存储操作与加解密逻辑分层清晰。
  - 可复用：应用设置可沿用同一套类型化同步桥接方式，不需要再写一套专用同步流程。
  - 需注意：`on_uploaded` 必须回写远端 ID，否则下次会重复上传。

- **实现2**: `crates/core/src/cloud_sync/certificate_sync.rs`
  - 模式：上传成功后同时回写 `cloud_id` 与 `last_synced_at`，下载更新通过仓储完成。
  - 可复用：应用设置也需要持久化同步元数据，避免每次同步都被判定为本地较新。
  - 需注意：元数据回写不能把业务 `updated_at` 误刷新成本地当前时间，否则会形成同步回环。

- **实现3**: `main/src/setting_tab.rs`
  - 模式：`AppSettings` 目前只保存在本地 `settings.json`，所有设置项通过 `settings.save()` 直接落盘。
  - 可复用：应用设置仍继续以 JSON 为本地单一数据源，不额外引入数据库表。
  - 需注意：如果云端下载后只改磁盘不刷新全局状态，本次运行的主题、语言、终端行为不会立即生效。

- **实现4**: `main/src/home/home_tabs.rs`
  - 模式：终端设置变更后会立即应用到所有存活终端视图。
  - 可复用：应用设置从云端回写后，也应复用同类运行时刷新逻辑更新现有终端。
  - 需注意：当前“并同步”注释并未真正触发云同步，需要补齐触发链路。

### 2. 项目约定
- **命名约定**: 远端同步标识统一使用 `cloud_id` 语义；本次在设置文件里补 `local_id` 和 `remote_id`，并由 `SyncableItem` 映射到 `cloud_id`
- **文件组织**: 通用同步基础设施留在 `crates/core/src/cloud_sync`，主应用特有的数据类型同步放在 `main/src`
- **代码风格**: 继续采用增量式扩展，复用现有 `SyncEngine + generic_sync + SyncTypeHandler` 体系

### 3. 可复用组件清单
- `crates/core/src/cloud_sync/generic_sync.rs::generic_sync`
- `crates/core/src/cloud_sync/service.rs::prepare_*_sync_data_upload`
- `crates/core/src/cloud_sync/engine.rs::SyncEngine::register_type`
- `main/src/home_tab.rs::trigger_sync`
- `main/src/home/home_tabs.rs::apply_terminal_settings_to_all`

### 4. 测试策略
- **测试框架**: Rust 单元测试 + `cargo check`
- **参考位置**:
  - `crates/core/src/cloud_sync/service.rs` 现有加解密与同步数据测试
  - `main/src/home_tab.rs` 与 `main/src/setting_tab.rs` 现有运行时设置应用链路
- **本次覆盖**:
  - 应用设置下载合并时必须保留本地 `sync_server_url`
  - 应用设置本地保存必须写入稳定 `local_id / remote_id / updated_at / last_synced_at`
  - 同步完成后必须能重新应用到全局运行时状态

### 5. 依赖和集成点
- **外部依赖**: `serde`、`serde_json`
- **内部依赖**:
  - `SyncEngine::new(...).register_type(...)` 负责把主应用设置同步处理器接入引擎
  - `GlobalHomePage` 负责把设置保存后的同步请求路由到 `HomePage`
  - `AppSettings` 仍以 `settings.json` 为本地单一数据源
- **集成方式**: 本地设置修改后先落盘，再通过主页实体在“已登录且主密钥已解锁”条件下触发同步；同步完成后从磁盘重载到全局状态

### 6. 技术选型理由
- **为什么继续使用 JSON 本地存储**: `AppSettings` 已经是全局 JSON 配置，补同步元数据即可接入云同步，改动最小且符合现有结构
- **为什么处理器放在 `main`**: 应用设置属于主应用 UI 层数据，不应反向耦合进 `one_core` 领域模型
- **优势**: 复用现有同步框架，避免新增第二套同步协议
- **风险**: 应用设置不是多条记录而是单例，需要稳定名称和本地固定 ID 保证双端能够正确对齐

### 7. 关键风险点
- **同步回环风险**: 下载后的写盘如果刷新了本地 `updated_at`，会被下一轮同步误判为本地新修改
- **运行时不一致风险**: 云端下载后若不重载全局设置，当前窗口中的主题、语言与终端行为会滞后
- **设备专属字段风险**: `sync_server_url` 属于设备/环境配置，下载时应保留本地值，避免跨设备错误改写同步服务地址

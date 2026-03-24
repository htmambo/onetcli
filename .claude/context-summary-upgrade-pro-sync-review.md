## 项目上下文摘要（upgrade-pro-sync-review）
生成时间：2026-03-24 15:18:53 +0800

### 1. 相似实现分析
- **实现1**: `main/src/home_tab.rs:319`
  - 模式：UI 层只负责同步前门禁、状态切换和结果展示，真正的同步执行交给 `SyncEngine`
  - 可复用：`trigger_sync` 的统一入口、`syncing/sync_requested/cloud_error` 状态机
  - 需注意：同步完成后通过 `refresh_local_home_data(cx)` 兜底刷新本地工作区和连接

- **实现2**: `main/src/home_tab.rs:717`
  - 模式：认证恢复成功后，先拉取订阅更新 License，再在主密钥已解锁时触发自动同步
  - 可复用：`AuthService -> CloudApiClient -> LicenseService` 的登录后状态恢复链路
  - 需注意：当前实现把“拉取失败”和“无订阅”合并为同一个 `None`

- **实现3**: `main/src/home_tab.rs:751`
  - 模式：OTP 登录完成后走与会话恢复同构的“拉订阅 -> 更新 License -> 自动同步”流程
  - 可复用：登录成功后的 License 与同步联动
  - 需注意：和会话恢复一样，订阅请求错误会被静默吞掉

- **实现4**: `crates/core/src/cloud_sync/engine.rs:143`
  - 模式：常规 `sync()` 会先获取团队列表并缓存，再依次执行工作区和连接同步
  - 可复用：`cached_teams` 作为团队密钥版本和角色缓存
  - 需注意：只有走 `sync()` 才会预热团队缓存，单独冲突解决未复用该初始化步骤

### 2. 项目约定
- **命名约定**: 页面入口使用 `trigger_*` / `show_*`；服务层使用 `update_from_*`、`apply_*` 等动词前缀
- **文件组织**: UI 编排在 `main/src/home_tab.rs`；License 规则在 `crates/core/src/license/*`；云同步策略在 `crates/core/src/cloud_sync/*`
- **代码风格**: 广泛采用 `cx.spawn(async move |this, cx| ...)` 后回到 `this.update(...)` 修改 UI 状态
- **状态管理**: 跨层共享状态多通过 `Arc<RwLock<_>>` 或 GPUI 全局对象持有

### 3. 可复用组件清单
- `main/src/license.rs`：全局 License 初始化、特性门禁、升级 Pro 对话框
- `crates/core/src/license/service.rs`：订阅到 License 的转换、缓存恢复与清理
- `main/src/auth.rs`：会话恢复、OTP 登录、认证持久化
- `crates/core/src/cloud_sync/engine.rs`：同步流程编排与冲突解决入口
- `crates/core/src/cloud_sync/service.rs`：加密服务、团队密钥、`key_version` 选择逻辑

### 4. 测试策略
- **测试框架**: Rust 原生 `cargo test`
- **现有覆盖**:
  - `crates/core/src/license/service.rs:410` 起覆盖 License 基本行为
  - `crates/core/src/cloud_sync/service.rs:504` 起覆盖加解密和团队密钥基础行为
  - `crates/core/src/cloud_sync/conflict.rs:314` / `queue.rs:325` 覆盖冲突副本命名和队列
- **覆盖缺口**:
  - `main/src/home_tab.rs` 没有登录/订阅/同步联动测试
  - `crates/core/src/cloud_sync/engine.rs` 没有 `apply_conflict_resolutions` 路径测试

### 5. 依赖和集成点
- **升级 Pro 链路**: `show_upgrade_dialog` 打开外部购买页；订阅刷新仅发生在 `try_restore_session` 和 `verify_otp`
- **同步链路**: `HomePage::trigger_sync` -> `SyncEngine::sync` -> `WorkspaceSyncType` / `ConnectionSyncHandler`
- **冲突解决链路**: `HomePage::apply_conflicts_with_strategies` -> `SyncEngine::apply_conflict_resolutions`
- **配置/存储来源**: License 走 `LocalLicenseStorage`，同步数据走 `StorageManager` + `CloudApiClient`

### 6. 技术选型理由
- **为什么用这个方案**: 当前架构把 UI 编排和同步执行解耦，便于多个入口复用同一同步引擎
- **优势**: License、认证、同步各有独立服务层，基础能力可复用
- **劣势和风险**:
  - UI 层把订阅请求错误压扁为 `None`，把网络失败与“免费用户”混淆
  - 冲突解决走了与常规同步不同的初始化路径，容易出现状态不一致
  - 升级 Pro 后缺少在线刷新回流

### 7. 关键风险点
- **状态一致性**: 订阅请求失败会把现有 Pro 状态覆盖成 Free
- **团队同步**: 单独冲突解决未预热团队缓存，团队 `key_version` 可能退回默认值 `1`
- **用户体验**: 购买完成后当前会话内没有任何自动刷新订阅的触发点
- **验证缺口**: 关键问题都发生在 `HomePage` 与 `SyncEngine` 交互边界，现有测试没有覆盖

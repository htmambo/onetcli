## 项目上下文摘要（conflict-resolution-persist）
生成时间：2026-03-26 10:37:22 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/cloud_sync/connection_sync.rs:241-271`
  - 模式：正常“更新云端”流程在云端成功后会调用 `update_sync_status(...)`，推进本地 `last_synced_at`
  - 可复用：冲突解决后也必须同步推进本地同步状态
  - 需注意：如果只更新云端不更新本地同步状态，下一轮同步仍会判定为本地已修改

- **实现2**: `crates/core/src/cloud_sync/engine.rs:383-505`
  - 模式：手动冲突解决走 `apply_conflict_resolutions(...) -> apply_single_conflict(...)`
  - 可复用：直接在引擎里补齐状态推进，避免 UI 层做额外兜底
  - 需注意：当前返回的 `result.conflicts` 原先直接复制输入冲突，语义不准确

- **实现3**: `main/src/home_tab.rs:881-957`
  - 模式：桌面端在收到 `Ok(stats)` 后直接打印“冲突解决完成”并清空 `pending_conflicts`
  - 可复用：需要根据 `stats.errors / stats.conflicts` 是否为空决定是否真正清空
  - 需注意：否则会出现“日志显示完成，但下次启动又提示冲突”的误导

### 2. 项目约定
- **命名约定**: 同步状态统一使用 `last_synced_at` / `update_sync_status`
- **文件组织**: 冲突解决核心逻辑在 `cloud_sync/engine.rs`，页面提示在 `main/src/home_tab.rs`
- **代码风格**: 优先复用既有同步状态回写模式，不在 UI 层硬编码“假清空”

### 3. 可复用组件清单
- `crates/core/src/storage/repository.rs`: `ConnectionRepository::update_sync_status(...)`
- `crates/core/src/cloud_sync/connection_sync.rs`: 正常更新云端后的同步状态推进逻辑
- `main/src/home_tab.rs`: 现有冲突解决反馈与 `pending_conflicts` 管理

### 4. 测试策略
- **测试框架**: Rust 单元测试
- **测试模式**: 引擎级回归测试 + 全量 `one-core` 单测 + 桌面端编译检查
- **参考命令**:
  - `cargo test -p one-core use_local_conflict_resolution_updates_local_sync_status --lib`
  - `cargo test -p one-core --lib`
  - `cargo check -p one-core -p main`

### 5. 依赖和集成点
- **外部依赖**: 无新增依赖
- **内部依赖**:
  - `SyncEngine::apply_conflict_resolutions(...)`
  - `SyncEngine::apply_single_conflict(...)`
  - `ConnectionRepository::update_sync_status(...)`
  - `HomePage::resolve_conflicts_individually(...)`

### 6. 技术选型理由
- **为什么用这个方案**: 根因是手动冲突解决与正常同步链路不一致，缺少本地同步状态推进；修复应直接对齐正常同步行为
- **优势**: 修复点集中，能从源头消除下次启动再次识别为冲突的问题
- **劣势和风险**: 若冲突解决过程里真的发生局部失败，需要让 UI 保留未解决冲突，不能再无条件清空

### 7. 关键风险点
- **状态一致性**: “使用本地版本”后若不更新 `last_synced_at`，会持续重复冲突
- **UI 误导**: 即使引擎返回了错误，当前页面之前也会打印“冲突解决完成”
- **工具限制**: 仓库规范要求优先使用 `sequential-thinking`、`desktop-commander`、`context7`、`github.search_code`，当前执行环境未提供，只能基于源码检索、`cargo` 本地验证与 `.claude` 留痕完成本次修改

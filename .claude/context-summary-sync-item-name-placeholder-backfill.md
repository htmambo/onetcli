## 项目上下文摘要（sync-item-name-placeholder-backfill）
生成时间：2026-03-25 17:06:47 +0800

### 1. 相似实现分析
- **实现1**: `crates/core/src/cloud_sync/service.rs`
  - 模式：连接、工作区、凭证在上传 `CloudSyncData` 时统一写入明文 `name`
  - 结论：真实名称源头已经正确，问题不在上传构造

- **实现2**: `crates/core/src/cloud_sync/generic_sync.rs`
  - 模式：通用同步流程先构建 `cloud_id -> name` 映射，再按名称匹配、补写云端名称
  - 结论：如果把占位值 `name == id` 误认成真实名称，就会阻断旧记录回填

- **实现3**: `crates/core/src/cloud_sync/connection_sync.rs`
  - 模式：连接同步保留独立冲突处理，但名称映射和缺失名称补写逻辑与通用同步平行存在
  - 结论：必须和 `generic_sync.rs` 一起修，不能只改一处

- **实现4**: `sync_server/server/migrations/003_add_sync_item_name.sql`
  - 模式：旧数据迁移时把空 `name` 初始化成 `id`
  - 结论：这是旧记录在云端长期显示为 `id` 的直接来源，需要追加规范化迁移

### 2. 项目约定
- **命名约定**: 同步模型能力优先收敛到 `CloudSyncData` 自身方法，避免在多个同步器复制条件判断
- **文件组织**: 客户端兼容逻辑放在 Rust `cloud_sync` 模块；服务端数据修复通过独立 migration 落地
- **代码风格**: 以小范围增量修复为主，不改既有协议与表结构语义

### 3. 可复用组件清单
- `crates/core/src/cloud_sync/models.rs`: 适合承载 `CloudSyncData` 的统一判定方法
- `crates/core/src/cloud_sync/generic_sync.rs::build_name_map`: 通用名称解析入口
- `crates/core/src/cloud_sync/connection_sync.rs::build_cloud_name_map`: 连接同步名称解析入口
- `sync_server/server/migrations`: 服务端历史数据规范化入口

### 4. 测试策略
- **测试框架**: Rust 内置单元测试 + 现有构建检查
- **参考验证**:
  - `cargo check -p main`
  - `cargo test -p one-core cloud_sync::models::tests --lib`
  - `npm --prefix sync_server/server run check`
  - `npm --prefix sync_server/web run build`
- **覆盖重点**:
  - `name == id` 应视为占位值
  - 明文真实名称应继续被直接使用
  - 服务端迁移文件不破坏 TypeScript 校验和前端构建

### 5. 依赖和集成点
- **客户端依赖**: `CloudSyncData`、通用同步流程、连接同步流程
- **服务端依赖**: SQLite migration 执行顺序
- **集成方式**: 旧数据通过服务端迁移清空占位值，下一次客户端同步自动回填真实名称

### 6. 技术选型理由
- **为什么用统一判定方法**: `generic_sync.rs` 与 `connection_sync.rs` 当前都包含名称判定，抽到模型层可以避免后续再分叉
- **为什么新增 migration**: 仅靠客户端逻辑只能修“未来同步”，服务端旧值若不规范化，会持续让云端直接显示 `id`
- **优势**: 改动集中、协议不变、兼容旧数据
- **风险**: 极少数真实名称恰好等于 `id` 的记录会被当作占位值处理

### 7. 关键风险点
- **边界条件**: 真实名称与云端 `id` 完全一致时，会被识别为待回填占位值
- **验证缺口**: 当前缺少端到端自动化联调测试覆盖“旧记录迁移后下一次同步自动回填”
- **操作要求**: 旧记录需要再触发一次同步，真实名称才会覆盖回云端

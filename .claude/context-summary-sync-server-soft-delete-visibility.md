## 项目上下文摘要（sync-server-soft-delete-visibility）
生成时间：2026-03-25 16:06:27 +0800

### 1. 相似实现分析
- **实现1**: `sync_server/server/src/http/routes/sync.ts`
  - 模式：同步项列表默认 `includeDeleted !== "false"`，删除接口调用 `softDeleteSyncItem`
  - 需注意：远端删除不是物理删除，而是打 `deletedAt`

- **实现2**: `sync_server/server/src/db/database.ts`
  - 模式：`softDeleteSyncItem(...)` 只更新 `deleted_at`、`version`、`updated_at`
  - 需注意：如果前端统计不区分 `deletedAt`，视觉上会像“没删”

- **实现3**: `sync_server/web/src/views/user/DashboardView.vue`
  - 模式：概览卡片和最近同步项直接基于 `items` 计算
  - 需注意：当前会把软删除项一并计入工作区/总数统计

- **实现4**: `sync_server/web/src/views/user/SyncItemsView.vue`
  - 模式：完整列表显示状态列，但没有状态筛选
  - 需注意：默认全量展示时，用户容易把“已软删除”误认为“未删除”

### 2. 技术结论
- 桌面端本地删除工作区时，应用已经会尝试远端删除或写入待删除队列
- `sync_server` 的远端删除语义是软删除，不是物理删除
- 当前更像展示语义问题，不是同步删除调用缺失

### 3. 修改方向
- Web 概览统计只计算有效项
- 完整列表增加状态筛选，默认仅显示有效项
- 保留查看软删除记录的能力，避免丢失审计信息

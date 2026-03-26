## 项目上下文摘要（sync-server-web-delete-entry）
生成时间：2026-03-26 15:49:58 CST

### 1. 相似实现分析
- **实现1**: `sync_server/web/src/views/user/SyncItemsView.vue:21`
  - 模式：列表页负责筛选、分页、刷新和行级操作入口。
  - 可复用：现有 `loadItems()`、分页状态、状态徽标和行内 `RouterLink`。
  - 需注意：当前操作列只有“查看详情”，新增删除入口后要兼容“仅有效 / 全部 / 仅已软删除”筛选。

- **实现2**: `sync_server/web/src/views/user/SyncItemDetailView.vue:3`
  - 模式：详情页负责单条记录加载、刷新、错误回显和额外工具操作。
  - 可复用：`loadItem()`、顶部操作区、`errorMessage` / `loading` 处理。
  - 需注意：删除成功后详情页仍会保留软删除记录，应直接更新本地状态而不是跳空页面。

- **实现3**: `sync_server/web/src/views/user/ProfileView.vue:81`
  - 模式：危险操作统一使用 `danger-button + window.confirm + success/error message`。
  - 可复用：确认弹窗、禁用态按钮、结果提示文案结构。
  - 需注意：同步项删除不是“清空全部数据”，需要更细粒度的并发提示与工作区解绑提示。

- **实现4**: `sync_server/web/src/views/admin/AdminOverviewView.vue:85`
  - 模式：表格中的危险操作放在行级按钮，成功后刷新当前列表。
  - 可复用：表格行按钮布局、错误回显、按行触发确认。
  - 需注意：本次删除接口支持版本冲突，不能只做盲删后刷新。

- **实现5**: `sync_server/server/src/http/routes/sync.ts:161`
  - 模式：服务端已提供 `DELETE /api/v1/sync/items/:id`，支持 `version` 查询参数并在冲突时返回 409。
  - 可复用：现有删除接口和错误文案，不需要再补后端路由。
  - 需注意：要把当前记录版本传给服务端，才能落实“删除和更新并发时以更新为准”。

### 2. 项目约定
- **命名约定**: 页面内状态统一使用 `loading`、`errorMessage`、`*Message`、`*Error`、`*ing`。
- **文件组织**: API 封装在 `sync_server/web/src/services/api.ts`，类型定义在 `sync_server/web/src/types/api.ts`，用户页逻辑在 `sync_server/web/src/views/user/`。
- **导入顺序**: 先 Vue / Router，再服务与 store，再类型与工具函数。
- **代码风格**: 继续使用 Vue 3 `<script setup lang="ts">`、`ref/computed/watch`，模板里按钮样式沿用 `ghost-button` / `inline-button` / `danger-button`。

### 3. 可复用组件清单
- `sync_server/web/src/services/api.ts::request`：统一请求与 `ApiError` 包装。
- `sync_server/web/src/views/user/SyncItemsView.vue::loadItems`：列表刷新与筛选复位逻辑。
- `sync_server/web/src/views/user/SyncItemDetailView.vue::loadItem`：详情刷新与配置并行读取。
- `sync_server/web/src/views/user/ProfileView.vue::clearData`：危险确认和结果提示模式。
- `sync_server/web/src/utils/syncItemType.ts`：识别工作区 / 连接 / 凭证类型并输出标签。

### 4. 测试策略
- **测试框架**: `vue-tsc -b` + `vite build`
- **测试模式**: 以 TypeScript 静态检查和前端构建为主
- **参考文件**: `sync_server/web/package.json`
- **覆盖要求**: 至少验证 API 类型、Vue 模板和脚本逻辑可编译；同时手工验证列表页与详情页删除入口可见、冲突文案合理

### 5. 依赖和集成点
- **外部依赖**: `vue`、`vue-router`、`pinia`
- **内部依赖**: `useAuthStore -> api.deleteSyncItem -> sync_server/server DELETE /api/v1/sync/items/:id`
- **集成方式**: 删除按钮携带当前 `item.version` 调接口；成功后更新本地记录状态，409 冲突则刷新最新数据
- **配置来源**: `VITE_API_BASE_URL`

### 6. 技术选型理由
- **为什么用现有 DELETE 接口**: 服务端已具备版本冲突判定，直接复用能最小化改动并保持前后端语义一致。
- **优势**: 删除入口补齐后即可在 Web 端完成单条软删除，并天然支持“更新优先”。
- **劣势和风险**: 当前前端没有统一弹窗组件，只能继续使用 `window.confirm`，交互较基础。

### 7. 关键风险点
- **并发问题**: 若不传 `version`，会绕过“更新优先”的用户语义。
- **边界条件**: 已软删除记录不能继续重复删除；删除成功后在“仅有效”筛选下会直接从列表消失。
- **性能瓶颈**: 冲突后会触发一次重新加载，但同步项数量通常有限。
- **安全考虑**: 无新增鉴权逻辑，沿用现有登录态和 Bearer Token。

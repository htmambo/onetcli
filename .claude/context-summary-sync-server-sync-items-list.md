## 项目上下文摘要（sync-server-sync-items-list）
生成时间：2026-03-25 10:00:22 +0800

### 1. 相似实现分析
- **实现1**: `sync_server/web/src/views/user/DashboardView.vue`
  - 模式：用户首页在单个 `DashboardView` 中同时承担概览统计、同步配置编辑和最近同步项预览。
  - 可复用：`loadData` 的并行请求模式、`formatDate` 时间展示方式、卡片和列表的面板结构。
  - 需注意：当前最近同步项通过 `items.slice(0, 8)` 截断，只存在预览，没有完整列表入口。

- **实现2**: `sync_server/web/src/layouts/AppLayout.vue`
  - 模式：应用内导航通过 `links` 计算属性集中维护，`RouterLink` 直接使用固定路径做高亮。
  - 可复用：在 `links` 中新增子页面导航项即可让侧边栏自动展示入口。
  - 需注意：当前只有“同步概览”“账号设置”“管理界面”三个入口，没有用户级同步项列表页。

- **实现3**: `sync_server/web/src/views/admin/AdminOverviewView.vue`
  - 模式：完整数据列表页使用独立视图，通过 `onMounted + loadData` 拉取数据，并用表格展示批量记录。
  - 可复用：错误提示块、顶部说明区、刷新按钮、表格布局与时间格式化模式。
  - 需注意：该页是管理员视图，风格可复用，但数据和权限逻辑不能直接复用。

- **实现4**: `sync_server/server/src/http/routes/sync.ts`
  - 模式：后端 `/api/v1/sync/items` 已支持返回当前账号的完整同步项列表，按查询参数筛选，默认包含已软删除记录。
  - 可复用：无需新增后端接口，前端可直接调用 `api.listSyncItems(token)` 获取完整列表。
  - 需注意：当前接口未提供分页，前端完整列表页应做好横向滚动与空状态处理。

### 2. 项目约定
- **命名约定**: Vue 路由名使用短横线含义清晰的英文标识，页面组件使用 `PascalCase`，组合式状态使用 `ref`/`computed`
- **文件组织**: 用户视图位于 `sync_server/web/src/views/user`，全局导航位于 `sync_server/web/src/layouts/AppLayout.vue`，API 封装集中在 `sync_server/web/src/services/api.ts`
- **导入顺序**: 先框架包，再本地组件/服务/类型，类型导入与值导入分开
- **代码风格**: 使用 Vue 3 `<script setup lang="ts">` 和 Composition API；列表页与仪表盘沿用现有圆角面板、浅色边框和 Tailwind 原子类

### 3. 可复用组件清单
- `sync_server/web/src/services/api.ts::api.listSyncItems`：读取当前账号全部同步项
- `sync_server/web/src/layouts/AppLayout.vue::links`：侧边栏导航入口集中配置点
- `sync_server/web/src/views/user/DashboardView.vue::formatDate`：同步项时间展示格式
- `sync_server/web/src/views/admin/AdminOverviewView.vue` 的表格布局：完整列表页的展示参考

### 4. 测试策略
- **测试框架**: 当前 `sync_server/web` 未发现 `.spec` 或 `.test` 文件
- **测试模式**: 以 TypeScript 类型检查和 Vite 生产构建作为本地自动验证
- **参考文件**: `sync_server/web/package.json` 中 `build` 脚本为 `vue-tsc -b && vite build`
- **覆盖要求**: 至少验证新增页面、路由和导航在构建阶段通过；手工关注最近同步项预览限制为 5 条且完整列表入口可访问

### 5. 依赖和集成点
- **外部依赖**: `vue`、`vue-router`、`pinia`、`vite`
- **内部依赖**:
  - `DashboardView` 依赖 `api.listSyncItems` 与 `api.getSyncConfig`
  - 新增列表页将依赖 `api.listSyncItems`
  - `AppLayout` 负责暴露页面入口
- **集成方式**: 通过 Vue Router 新增用户子路由，再由仪表盘按钮和侧边栏导航跳转
- **配置来源**: API 地址来自 `VITE_API_BASE_URL`

### 6. 技术选型理由
- **为什么用这个方案**: 后端接口已经完整，新增独立前端列表页成本最低，也最符合现有“仪表盘预览 + 独立列表页”的实现模式
- **优势**: 不改后端协议；仪表盘更聚焦；完整列表页可承载更多字段展示
- **劣势和风险**: 由于没有分页，数据很多时列表页会一次性渲染全部记录；不过当前需求只要求入口和展示，风险可接受

### 7. 关键风险点
- **术语风险**: 用户口头说“全部链接列表”，但项目实际数据结构是“同步项”；文案需要在用户意图和现有术语之间保持一致
- **空状态风险**: 新页面需要独立处理无数据和加载失败，不能只依赖仪表盘预览
- **一致性风险**: 预览条数、按钮入口和侧边栏导航若不统一，用户会感知到页面行为不一致
- **工具约束**: 本次环境未提供 `desktop-commander`、`context7`、`github.search_code`，已改用仓库内现有实现和本地构建脚本完成上下文收集与验证

### 8. 追加上下文（详情能力）
- **后端已有能力**: `sync_server/server/src/db/database.ts::getSyncItem(ownerId, id)` 已存在，说明详情能力只缺 HTTP 路由暴露，不需要修改数据库层
- **前端现状**: `sync_server/web` 在本次改动前没有任何动态路由或详情页模式，因此需要一并补路由、列表入口和高亮逻辑
- **实现决策**: 详情页采用独立路由而不是列表页展开，一方面便于直达和刷新，另一方面能承载 `encryptedData` 等长文本字段

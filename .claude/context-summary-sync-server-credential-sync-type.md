## 项目上下文摘要（sync-server-credential-sync-type）
生成时间：2026-03-25 12:46:46 +0800

### 1. 相似实现分析
- **实现1**: `sync_server/web/src/utils/syncItemType.ts`
  - 模式：同步项类型的展示文案统一收敛在单一工具函数中。
  - 可复用：新增凭证类型时应继续在这里统一处理映射，而不是在页面里分散写条件判断。
  - 需注意：当前只识别 `connection / workspace / app_settings`，会把新增类型显示为“未识别类型”。

- **实现2**: `sync_server/web/src/views/user/DashboardView.vue`
  - 模式：概览页通过 `computed` 直接对同步项数组做分类统计。
  - 可复用：新增类型统计应继续沿用计算属性模式，不引入额外状态层。
  - 需注意：当前统计字段是硬编码判断，新增类型不会进入概览卡片。

- **实现3**: `sync_server/web/src/views/user/SyncItemsView.vue`
  - 模式：列表页用 `computed` 从真实数据集中生成筛选选项，并在前端分页。
  - 可复用：新增类型应该走动态选项生成和统一标签映射。
  - 需注意：当前去重逻辑按原始 `dataType` 做唯一值聚合，若存在别名类型会出现重复语义选项。

- **实现4**: `sync_server/server/src/http/routes/sync.ts` + `sync_server/server/src/db/database.ts`
  - 模式：服务端对 `dataType` 不做白名单限制，按字符串透传和筛选。
  - 可复用：新增类型无需改后端协议，前端适配即可。
  - 需注意：如果前端不做兼容，新类型只会在页面上以未知类型出现。

### 2. 项目约定
- **命名约定**: Vue 页面使用 `<script setup lang="ts">`；工具函数使用语义清晰的纯函数命名
- **文件组织**: 展示层类型映射集中在 `sync_server/web/src/utils`
- **代码风格**: 通过 `computed` 派生筛选、分页和统计，不引入额外 store

### 3. 可复用组件清单
- `sync_server/web/src/utils/syncItemType.ts`
- `sync_server/web/src/views/user/DashboardView.vue`
- `sync_server/web/src/views/user/SyncItemsView.vue`
- `sync_server/web/src/views/user/SyncItemDetailView.vue`

### 4. 测试策略
- **测试框架**: 以前端构建校验为主
- **参考命令**:
  - `npm --prefix sync_server/web run build`
- **覆盖要求**: 至少验证类型映射、筛选和概览页编译链路通过

### 5. 依赖和集成点
- **外部依赖**: Vue 3、Vue Router、Vite、TypeScript
- **内部依赖**: `api.listSyncItems`、`api.getSyncItem`、`syncItemType.ts`
- **集成方式**: 服务端继续透传 `dataType`，前端统一归一化并展示

### 6. 技术选型理由
- **为什么用这个方案**: 后端协议已支持任意字符串类型，最小变更是前端做统一兼容层
- **优势**: 改动小、兼容历史 `certificate` 数据和未来 `credential` 命名
- **劣势和风险**: 如果后续需要在服务端做严格类型校验，还需同步补白名单

### 7. 关键风险点
- **别名重复风险**: 若同时存在 `certificate` 和 `credential`，筛选选项可能重复
- **统计遗漏风险**: 概览页若继续写死类型判断，会漏掉新增凭证项
- **验证限制**: 当前没有浏览器级自动化测试，只能用构建验证兜底

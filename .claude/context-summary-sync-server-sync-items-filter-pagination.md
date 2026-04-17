## 项目上下文摘要（sync-server-sync-items-filter-pagination）
生成时间：2026-03-25 11:51:05 +0800

### 1. 相似实现分析
- **实现1**: `sync_server/web/src/views/user/SyncItemsView.vue:21`
  - 模式：列表页使用“顶部说明卡片 + 表格卡片”的两段式结构
  - 可复用：刷新按钮、错误/空态提示、表格列定义、详情跳转入口
  - 需注意：当前表格直接遍历 `items`，没有任何中间视图状态，适合在本文件中补 `computed` 派生数据

- **实现2**: `sync_server/web/src/views/admin/AdminOverviewView.vue:21`
  - 模式：表格页头部使用左侧说明 + 右侧统计徽标
  - 可复用：列表统计的展示方式、操作按钮组样式、表格行边框和状态标签样式
  - 需注意：管理页没有筛选控件，说明本次筛选/分页最好先在单页内自包含实现，不抽新组件

- **实现3**: `sync_server/web/src/views/user/DashboardView.vue:109`
  - 模式：数据列表由 `computed` 派生展示子集，例如 `previewItems`
  - 可复用：从完整数据集派生“视图层结果”的组织方式，适合扩展为筛选结果和分页结果
  - 需注意：当前项目倾向于把显示逻辑留在 `computed`，而不是在模板里写复杂表达式

- **实现4**: `sync_server/web/src/views/user/SyncItemDetailView.vue:116`
  - 模式：路由相关页面状态使用 `ref + computed + watch` 组合维护
  - 可复用：轻量级响应式状态写法，适合处理筛选条件变化后页码复位
  - 需注意：项目使用 `script setup` 和组合式 API，不引入 Options API

### 2. 项目约定
- **命名约定**: 页面状态使用语义化英文命名，如 `loading`、`errorMessage`、`previewItems`
- **文件组织**: 页面逻辑集中在对应 `src/views/...View.vue` 文件，不额外拆 composable
- **导入顺序**: 先 Vue API，再项目内模块
- **代码风格**: 2 空格缩进，样式通过内联 Tailwind 类表达，状态派生优先用 `computed`

### 3. 可复用组件清单
- `sync_server/web/src/views/user/SyncItemsView.vue`：列表表格、刷新交互和空态提示
- `sync_server/web/src/views/admin/AdminOverviewView.vue`：统计徽标和操作区布局风格
- `sync_server/web/src/views/user/DashboardView.vue`：使用 `computed` 派生展示子集的模式
- `sync_server/web/src/utils/syncItemType.ts`：同步项类型中文标签映射
- `sync_server/web/src/utils/syncItemVersion.ts`：密钥版本和记录版本格式化

### 4. 测试策略
- **测试框架**: 当前 `sync_server/web` 未配置单元测试
- **测试模式**: 使用 `npm --prefix sync_server/web run build` 做类型检查和生产构建验证
- **参考文件**: `sync_server/web/package.json`
- **覆盖要求**: 验证筛选状态、分页状态、模板绑定和新增 `watch/computed` 不影响构建

### 5. 依赖和集成点
- **外部依赖**: Vue 3.5、Vue Router 4、Tailwind CSS 4
- **内部依赖**: `SyncItemsView` 依赖 `useAuthStore()`、`api.listSyncItems()`、`getSyncItemTypeLabel()`、版本格式化工具
- **集成方式**: 前端本地拿到完整 `items` 后在页面内做筛选和分页，不改后端接口
- **配置来源**: `sync_server/web/src/views/user/SyncItemsView.vue` 和 `sync_server/web/package.json`

### 6. 技术选型理由
- **为什么用这个方案**: 用户只要求简单筛选和分页，直接基于前端已拿到的完整数组做本地派生，改动最小且不需要扩接口
- **优势**: 实现快、风险小、无需改服务端、保留现有排序结果
- **劣势和风险**: 如果未来同步项非常多，前端本地分页的性能和首屏数据量会受限，但当前页面目标明显偏管理查看

### 7. 关键风险点
- **边界条件**: 切换筛选条件或每页条数后，当前页可能越界，必须自动回到第一页或夹紧页码
- **可用性风险**: 如果筛选后结果为空，需要和“接口返回空数据”区分文案
- **性能瓶颈**: 当前只做简单 `filter/slice`，数据量中等时影响可忽略
- **工具限制**: 规范要求优先使用 `sequential-thinking`、`desktop-commander`、`context7`、`github.search_code`，当前执行环境未提供这些工具，本次改为基于仓库源码、`rg` 和本地 Node 构建命令完成检索与验证

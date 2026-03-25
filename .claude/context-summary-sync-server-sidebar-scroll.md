## 项目上下文摘要（sync-server-sidebar-scroll）
生成时间：2026-03-25 11:45:07 +0800

### 1. 相似实现分析
- **实现1**: `sync_server/web/src/layouts/AppLayout.vue:2`
  - 模式：认证后页面统一挂在 `AppLayout` 双栏布局下，左侧导航 + 右侧 `RouterView`
  - 可复用：现有侧栏结构、`isActive()` 激活判定、账号卡片和退出登录区域
  - 需注意：当前整体依赖页面自然滚动，左侧栏会被右侧长内容拉伸并随页面一起滚动

- **实现2**: `sync_server/web/src/views/auth/LoginView.vue:2`
  - 模式：登录页使用 `min-h-screen` 让卡片在视口内居中，不影响其它布局
  - 可复用：页面级高度控制仍由顶层容器负责，而不是拆散到子组件
  - 需注意：登录/注册页不是本次改动目标，不能误伤访客态布局

- **实现3**: `sync_server/web/src/views/user/ProfileView.vue:2`
  - 模式：子页面只负责自身内容网格和卡片，不管理外层滚动容器
  - 可复用：所有 `/app` 子页面都默认输出可自然增高的内容块
  - 需注意：如果改成主内容区独立滚动，滚动容器必须放在 `AppLayout`，否则每个子页都要重复处理

- **实现4**: `sync_server/web/src/router/index.ts:25`
  - 模式：`/app` 路由统一复用 `AppLayout`，内部子路由切换不销毁布局壳层
  - 可复用：只改一个布局文件即可覆盖同步概览、同步项、账号设置、管理界面
  - 需注意：一旦主内容改为独立滚动容器，切路由时最好主动复位滚动位置

### 2. 项目约定
- **命名约定**: 组件使用 PascalCase 文件名，组合式 API 使用 `script setup`，局部引用使用语义化英文变量名
- **文件组织**: 全局壳层位于 `src/layouts/`，页面位于 `src/views/`，路由位于 `src/router/`
- **导入顺序**: 先第三方库，再项目内别名模块
- **代码风格**: 2 空格缩进，模板类名以内联 Tailwind 原子类表达，不额外抽离局部样式

### 3. 可复用组件清单
- `sync_server/web/src/layouts/AppLayout.vue`：全局侧栏、主内容容器和路由出口
- `sync_server/web/src/router/index.ts`：确认 `/app` 子页面全部复用同一布局
- `sync_server/web/src/style.css`：全局高度基线和面板视觉变量

### 4. 测试策略
- **测试框架**: 当前 `sync_server/web` 未配置单元测试框架
- **测试模式**: 使用本地 `npm --prefix sync_server/web run build` 做类型检查和生产构建验证
- **参考文件**: `sync_server/web/package.json`
- **覆盖要求**: 本次重点验证 Vue 模板类型、组合式 API 改动和打包产物生成

### 5. 依赖和集成点
- **外部依赖**: Vue 3.5、Vue Router 4、Tailwind CSS 4
- **内部依赖**: `AppLayout` 直接依赖 `useAuthStore()`、`useRoute()`、`useRouter()`
- **集成方式**: 通过 `<RouterView />` 挂载所有 `/app` 子页面
- **配置来源**: `sync_server/web/package.json` 与 `sync_server/web/src/router/index.ts`

### 6. 技术选型理由
- **为什么用这个方案**: 把独立滚动能力集中在布局层，能一次性覆盖所有业务页，同时不改动每个子页面内部结构
- **优势**: 改动面小、移动端可保留原行为、桌面端左栏高度稳定
- **劣势和风险**: 主内容区改为内部滚动后，路由切换默认不会自动回到顶部，需要额外处理

### 7. 关键风险点
- **边界条件**: 小屏幕下如果也强制固定高度，会导致顶部侧栏挤压内容，因此仅在 `lg` 及以上启用独立滚动
- **集成风险**: `AppLayout` 在子路由之间会持久存在，必须注意独立滚动容器的滚动复位
- **性能瓶颈**: 仅为布局样式和一次轻量 watch，性能影响可忽略
- **工具限制**: 仓库规范要求优先使用 `sequential-thinking`、`desktop-commander`、`context7`、`github.search_code`，当前执行环境未提供这些工具，本次改为使用本地源码检索与构建验证

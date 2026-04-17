## 项目上下文摘要（sync-server-user-nickname）
生成时间：2026-03-25 10:40:01 +0800

### 1. 相似实现分析
- **实现1**: `sync_server/server/src/db/database.ts`
  - 模式：用户数据集中通过 `UserRecord`、`toPublicUser`、`createUser`、`findSessionWithUserByTokenHash` 和 `listUsers` 串联数据库层与接口层。
  - 可复用：只要补齐 `UserRecord` 字段、查询 SQL 和 `toPublicUser` 映射，认证、`/me`、管理员列表都会自动拿到新字段。
  - 需注意：现有 `users` 表和查询语句都未包含昵称，必须同步更新迁移、初始化 SQL 和登录态查询。

- **实现2**: `sync_server/server/src/http/routes/auth.ts` + `sync_server/server/src/services/auth.ts`
  - 模式：账号自助能力统一挂在 `/api/v1/auth/*`，业务校验放在 `AuthService`，路由只做参数解析和错误返回。
  - 可复用：昵称自助修改适合新增到这里，沿用 `requireAuth + zod + AuthService` 模式。
  - 需注意：返回给前端的 `PublicUser` 必须包含昵称，否则登录、会话恢复和资料页刷新会出现字段不一致。

- **实现3**: `sync_server/web/src/views/user/ProfileView.vue`
  - 模式：用户自助设置页目前已承载“修改密码”和“清空同步数据”两类自助操作，使用独立表单、消息提示和本地状态。
  - 可复用：昵称设置表单可以沿用相同表单和反馈模式，不需要新增独立路由或状态层。
  - 需注意：页面目前只显示密码相关能力，需要调整布局以容纳昵称设置卡片。

- **实现4**: `sync_server/web/src/layouts/AppLayout.vue` + `sync_server/web/src/views/admin/AdminOverviewView.vue`
  - 模式：当前账号信息和管理员账号列表都直接依赖 `PublicUser` / `AdminUserSummary` 渲染。
  - 可复用：昵称字段进入用户类型后，可直接补到侧边栏和管理员表格中，形成一致展示。
  - 需注意：若昵称默认等于邮箱，界面需要避免只显示重复信息。

### 2. 项目约定
- **命名约定**: 数据库字段使用 `snake_case`，前端接口字段使用 `camelCase`，Vue 组件使用 `<script setup lang="ts">`
- **文件组织**: 数据库结构在 `sync_server/server/migrations` 和 `database.ts`；用户接口在 `sync_server/server/src/http/routes/auth.ts`；前端用户资料页在 `sync_server/web/src/views/user/ProfileView.vue`
- **导入顺序**: 先框架/第三方，再本地模块；类型导入与值导入分离
- **代码风格**: 沿用现有轻量 REST 风格和页面面板布局，不额外引入状态管理层或表单库

### 3. 可复用组件清单
- `sync_server/server/src/db/database.ts::toPublicUser`
- `sync_server/server/src/services/auth.ts::createSessionForUser`
- `sync_server/web/src/stores/auth.ts::refreshMe`
- `sync_server/web/src/views/user/ProfileView.vue` 现有表单反馈模式

### 4. 测试策略
- **测试框架**: 当前 `sync_server` 未发现前端或服务端独立测试文件
- **测试模式**: 以 TypeScript 类型检查和构建验证为主
- **参考命令**:
  - `npm run build`（`sync_server`）
  - `npm run check`（`sync_server/server`）
- **覆盖要求**: 至少确保迁移 SQL、服务端类型、前端资料页和用户类型改动都通过构建

### 5. 依赖和集成点
- **外部依赖**: `better-sqlite3`、`fastify`、`zod`、`vue`、`pinia`
- **内部依赖**:
  - `authPlugin` 依赖 `PublicUser`
  - `AuthService` 依赖 `DatabaseClient`
  - `AppLayout`、`ProfileView`、`AdminOverviewView` 依赖前端 `PublicUser` / `AdminUserSummary`
- **集成方式**: 通过数据库迁移新增列，服务端映射到 `PublicUser`，前端通过既有登录态和资料页接口刷新
- **配置来源**: 无新增环境变量或配置项

### 6. 技术选型理由
- **为什么用这个方案**: 昵称是用户资料字段，最合适放在 `users` 表中，并通过自助资料接口维护
- **优势**: 数据结构简单；默认值可直接从邮箱回填；前后端改动集中
- **劣势和风险**: 需要兼容已有数据库，必须通过迁移回填历史用户昵称

### 7. 关键风险点
- **迁移风险**: 旧库必须把 `nickname` 回填为 `email`，否则已有账号会出现空昵称
- **会话一致性风险**: 若登录态返回不含昵称，前端会在刷新前后表现不一致
- **展示重复风险**: 昵称默认等于邮箱时，侧边栏和管理页要注意不要让信息显得冗余
- **工具约束**: 仓库要求优先使用 `desktop-commander`、`context7`、`github.search_code`，但本次会话未提供这些工具；已使用本地代码检索和构建命令替代并留痕

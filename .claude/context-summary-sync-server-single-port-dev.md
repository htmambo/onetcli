## 项目上下文摘要（sync-server-single-port-dev）
生成时间：2026-03-24 19:31:20 +0800

### 1. 相似实现分析
- **实现1**: `sync_server/package.json:9`
  - 模式：根级 `dev` 脚本通过 `concurrently` 同时启动 `server` 与 `web`。
  - 可复用：保留 workspace 结构和 `dev:server` / `dev:web` 子脚本，不改目录组织。
  - 需注意：双端口问题正是从这里产生，默认入口需要调整。

- **实现2**: `sync_server/server/src/http/app.ts:44`
  - 模式：后端统一负责网页入口，生产模式托管 `web/dist`，开发模式此前通过根路径代理到 Vite。
  - 可复用：继续让 Fastify 成为唯一对外入口，API 与 Web 共用同一个服务。
  - 需注意：新的开发模式必须保留 `/api/*` 与 `/health` 本地路由优先级。

- **实现3**: `sync_server/web/vite.config.ts:13`
  - 模式：前端已有独立 Vite 配置，包含 Vue、Tailwind 和 API 代理规则。
  - 可复用：单端口开发时应继续复用这份 Vite 配置，而不是重写前端构建链。
  - 需注意：如果改成 Vite middleware，需要把 HMR 挂到父级 HTTP server，避免另开端口。

- **实现4**: `sync_server/node_modules/vite/dist/node/index.d.ts:2388`
  - 模式：Vite 原生支持 `server.middlewareMode`，并允许把 HMR WebSocket 挂到父级 `HttpServer`。
  - 可复用：这是标准化方案，符合仓库“优先复用官方能力”的要求。
  - 需注意：middleware 模式下 `httpServer` 为 `null`，请求需要由 Fastify 路由转交给 `vite.middlewares`。

### 2. 项目约定
- **命名约定**: Web 入口统一由 `sync_server/server/src/http/app.ts` 编排。
- **文件组织**: 根目录负责 workspace 脚本，`server` 只处理 Fastify 服务，`web` 只保留前端资源与 Vite 配置。
- **代码风格**: 偏向直接、少抽象的 Fastify 配置对象与小型 helper。
- **运行模式**: `src` 运行态可视为开发模式，`dist` 运行态可视为生产模式。

### 3. 可复用组件清单
- `sync_server/package.json`
- `sync_server/server/src/http/app.ts`
- `sync_server/server/src/config/env.ts`
- `sync_server/web/vite.config.ts`
- `sync_server/web/index.html`
- `sync_server/README.md`

### 4. 测试策略
- 当前 `sync_server` 没有现成的 `*.test.*` / `*.spec.*`。
- 本次采用本地可重复验证：
  - `npm run check --workspace server`
  - `npm run build`
  - `tsx --eval "... await app.ready() ..."` 验证源码运行态单端口中间件初始化
  - `node --input-type=module -e "... await app.ready() ..."` 验证构建产物静态模式

### 5. 依赖和集成点
- **外部依赖**: `fastify`、`@fastify/cors`、`@fastify/static`、`vite`
- **内部依赖**: `env.webDistPath`、`env.webRootPath`、`web/vite.config.ts`
- **集成方式**: Fastify 保留 API 路由，本地前端请求在兜底路由中转交给 `vite.middlewares`
- **配置来源**: `sync_server/server/src/config/env.ts`

### 6. 技术选型理由
- **为什么选 Vite middleware**: 官方标准能力，能保留 HMR，又不需要第二个对外端口。
- **为什么不继续代理 5173**: 代理模式仍要求独立 Vite server 监听额外端口，无法满足用户目标。
- **为什么默认 `dev` 改为单进程**: 用户当前的核心问题是“默认启动出现两个端口”，应先修正默认路径。

### 7. 关键风险点
- **开发/生产切换风险**: 运行态识别不能只依赖 `web/dist` 是否存在，否则开发环境在有旧构建产物时会误走静态分支。
- **HMR 集成风险**: 必须把 Vite 的 HMR 绑到 Fastify 的底层 HTTP server，否则仍会额外监听端口。
- **验证限制**: 当前沙箱禁止真实监听端口，需通过 `app.ready()` 和源码/产物两种运行态分别验证。
- **工具限制**: 仓库规范要求优先使用 `context7`、`github.search_code`、`desktop-commander`，当前执行环境未提供这些工具，本次改为基于本地源码与依赖源码完成检索。

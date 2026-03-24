## 项目上下文摘要（sync-server-proxy-options-conflict）
生成时间：2026-03-24 19:20:45 +0800

### 1. 相似实现分析
- **实现1**: `sync_server/server/src/http/app.ts:44`
  - 模式：生产模式通过 `fastifyStatic` 托管前端资源，并只对 `GET /*` 做 `index.html` 回退。
  - 可复用：开发模式也应只暴露前端实际需要的页面资源请求方法。
  - 需注意：`/api/*` 与 `/health` 必须继续留给本地 Fastify 路由处理。

- **实现2**: `sync_server/server/src/http/app.ts:60`
  - 模式：开发模式把根路径 `"/"` 交给 `@fastify/http-proxy` 代理到 Vite。
  - 可复用：保留 `prefix`、`rewritePrefix`、`websocket` 和请求头重写逻辑，不改动代理目标。
  - 需注意：根路径代理会覆盖 `"/"` 与 `"/*"` 两个路由，方法集过宽时容易与其他插件冲突。

- **实现3**: `sync_server/node_modules/@fastify/cors/index.js:72`
  - 模式：`@fastify/cors` 默认注册 `fastify.options('*', ...)` 作为全局预检入口。
  - 可复用：预检响应应继续由 CORS 插件统一处理，而不是由前端资源代理处理。
  - 需注意：只要其他插件再次声明同路径 `OPTIONS`，Fastify 启动阶段就会直接报重复路由错误。

- **实现4**: `sync_server/node_modules/@fastify/http-proxy/index.js:9`
  - 模式：`@fastify/http-proxy` 的默认 `httpMethods` 包含 `OPTIONS`，并对 `['/', '/*']` 批量注册路由。
  - 可复用：插件支持用 `httpMethods` 显式收窄代理方法集。
  - 需注意：当前根路径代理如果不覆写 `httpMethods`，会与 CORS 的 `OPTIONS *` 冲突。

### 2. 项目约定
- **命名约定**: Fastify 启动入口集中在 `sync_server/server/src/http/app.ts`。
- **文件组织**: HTTP 插件注册、API 路由注册和前端托管逻辑都在 `createApp()` 内集中编排。
- **代码风格**: 使用直接的对象字面量配置插件，少做额外抽象。
- **注释规范**: 只在非显而易见的框架行为旁补充简短中文意图说明。

### 3. 可复用组件清单
- `sync_server/server/src/http/app.ts`
- `sync_server/server/src/config/env.ts`
- `sync_server/README.md`
- `sync_server/node_modules/@fastify/cors/index.js`
- `sync_server/node_modules/@fastify/http-proxy/index.js`
- `sync_server/node_modules/@fastify/http-proxy/README.md`

### 4. 测试策略
- 当前 `sync_server` 未提供现成的 `*.test.*` / `*.spec.*` 测试文件。
- 采用本地可重复验证：
  - `npm run build --workspace server`
  - `node --input-type=module -e "...createApp(); await app.ready(); ..."` 作为最小复现与回归验证
  - `npm run check --workspace server`

### 5. 依赖和集成点
- **外部依赖**: `fastify`、`@fastify/cors`、`@fastify/http-proxy`、`@fastify/static`
- **内部依赖**: `env.webDistPath` 决定静态托管还是开发代理分支
- **集成方式**: API 路由先注册，根路径前端托管逻辑后注册
- **配置来源**: `sync_server/server/src/config/env.ts`

### 6. 技术选型理由
- **为什么不关闭 CORS 预检**: API 仍需要统一处理浏览器预检，请保留现有 CORS 行为。
- **为什么收窄代理方法**: 根路径代理只服务前端页面、静态资源和 HMR，不需要 `OPTIONS`。
- **为什么只改一处**: 问题由插件组合触发，最小修复点就是开发模式代理配置。

### 7. 关键风险点
- **行为漂移风险**: 如果开发模式代理允许的 HTTP 方法多于生产模式，未来更容易出现隐藏分歧。
- **验证限制**: 目前没有自动化测试框架，需要用 `app.ready()` 冒烟覆盖启动期错误。
- **工具限制**: 仓库要求优先使用 `context7`、`github.search_code`、`desktop-commander`，当前执行环境未提供这些工具，本次改为基于本地源码与依赖源码完成检索。

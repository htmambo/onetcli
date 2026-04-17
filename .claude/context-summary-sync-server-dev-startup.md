## 项目上下文摘要（sync-server-dev-startup）
生成时间：2026-03-24 19:39:10 +0800

### 1. 相似实现分析
- **实现1**: `sync_server/package.json:10`
  - 模式：根级默认开发入口通过 workspace 调用 `server` 包的 `dev` 脚本。
  - 可复用：问题修复应优先落在 `server/package.json` 和 `env.ts`，不要再改回双进程结构。
  - 需注意：workspace 执行时实际 `cwd` 是 `sync_server/server`。

- **实现2**: `sync_server/server/package.json:7`
  - 模式：当前使用 `tsx watch src/main.ts` 作为热更新入口。
  - 可复用：仍可继续复用 `tsx` 作为 TypeScript 运行时，只是要避开其额外 IPC 监听模式。
  - 需注意：当前环境里 `tsx watch` 在业务代码启动前就会因为创建 `/tmp/tsx-*.pipe` 失败而退出。

- **实现3**: `sync_server/server/src/config/env.ts:5`
  - 模式：环境变量文件当前按 `process.cwd()` 读取。
  - 可复用：继续使用 `dotenv`，但路径必须绑定到项目根而不是工作目录。
  - 需注意：默认 `npm run dev --workspace server` 时 `cwd` 是 `sync_server/server`，会漏掉根目录 `.env`。

- **实现4**: `node --watch --import tsx/esm server/src/main.ts`
  - 模式：Node 原生 `--watch` 可以直接重启 ESM 入口，`tsx/esm` 负责 TypeScript 运行时转换。
  - 可复用：这是标准能力组合，不需要引入新的守护进程工具。
  - 需注意：业务进程启动后仍会在当前沙箱因端口监听受限报 `listen EPERM`，但这已经说明脚本入口不再被 `tsx watch` 卡住。

### 2. 项目约定
- **命名约定**: `dev` 仍保留在 `sync_server/server/package.json`。
- **文件组织**: 环境读取逻辑继续集中在 `sync_server/server/src/config/env.ts`。
- **代码风格**: 运行时修复优先做最小改动，不引入新的脚本文件。
- **开发路径**: 根级 `npm run dev` 应始终可复用 `sync_server/.env`。

### 3. 可复用组件清单
- `sync_server/package.json`
- `sync_server/server/package.json`
- `sync_server/server/src/config/env.ts`
- `sync_server/server/src/main.ts`

### 4. 测试策略
- `npm run dev`：确认不再死在 `tsx watch` IPC 初始化阶段
- `npm --workspace server exec -- node -p 'process.cwd()'`：确认 workspace 运行目录
- `node --watch --import tsx/esm server/src/main.ts`：确认新开发命令能进入业务启动逻辑
- `npm run check --workspace server`：确认修改未破坏类型检查

### 5. 依赖和集成点
- **外部依赖**: `tsx`、`dotenv`、Node 原生 `--watch`
- **内部依赖**: `server/src/main.ts`、`server/src/http/app.ts`
- **配置来源**: `sync_server/.env`
- **启动链路**: 根级 `npm run dev` → workspace `server` 的 `dev` 脚本 → `src/main.ts`

### 6. 技术选型理由
- **为什么替换 `tsx watch`**: 当前失败发生在它自己的 IPC 管道初始化阶段，不是业务代码。
- **为什么继续使用 `tsx`**: 只需要 TypeScript ESM 运行时，不需要它的专属 watch 守护。
- **为什么修 `.env` 路径**: 这是默认 workspace 启动下的真实配置缺陷，不修会导致根目录 `.env` 不生效。

### 7. 关键风险点
- **沙箱限制**: 当前环境禁止真实监听端口，只能验证脚本是否已进入业务启动阶段。
- **环境漂移**: 若 `.env` 继续跟随 `cwd`，根级和子包启动行为会不一致。
- **工具限制**: 仓库规范要求优先使用 `context7`、`github.search_code`、`desktop-commander`，当前执行环境未提供这些工具，本次改为基于本地源码与命令完成检索。

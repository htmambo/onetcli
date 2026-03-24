# sync_server

`sync_server` 是一个独立部署的轻量同步服务，默认使用 `SQLite` 作为数据库，同时提供：

- 同步服务 API
- 用户自助界面
- 管理界面

## 技术栈

- 后端：Node.js + Fastify + better-sqlite3 + TypeScript
- 前端：Vue 3 + Vite + TypeScript + Tailwind CSS
- 存储：SQLite

## 目录结构

```text
sync_server/
├── server/        # API 服务
├── web/           # Vue 管理与用户界面
├── data/          # SQLite 数据文件
├── .env.example   # 环境变量示例
└── package.json   # 独立项目工作区入口
```

## 主要能力

- 账号注册/登录
- Bearer Token 会话
- 用户同步密钥配置管理
- 同步数据增量读取、创建、更新、软删除
- 用户自助页面
- 管理员账号管理与同步数据清理

## 快速开始

### 1. 安装依赖

```bash
cd sync_server
npm install
```

### 2. 配置环境变量

```bash
cp .env.example .env
```

首次启动时会自动创建管理员账号：

- `SYNC_SERVER_ADMIN_EMAIL`
- `SYNC_SERVER_ADMIN_PASSWORD`

### 3. 开发模式

```bash
npm run dev
```

默认地址：

- 统一入口：`http://localhost:8787`
- Vite 内部开发服务：`http://localhost:5173`

说明：

- 开发模式下，浏览器只需要打开 `8787`
- Fastify 会把网页和 HMR 请求代理到内部的 Vite 端口
- API 和 Web 因此共用一个对外端口

### 4. 生产构建

```bash
npm run build
npm run start
```

生产模式下，后端会优先尝试托管 `web/dist` 中的前端静态文件。

## 默认数据库

默认数据库路径：

```text
./data/sync_server.db
```

可以通过 `SYNC_SERVER_DB_PATH` 覆盖。

## 主要 API

- `GET /health`
- `POST /api/v1/auth/register`
- `POST /api/v1/auth/login`
- `GET /api/v1/auth/me`
- `POST /api/v1/auth/logout`
- `GET /api/v1/sync/config`
- `PUT /api/v1/sync/config`
- `GET /api/v1/sync/items`
- `POST /api/v1/sync/items`
- `PUT /api/v1/sync/items/:id`
- `DELETE /api/v1/sync/items/:id`
- `GET /api/v1/admin/overview`
- `GET /api/v1/admin/users`
- `PATCH /api/v1/admin/users/:id`
- `DELETE /api/v1/admin/users/:id/data`

## 独立部署说明

`sync_server` 不加入当前仓库的 Rust workspace。

它是一个独立的 Node 项目，可以单独：

- 安装依赖
- 构建
- 运行
- 部署到服务器或容器

## 当前默认模型

- 一个账号对应一套云端同步状态
- 同步密钥与账号解耦
- 默认不做设备授权

## 后续可扩展方向

- 邮件验证码
- 找回密码
- 操作审计
- 更细粒度的同步管理

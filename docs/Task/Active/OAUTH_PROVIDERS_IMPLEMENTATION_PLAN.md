# OAuth Providers 实现计划

**状态**: 🔄 进行中 (开始时间: 2026-04-08)

## 目标

实现 GitHub Gist、Google Drive、Microsoft OneDrive 三个 OAuth 存储后端。

---

## OAuth 认证流程

### GitHub Device Flow（无需浏览器）
```
1. 请求 GitHub /device/code → 获取 device_code + user_code + verification_uri
2. 打开浏览器 / 显示验证 URI + user_code
3. 轮询 /device/code?grant_type=... 直到 user_code 被输入完成
4. 获取 access_token
```

### Google / OneDrive PKCE（需要浏览器 + 本地回调）
```
1. 生成 code_verifier (随机字符串)
2. 计算 code_challenge = BASE64URL(SHA256(code_verifier))
3. 打开浏览器到授权 URL（含 code_challenge）
4. 启动本地 HTTP server（随机端口，5分钟超时）监听回调
5. 收到回调后用 code_verifier 换 access_token
6. 关闭本地 server
```

---

## 子任务

### T1: OAuth 核心基础设施

**状态**: ⏳ 待开始

**内容**:
- 创建 `crates/core/src/cloud_sync/oauth/` 模块
- 定义 `OAuthTokens`、`OAuthConfig` 结构
- 实现 PKCE 辅助函数（generate_verifier、generate_challenge）
- 实现 GitHub Device Flow 客户端
- 实现通用 PKCE OAuth 客户端（Google/OneDrive 共用）
- 实现本地 HTTP 回调服务器（用于 PKCE）

**涉及文件**:
- `crates/core/src/cloud_sync/oauth/mod.rs`
- `crates/core/src/cloud_sync/oauth/pkce.rs`
- `crates/core/src/cloud_sync/oauth/github_device.rs`
- `crates/core/src/cloud_sync/oauth/pkce_flow.rs`
- `crates/core/src/cloud_sync/oauth/callback_server.rs`

**验收标准**: GitHub Device Flow 可完整执行获取 token

---

### T2: GitHub Gist Adapter

**状态**: ⏳ 待开始

**内容**:
- 实现 `GithubVault` struct
- 实现 `BlobVault` trait:
  - `upload` → PUT gist
  - `download` → GET gist
  - `delete` → DELETE gist
  - `exists` → GET gist (检查 404)
  - `list` → GET user's gists
- 存储：使用单个 gist 作为同步文件（文件名 `netcatty-vault.json`）
- `initializeSync` → 创建或定位 netcatty-vault gist

**GitHub API 端点**:
- `POST /gists` — 创建 gist
- `GET /gists/{gist_id}` — 获取 gist
- `PATCH /gists/{gist_id}` — 更新 gist
- `DELETE /gists/{gist_id}` — 删除 gist
- `GET /gists` — 列出用户 gists

**涉及文件**:
- `crates/core/src/cloud_sync/oauth/github_gist.rs`

---

### T3: Google Drive Adapter

**状态**: ⏳ 待开始

**内容**:
- 实现 `GoogleDriveVault` struct
- 实现 `BlobVault` trait:
  - `upload` → 上传到 AppDataFolder
  - `download` → 下载文件
  - `delete` → 删除文件
  - `exists` → 检查文件是否存在
  - `list` → 列举文件
- OAuth 端点：
  - 授权：`https://accounts.google.com/o/oauth2/v2/auth`
  - Token：`https://oauth2.googleapis.com/token`
  - API：`https://www.googleapis.com/drive/v3/`
- 存储：应用专属文件夹（AppDataFolder，无需用户交互选择）

**涉及文件**:
- `crates/core/src/cloud_sync/oauth/google_drive.rs`

---

### T4: Microsoft OneDrive Adapter

**状态**: ⏳ 待开始

**内容**:
- 实现 `OneDriveVault` struct
- 实现 `BlobVault` trait:
  - `upload` → 上传到用户 OneDrive 根目录
  - `download` → 下载文件
  - `delete` → 删除文件
  - `exists` → 检查文件是否存在
  - `list` → 列举文件
- OAuth 端点：
  - 授权：`https://login.microsoftonline.com/common/oauth2/v2.0/authorize`
  - Token：`https://login.microsoftonline.com/common/oauth2/v2.0/token`
  - API：`https://graph.microsoft.com/v1.0/me/drive`
- 存储：用户 OneDrive 根目录下的 `netcatty-vault` 文件夹

**涉及文件**:
- `crates/core/src/cloud_sync/oauth/onedrive.rs`

---

### T5: 设置界面集成

**状态**: ⏳ 待开始

**内容**:
- `AppSettings` 添加 `GithubSettings`、`GoogleDriveSettings`、`OneDriveSettings`
- 设置界面添加三个后端选项：
  - GitHub Gist → client_id 输入
  - Google Drive → client_id + client_secret 输入
  - OneDrive → client_id + client_secret 输入
- OAuth 登录按钮触发认证流程
- 登录后显示账号信息

**涉及文件**:
- `main/src/setting_tab.rs`
- `crates/core/src/cloud_sync/mod.rs`

---

## 实施顺序

```
T1 (核心基础设施) → T2 (GitHub) → T3 (Google) → T4 (OneDrive) → T5 (UI)
```

GitHub Device Flow 最简单，先实现作为参考。PKCE Flow 通用，可复用。

---

## 风险评估

| 风险 | 级别 | 缓解措施 |
|------|------|---------|
| GitHub Device Flow client_id 安全 | 低 | 不存储 secret，只用 public client_id |
| PKCE 本地回调端口冲突 | 中 | 使用随机端口，超时自动清理 |
| Token 刷新时机 | 中 | 各 adapter 自己管理 refresh |
| 敏感信息（client_secret）存储 | 高 | 复用 onetcli 现有 key_storage 机制 |

---

## 依赖

- `tiny_http` — 本地 OAuth 回调服务器（轻量，无需 async runtime）
- `sha2` — 已有（PKCE SHA256）
- `base64` — 已有

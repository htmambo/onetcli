# Netcatty 同步功能分析报告 — 引入 onetcli 可行性研究

**研究日期**：2026-04-08
**研究范围**：Netcatty 多云存储同步功能 + onetcli 现有云同步能力评估

---

## 一、Netcatty 同步架构概览

Netcatty 构建了一套**零知识加密多云存储同步**系统，核心特点：

```
用户数据 ──(AES-256-GCM 加密)──► 公共云存储
                                   ├─ GitHub Gist
                                   ├─ Google Drive
                                   ├─ Microsoft OneDrive
                                   ├─ WebDAV (自建服务器)
                                   └─ S3 兼容存储
```

### 1.1 核心文件清单

```
Netcatty/
├── domain/
│   ├── sync.ts              # 同步域类型、状态机、常量
│   └── syncMerge.ts          # 三路合并算法
├── infrastructure/
│   ├── services/
│   │   ├── CloudSyncManager.ts  # 同步中枢管理器
│   │   └── adapters/
│   │       ├── index.ts         # Adapter 工厂 (createAdapter)
│   │       ├── GitHubAdapter.ts  # GitHub Gist
│   │       ├── GoogleDriveAdapter.ts
│   │       ├── OneDriveAdapter.ts
│   │       ├── WebDAVAdapter.ts  # 基于 webdav npm 库
│   │       └── S3Adapter.ts     # 基于 AWS SDK v3
│   └── persistence/
│       └── localStorageAdapter.ts
├── electron/bridges/
│   ├── cloudSyncBridge.cjs     # Electron IPC
│   └── onedriveAuthBridge.cjs  # OneDrive OAuth
└── components/
    ├── CloudSyncSettings.tsx   # 设置 UI
    ├── SyncStatusButton.tsx     # 状态按钮
    └── settings/tabs/
        └── SettingsSyncTab.tsx  # 同步设置页
```

### 1.2 CloudAdapter 统一接口

```typescript
export interface CloudAdapter {
  readonly isAuthenticated: boolean;
  readonly accountInfo: ProviderAccount | null;
  readonly resourceId: string | null;

  signOut(): void;
  initializeSync(): Promise<string | null>;  // 创建/定位同步文件
  upload(syncedFile: SyncedFile): Promise<string>;  // 上传加密数据
  download(): Promise<SyncedFile | null>;  // 下载并解密
  deleteSync(): Promise<void>;
  getTokens(): OAuthTokens | null;
}
```

通过 `createAdapter(provider, tokens?, resourceId?, config?)` 工厂函数创建对应 provider 的 adapter。

### 1.3 支持的 Provider

| Provider | 认证方式 | 库/SDK | 存储位置 |
|-----------|---------|---------|----------|
| **GitHub Gist** | OAuth Device Flow | REST API | `netcatty-vault.json` gist |
| **Google Drive** | OAuth PKCE | REST API | 应用专属文件夹 |
| **OneDrive** | OAuth PKCE | Microsoft Graph API | 应用专属文件夹 |
| **WebDAV** | Basic/Digest/Token | `webdav` npm | 指定路径 |
| **S3** | AccessKey+SecretKey | AWS SDK v3 | 指定 bucket/prefix |

### 1.4 安全设计

**零知识加密**（Zero-Knowledge）：
- 数据在客户端加密（AES-256-GCM），服务端永远不接触明文
- Master Key 存储在 Electron safeStorage，永不上传
- `key_verification` 用于验证主密钥正确性，不含密钥本身
- 加密参数（iv, salt, algorithm, kdf）存储在 `SyncFileMeta` 中

### 1.5 三路合并算法（syncMerge.ts）

```typescript
// 对于每个实体 ID:
纯新增 → 保留
纯删除 → 删除
两边都改 → 优先本地（记录冲突）
一方改一方删 → 保留修改（安全优先）
```

- 使用 JSON fingerprint 进行内容比对（递归键排序）
- 特殊处理：按 (hostname, port, keyType) 去重 known hosts
- 冲突时倾向本地版本（本地修改优先）

### 1.6 SyncPayload 内容

```typescript
interface SyncPayload {
  hosts: Host[];           // SSH/终端连接
  keys: SSHKey[];         // SSH 密钥
  identities?: Identity[]; // 身份
  snippets: Snippet[];     // 代码片段
  customGroups: string[]; // 自定义分组
  knownHosts?: KnownHost[];
  portForwardingRules?: PortForwardingRule[];
  groupConfigs?: GroupConfig[];
  settings?: {             // 全局设置
    theme?, terminalTheme?, sftp*, keyboard*, ...
  };
}
```

所有内容加密为单个 JSON blob，存储到云端。

---

## 二、onetcli 现有云同步能力

### 2.1 架构概览

onetcli 已有一套**基于云端同步服务器**的同步系统：

```
本地 SQLite ──(加密)──► sync_server (REST API) ──► 团队共享
                              │
                              └─ 个人云端备份
```

### 2.2 核心模块（crates/core/src/cloud_sync/）

| 文件 | 作用 |
|------|------|
| `models.rs` | 同步数据模型：CloudSyncData, SyncStatus, SyncPlan, CloudApiError |
| `client.rs` | 云端 API 客户端 trait（依赖 sync_server） |
| `engine.rs` | 同步引擎：执行 SyncPlan |
| `connection_sync.rs` | 连接级别的同步逻辑 |
| `workspace_sync.rs` | 工作空间同步 |
| `certificate_sync.rs` | 证书同步 |
| `generic_sync.rs` | 通用同步逻辑 |
| `sync_server.rs` | sync_server REST API 客户端实现 |
| `conflict.rs` | 冲突检测与解决 |
| `queue.rs` | 同步队列 |
| `state_manager.rs` | 同步状态管理 |

### 2.3 现有同步模型

**CloudSyncData**（统一加密 blob）：
```rust
pub struct CloudSyncData {
    pub id: String,
    pub data_type: String,  // "connection" | "workspace" | "certificate"
    pub name: String,         // 明文名称（便于展示）
    pub encrypted_data: String, // base64(nonce + AES-GCM ciphertext)
    pub key_version: u32,
    pub checksum: String,      // SHA-256 明文校验和
    pub version: u32,          // 乐观并发控制
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}
```

**SyncPlan**（操作计划）：
```rust
pub struct SyncPlan {
    pub to_upload: Vec<StoredConnection>,
    pub to_update_cloud: Vec<(StoredConnection, CloudSyncData)>,
    pub to_download: Vec<CloudSyncData>,
    pub to_update_local: Vec<(CloudSyncData, StoredConnection)>,
    pub to_delete_cloud: Vec<String>,
    pub to_delete_local: Vec<i64>,
    pub conflicts: Vec<SyncConflict>,
}
```

### 2.4 与 Netcatty 的关键差异

| 维度 | Netcatty | onetcli |
|------|----------|---------|
| **存储后端** | 公共云存储（无自建服务器） | 依赖 sync_server |
| **多 Provider** | WebDAV/S3/GitHub/GDrive/OneDrive | 仅 sync_server |
| **数据格式** | 单个加密 JSON blob | 多条 CloudSyncData 记录 |
| **合并策略** | 实体级三路合并 | 版本号 + checksum 检测 |
| **冲突解决** | USE_LOCAL / USE_REMOTE / AUTO_MERGED | KeepBoth（保留副本） |
| **认证** | 各 Provider 独立 OAuth | 邮箱+密码（sync_server） |
| **团队支持** | 无（个人存储） | 有（team_id 归属） |

---

## 三、引入可行性评估

### 3.1 技术架构对比

**根本差异**：onetcli 的同步是**中心化**的（依赖 sync_server），Netcatty 是**去中心化**的（无自建服务器）。

这意味着：
- Netcatty 的 adapter 架构**不能直接移植**
- 但**可以借鉴适配层设计**，在 onetcli 的现有架构上扩展

### 3.2 各项功能引入评估

| 功能 | 可行性 | 工作量 | 推荐优先级 | 原因 |
|------|--------|--------|------------|------|
| **WebDAV Adapter** | 中 | 2-3周 | P1 | 有成熟 Rust 库（reqwest + 手写 WebDAV），可复用 onetcli 现有 reqwest_client |
| **S3 Adapter** | 中 | 2-3周 | P1 | AWS SDK for Rust (`aws-sdk-s3`) 成熟，与 onetcli 现有加密基础设施兼容 |
| **CloudAdapter Trait** | 高 | 1周 | P0 | 统一接口设计，可作为 onetcli 同步架构的扩展点 |
| **三路合并算法** | 高 | 1周 | P0 | Rust 实现简单，纯算法移植，无外部依赖 |
| **GitHub Gist Adapter** | 中 | 2周 | P2 | OAuth Device Flow + Gist API，有 Rust 库（reqwest） |
| **Google Drive / OneDrive** | 低 | 3-4周 | P2 | OAuth2 + Drive/Graph API 实现复杂，收益有限 |
| **Auto-sync 定时器** | 高 | 3-5天 | P0 | 借鉴 Netcatty 的 DEFAULT_AUTO_SYNC_INTERVAL 机制 |
| **SyncStatusButton UI** | 中 | 1周 | P1 | 借鉴 Netcatty 的状态按钮设计 |

### 3.3 实施建议

**短期（1-2个月）— P0**

1. **提取 CloudAdapter Trait**（1周）
   - 在 `cloud_sync` 模块中定义统一的 `CloudStorageAdapter` trait
   - 抽象 `upload()`, `download()`, `deleteSync()`, `initialize()`, `getAccount()`
   - 迁移现有 sync_server 为 `SyncServerAdapter` 实现该 trait

2. **三路合并算法 Rust 实现**（1周）
   - 将 Netcatty 的 `syncMerge.ts` 移植为 Rust
   - 基于 `fingerprint` 函数实现内容比对
   - 集成到 onetcli 的 `conflict.rs`

3. **Auto-sync 定时器**（3-5天）
   - 在 `SyncStateManager` 中添加可配置的同步间隔
   - 参考 Netcatty 的 DEFAULT_AUTO_SYNC_INTERVAL = 5min

**中期（3-4个月）— P1**

1. **WebDAV Adapter**（2-3周）
   - 使用 `reqwest` 或专用 WebDAV 库
   - 支持 Basic/Digest/Token 认证
   - 复用 onetcli 现有的加密/解密逻辑

2. **S3 Adapter**（2-3周）
   - 使用 `aws-sdk-s3` Rust SDK
   - 支持任意 S3 兼容存储（MinIO、COS、OBS 等）
   - 利用 onetcli 现有的 `aes-gcm` 加密

3. **SyncStatusButton UI**（1周）
   - 借鉴 Netcatty 的 `SyncStatusButton.tsx`
   - 显示连接状态、同步进度、冲突提示

**长期（6个月+）— P2**

1. GitHub Gist Adapter（2周）
2. Google Drive / OneDrive Adapter（3-4周）

---

## 四、关键风险

| 风险 | 级别 | 缓解措施 |
|------|------|----------|
| 现有 sync_server 与多 Provider 架构冲突 | 高 | CloudAdapter trait 作为统一抽象，两者并存 |
| WebDAV/S3 认证凭据安全存储 | 中 | 复用 onetcli 现有的 key_storage 机制 |
| 三路合并性能（大量实体） | 低 | 按 entity ID 分批合并，避免全量加载 |
| 多 Provider 并发同步复杂性 | 中 | CloudSyncManager 串行化同步操作 |
| OAuth token 刷新机制 | 中 | 各 adapter 自己管理 token 生命周期 |

---

## 五、结论

Netcatty 的多云存储同步**不能直接移植**——其 adapter 架构基于 Electron + npm 库，存储后端完全不同。但以下方面**值得借鉴**：

1. **CloudAdapter 统一接口模式** — 高价值，作为 onetcli 同步架构的扩展点
2. **三路合并算法** — 高价值，Rust 实现简单，增强冲突解决能力
3. **零知识加密设计** — onetcli 已具备（Master Key + AES-GCM）
4. **WebDAV/S3 作为公共存储后端** — 中等价值，扩大用户选择

**Netcatty 的最大价值**：证明了"无自建服务器"同步的可行性，onetcli 可在此方向上探索。

**核心建议**：不替换现有的 sync_server 架构，而是将其作为第一个 CloudAdapter 实现，在此基础上扩展 WebDAV 和 S3 支持。

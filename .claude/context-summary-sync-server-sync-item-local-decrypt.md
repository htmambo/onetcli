## 项目上下文摘要（sync-server-sync-item-local-decrypt）
生成时间：2026-03-25 16:06:01 +0800

### 1. 相似实现分析
- **实现1**: `sync_server/web/src/views/user/SyncItemDetailView.vue:1`
  - 模式：详情页使用 `watch(..., { immediate: true })` 监听路由参数并加载数据
  - 可复用：页面结构、错误态/加载态、右侧信息卡片区域
  - 需注意：当前仅展示 `encryptedData` 原文，没有解密表单和辅助状态

- **实现2**: `sync_server/web/src/views/user/ProfileView.vue:1`
  - 模式：表单交互使用 `ref` 维护输入值、提交状态、成功提示和错误提示
  - 可复用：密码输入框样式、提交按钮禁用态、消息提示块样式
  - 需注意：本次主密钥只允许浏览器内存短暂使用，不能持久化

- **实现3**: `sync_server/web/src/views/user/DashboardView.vue:32`
  - 模式：同步配置页面已展示 `keyVerification` 与 `keyVersion`，说明前端已有读取同步配置的能力
  - 可复用：`api.getSyncConfig()`、同步密钥配置的说明文案和状态徽标样式
  - 需注意：`keyVerification` 只能用于校验主密钥，不是明文主密钥

- **实现4**: `crates/core/src/crypto.rs:110`
  - 模式：Rust 端通过 `SHA-256(master_key + 固定 salt)` 派生 32 字节密钥，再用 `AES-256-GCM` 解密
  - 可复用：固定盐值 `onehub_password_encryption_salt_v1`、前缀 `ENC:`、验证魔术串 `ONEHUB_KEY_VERIFY_V1`
  - 需注意：前端必须严格复刻同样算法，否则无法解密现有密文

- **实现5**: `crates/core/src/cloud_sync/service.rs:324`
  - 模式：云同步数据是“整段 JSON 明文”整体加密上传，解密后再反序列化
  - 可复用：解密结果应按 JSON 优先格式化展示
  - 需注意：不同 `dataType` 的明文结构不同，但统一都是 JSON 字符串

### 2. 项目约定
- **命名约定**: 页面状态使用语义化英文命名，如 `loading`、`errorMessage`、`masterKey`
- **文件组织**: 页面逻辑集中在对应 `View.vue`，通用逻辑可下沉到 `src/utils/`
- **导入顺序**: 先 Vue API，再项目内服务、类型、工具模块
- **代码风格**: 2 空格缩进，组合式 API，样式通过内联 Tailwind 类表达

### 3. 可复用组件清单
- `sync_server/web/src/views/user/SyncItemDetailView.vue`：详情页布局和数据加载入口
- `sync_server/web/src/views/user/ProfileView.vue`：表单输入、提交态和提示文案模式
- `sync_server/web/src/views/user/DashboardView.vue`：同步配置读取能力与 keyVerification 说明
- `sync_server/web/src/services/api.ts`：`getSyncItem()` 与 `getSyncConfig()` 接口
- `crates/core/src/crypto.rs`：前端需对齐的加解密算法定义
- `crates/core/src/cloud_sync/service.rs`：确认云端同步 payload 的明文就是 JSON 字符串

### 4. 测试策略
- **测试框架**: 当前 `sync_server/web` 未配置单元测试
- **测试模式**: 使用 `npm --prefix sync_server/web run build` 做类型检查和生产构建验证
- **参考文件**: `sync_server/web/package.json`
- **覆盖要求**: 验证新增 Web Crypto 工具、详情页表单状态、模板绑定和类型导入不影响构建

### 5. 依赖和集成点
- **外部依赖**: Vue 3.5、Vue Router 4、浏览器 `Web Crypto API`
- **内部依赖**: `api.getSyncItem()`、`api.getSyncConfig()`、`getSyncItemTypeLabel()`、版本格式化工具
- **集成方式**: 仅在浏览器本地使用主密钥校验和解密，主密钥不发送到后端
- **配置来源**: `keyVerification` 来自 `/api/v1/sync/config`，密文来自 `/api/v1/sync/items/:id`

### 6. 技术选型理由
- **为什么用这个方案**: 服务端按设计不能解密，正确实现方式只能是“用户手动输入主密钥 -> 浏览器本地校验/解密”
- **优势**: 保持端到端加密边界不变，不需要改后端接口或暴露主密钥
- **劣势和风险**: 前端必须严格复刻 Rust 算法；浏览器不支持 `Web Crypto` 时无法使用

### 7. 关键风险点
- **边界条件**: 没有同步配置、主密钥错误、密文格式损坏、解密后不是 JSON，都要给出明确反馈
- **安全边界**: 主密钥只能放在内存 `ref` 中，不能写入 localStorage / sessionStorage
- **性能瓶颈**: 单条记录的解密是轻量操作，性能影响可忽略
- **工具限制**: 规范要求优先使用 `sequential-thinking`、`desktop-commander`、`context7`、`github.search_code`，当前执行环境未提供这些工具，本次改为基于仓库源码、`rg` 和本地 Node 构建命令完成检索与验证

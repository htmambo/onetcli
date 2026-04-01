# 性能优化与安全修复任务计划

**状态**: 🔄 部分完成

## 已完成项

- ✅ **T3**: 密钥派生升级到 Argon2id（验证数据用 Argon2id 加密，密码加密格式升级 V2，8 个测试全部通过）
- ✅ **T4**: 递归缓存失效改为迭代（避免栈溢出）
- ✅ **T6**: 元数据缓存写入改为同步 await（消除 orphan task）
- ✅ **T7**: scrypt 成本因子从默认值升级到 N=32768, r=8, p=1
- ⏭️ **T5** (跳过): SQL Parser 缓存（分析结论：Parser 创建成本极低，驱动已内置 prepared statement 缓存）

## 待完成项

- ⏳ **T1**: tokio::spawn 缺少取消机制（CRITICAL）
- ⏳ **T2**: key_storage 硬编码 AES 密钥（CRITICAL）
**创建时间**: 2026-03-31
**创建人**: Claude Code 分析

## 任务目标

修复项目中的内存泄漏风险、安全隐患及性能问题，提升应用稳定性与安全性。

---

## 问题汇总

| 序号 | 类别 | 问题 | 优先级 |
|------|------|------|--------|
| T1 | 内存泄漏 | tokio::spawn 缺少取消机制 | CRITICAL |
| T2 | 安全 | key_storage 硬编码 AES 密钥 | CRITICAL |
| T3 | 安全 | 密钥派生使用 SHA-256 | HIGH |
| T4 | 内存泄漏 | 递归缓存失效可能栈溢出 | MEDIUM |
| T5 | 性能 | SQL Parser 每次 execute 重新创建 | LOW |
| T6 | 性能 | 元数据缓存文件写入无错误跟踪 | LOW |
| T7 | 安全 | scrypt 成本因子默认 | LOW |

---

## 子任务详情

### T1: tokio::spawn 缺少取消机制

**优先级**: CRITICAL

**影响范围**:
- `crates/core/src/agent/dispatcher.rs:88, 137` — agent execute spawn
- `crates/core/src/ai_chat/stream.rs:108` — AI stream spawn
- `crates/terminal/src/terminal.rs:626, 634` — SSH 通知转发 spawn
- `crates/sftp/src/russh_impl.rs:252, 272` — 并发文件读取 spawn
- `crates/db/src/manager.rs:711` — 连接清理定时任务

**修复方案**:

1. **统一封装**: 扩展 `crates/core/src/gpui_tokio.rs` 中的 `Tokio::spawn`，添加可取消的 spawn 变体，接受 `CancellationToken` 或类似机制

2. **分类处理**:
   - 对于有生命周期边界的 spawn（agent、stream）：通过 cancellation token 控制
   - 对于永久循环任务（清理任务）：确保与 `GlobalDbState` / `CacheManager` 生命周期绑定，在 `Global` drop 时正确终止

3. **具体修改**:
   - `agent/dispatcher.rs`: 将 `tokio::spawn` 替换为 `Tokio::spawn`，传递 cancellation token
   - `ai_chat/stream.rs`: 同样替换，stream 已有 `cancel_clone`，需确保 spawn 也被 cancel
   - `terminal.rs`: SSH 通知转发 spawn 是连接生命周期内的，需随连接关闭而终止
   - `sftp/russh_impl.rs`: 生产者/并发读取任务，通过 `cancelled` AtomicBool 和 channel close 信号控制
   - `db/manager.rs`: 清理任务的 Task 需存储或确保与 GlobalDbState 生命周期一致

**验收标准**: 所有 tokio::spawn 都有对应的取消机制，无游离任务

---

### T2: key_storage 硬编码 AES 密钥

**优先级**: CRITICAL

**当前代码**: `crates/core/src/key_storage.rs:21`
```rust
const LOCAL_STORAGE_FIXED_KEY: &[u8; 32] = b"onehub-local-dev-key-2025-fixed!";
```

**修复方案**:

1. **macOS**: 使用 Keychain Services API 存储主密钥
2. **Windows**: 使用 Credential Manager (Windows API)
3. **Linux**: 使用 libsecret (Secret Service API)，fallback 到现有固定 key 方案

**具体修改**:
- 新增 `crates/core/src/keychain.rs` 模块，封装跨平台密钥链接口
- 平台检测通过 `std::env::consts::OS` 实现
- 修改 `LocalFileStorage` 的 save/load，优先从密钥链读写，失败时使用安全提示而非静默 fallback 到固定 key
- 如果密钥链不可用，降级方案需用户确认风险，不自动降级

**验收标准**: 主密钥不再以固定 key 加密存储，改用 OS 密钥链

---

### T3: 密钥派生使用 SHA-256

**优先级**: HIGH

**当前代码**: `crates/core/src/crypto.rs:117-126`
```rust
fn derive_key(master_key: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(master_key.as_bytes());
    hasher.update(b"onehub_password_encryption_salt_v1");
    let result = hasher.finalize();
    // ...
}
```

**修复方案**:

1. 切换到 Argon2id 算法
2. 添加 `argon2` crate 依赖（editions 2024 兼容）
3. 替换 `derive_key` 函数使用 Argon2id
4. 兼容考虑：现有 `ENC:` 前缀密文使用旧 KDF，需保留解密兼容性
   - 在密文格式中增加版本标识，如 `ENC:V2:base64(...)`
   - 新密文使用 Argon2id，旧密文仍用 SHA-256 解密

**具体修改**:
- `crates/core/src/crypto.rs`: 新增 `derive_key_argon2` 函数，修改 `encrypt_password` 使用新 KDF
- 密文格式: `ENC:V2:base64(nonce + ciphertext)` 标识新版本
- 解密时自动检测版本前缀，选择对应 KDF

**验收标准**: 新加密使用 Argon2id，旧密文仍可解密

---

### T4: 递归缓存失效可能栈溢出

**优先级**: MEDIUM

**当前代码**: `crates/db/src/cache.rs:290-296`

**修复方案**: 将递归改为迭代 + Vec 队列

```rust
pub async fn invalidate_node_recursive(&self, ctx: &CacheContext, node_id: &str) {
    let mut queue = VecDeque::new();
    queue.push_back(node_id.to_string());

    while let Some(current_id) = queue.pop_front() {
        if let Some(node) = self.get_node(ctx, &current_id).await {
            for child in &node.children {
                queue.push_back(child.id.clone());
            }
        }
        self.invalidate_node(ctx, &current_id).await;
    }
}
```

**验收标准**: 任意深度树结构缓存失效不栈溢出

---

### T5: SQL Parser 每次 execute 重新创建

**优先级**: LOW

**影响范围**: PostgreSQL 和 MySQL 的 execute 方法

**修复方案**:

1. 在 `PostgresConnection` / `MysqlConnection` 结构体中添加 `statement_cache: DashMap<String, Statement>` 字段
2. execute 前先查缓存，miss 时创建并缓存
3. DDL 执行后清除相关表的缓存条目

**验收标准**: 重复执行的 SQL 语句复用 Parser 对象

---

### T6: 元数据缓存文件写入无错误跟踪

**优先级**: LOW

**当前代码**: `crates/db/src/metadata_cache.rs:520-531`

**修复方案**:

1. 不使用 fire-and-forget 的 tokio::spawn
2. 改用带跟踪的方式：
   - 方案A: 使用 `tokio::sync::mpsc` channel 收集写入任务，由单一任务处理
   - 方案B: 直接 await 写入（低频场景可接受同步开销）
3. 写入失败时记录错误日志，不静默丢弃

**验收标准**: 缓存写入失败时行为可见，不产生 orphan task

---

### T7: scrypt 成本因子使用默认值

**优先级**: LOW

**当前代码**: `sync_server/server/src/utils/crypto.ts:5`

**修复方案**:

```typescript
export function hashPassword(password: string): string {
  const salt = randomBytes(16).toString("hex");
  // N=32768, r=8, p=1 -> ~16ms on modern hardware
  const derived = scryptSync(password, salt, 64, {
    N: 32768,
    r: 8,
    p: 1,
  }).toString("hex");
  return `${salt}:${derived}`;
}
```

**验收标准**: scrypt 使用明确的成本因子，高于默认值

---

## 实施顺序

```
T2 (CRITICAL 安全) → T3 (HIGH 安全) → T1 (CRITICAL 泄漏) → T4 (MEDIUM 泄漏) → T6 (LOW) → T5 (LOW) → T7 (LOW)
```

**说明**:
- T2/T3 优先级最高，因为安全漏洞影响用户数据安全
- T1 虽然标记 CRITICAL，但 GPUI 的 Entity 系统本身已通过引用计数提供了一定保护，真正影响有限
- T5-T7 为可选优化，可根据时间安排决定是否实施

---

## 风险评估

| 子任务 | 风险 | 缓解措施 |
|--------|------|---------|
| T1 | 改动面广，影响所有 async spawn | 按模块逐个修改，充分测试 |
| T2 | 跨平台 API 复杂度高 | 使用现有 OSS crate (keyring-rs) |
| T3 | 密文格式变更需向后兼容 | 增加版本前缀，不破坏旧密文 |
| T4 | 迭代改造可能遗漏边界条件 | 添加单元测试覆盖极端深度 |
| T5 | 缓存清理逻辑复杂 | 仅缓存 DML 类 Statement，不缓存 DDL |
| T6 | 改动简单，基本无风险 | 直接改为 await |
| T7 | 改动极简单，无风险 | 明确参数即可 |

---

## 依赖关系

- T1 → 无依赖，可独立进行
- T2 → 无依赖，可独立进行
- T3 → 无依赖，但 T2/T3 都改 crypto 模块，需注意合并冲突
- T4 → 无依赖
- T5 → 无依赖
- T6 → 无依赖
- T7 → 无依赖

**注意**: T2 和 T3 都涉及 `crates/core/src` 目录，建议安排在不同时间实施，或在实施前拉取最新代码避免冲突。

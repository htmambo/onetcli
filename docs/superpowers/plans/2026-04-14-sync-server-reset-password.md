# Sync Server Reset Password Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 `sync_server/server` 增加一个服务器本地 CLI，用登录邮箱重置指定用户密码，支持命令行传入新密码或交互式隐藏输入，并在成功后撤销该用户现有会话。

**Architecture:** 在 `src/scripts/reset-password.ts` 中实现一个独立入口，复用 `env`、`DatabaseClient`、`emailSchema/passwordSchema` 和 `hashPassword()`，不暴露任何新 HTTP API。数据库层补一个 `deleteSessionsByUserId()`，脚本层导出可测试的纯逻辑函数，测试通过 `node:test + tsx` 跑在源码模式下完成。

**Tech Stack:** TypeScript, Node.js ESM, `tsx`, `node:test`, `better-sqlite3`, `zod`

---

## File Structure

- Create: `sync_server/server/src/scripts/reset-password.ts`
- Create: `sync_server/server/src/scripts/reset-password.test.ts`
- Modify: `sync_server/server/src/db/database.ts`
- Modify: `sync_server/server/package.json`

---

### Task 1: 先写失败测试，锁定脚本行为

**Files:**
- Create: `sync_server/server/src/scripts/reset-password.test.ts`
- Test: `sync_server/server/src/scripts/reset-password.test.ts`

- [ ] **Step 1: 写失败测试文件**

```ts
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { env } from "../config/env.js";
import { DatabaseClient } from "../db/database.js";
import { hashPassword, sha256, verifyPassword } from "../utils/crypto.js";
import { plusHoursIso } from "../utils/time.js";
import { resetPasswordForEmail, resolveNewPassword } from "./reset-password.js";

function createTestDatabase() {
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "sync-server-reset-password-"));
  const dbPath = path.join(tempDir, "sync_server.db");
  const database = new DatabaseClient(dbPath, env.migrationsPath);

  return {
    database,
    cleanup() {
      database.close();
      fs.rmSync(tempDir, { recursive: true, force: true });
    },
  };
}

test("resolveNewPassword 优先使用命令行参数", async () => {
  const password = await resolveNewPassword("new-password-123", false, async () => {
    throw new Error("prompt should not be called");
  });

  assert.equal(password, "new-password-123");
});

test("resolveNewPassword 在非交互环境且没有密码参数时失败", async () => {
  await assert.rejects(
    () => resolveNewPassword(undefined, false, async () => "unused-password"),
    /非交互环境且未提供新密码/,
  );
});

test("resolveNewPassword 在交互模式下使用 prompt 返回值", async () => {
  const password = await resolveNewPassword(undefined, true, async () => "prompt-password-123");

  assert.equal(password, "prompt-password-123");
});

test("resetPasswordForEmail 更新密码并清理已有会话", () => {
  const { database, cleanup } = createTestDatabase();

  try {
    const user = database.createUser("user@example.com", hashPassword("old-password-123"));
    const tokenHashes = [sha256("token-1"), sha256("token-2")];
    database.createSession(user.id, tokenHashes[0], plusHoursIso(1));
    database.createSession(user.id, tokenHashes[1], plusHoursIso(1));

    const result = resetPasswordForEmail(database, "User@Example.com", "new-password-123");
    const updatedUser = database.findUserByEmail("user@example.com");

    assert.ok(updatedUser);
    assert.equal(result.email, "user@example.com");
    assert.equal(result.revokedSessions, 2);
    assert.equal(verifyPassword("new-password-123", updatedUser.password_hash), true);
    assert.equal(verifyPassword("old-password-123", updatedUser.password_hash), false);
    assert.equal(database.findSessionWithUserByTokenHash(tokenHashes[0]), null);
    assert.equal(database.findSessionWithUserByTokenHash(tokenHashes[1]), null);
  } finally {
    cleanup();
  }
});

test("resetPasswordForEmail 在用户不存在时失败", () => {
  const { database, cleanup } = createTestDatabase();

  try {
    assert.throws(
      () => resetPasswordForEmail(database, "missing@example.com", "new-password-123"),
      /用户不存在/,
    );
  } finally {
    cleanup();
  }
});
```

- [ ] **Step 2: 运行测试，确认它先失败**

Run:

```bash
cd sync_server/server
node --test --import tsx/esm src/scripts/reset-password.test.ts
```

Expected: FAIL，错误应指向 `./reset-password.js` 模块不存在，或 `resetPasswordForEmail` / `resolveNewPassword` 未导出。

- [ ] **Step 3: 实现最小生产代码让测试有机会通过**

在 `sync_server/server/src/db/database.ts` 新增删除指定用户会话的方法：

```ts
  deleteSessionsByUserId(userId: string): number {
    const result = this.db.prepare("DELETE FROM auth_sessions WHERE user_id = ?").run(userId);
    return result.changes;
  }
```

创建 `sync_server/server/src/scripts/reset-password.ts`：

```ts
import process from "node:process";
import readline from "node:readline";
import { pathToFileURL } from "node:url";

import { env } from "../config/env.js";
import { DatabaseClient } from "../db/database.js";
import { emailSchema, passwordSchema } from "../services/auth.js";
import { hashPassword } from "../utils/crypto.js";

type PasswordPrompt = (label: string) => Promise<string>;

export async function resolveNewPassword(
  argvPassword: string | undefined,
  isInteractive: boolean,
  prompt: PasswordPrompt,
): Promise<string> {
  if (argvPassword !== undefined) {
    return passwordSchema.parse(argvPassword);
  }

  if (!isInteractive) {
    throw new Error("非交互环境且未提供新密码，请通过命令行参数传入");
  }

  const promptedPassword = await prompt("请输入新密码: ");
  return passwordSchema.parse(promptedPassword);
}

export function resetPasswordForEmail(database: DatabaseClient, email: string, newPassword: string) {
  const normalizedEmail = emailSchema.parse(email);
  const validatedPassword = passwordSchema.parse(newPassword);
  const user = database.findUserByEmail(normalizedEmail);

  if (!user) {
    throw new Error("用户不存在");
  }

  database.updateUserPassword(user.id, hashPassword(validatedPassword));
  const revokedSessions = database.deleteSessionsByUserId(user.id);

  return {
    email: normalizedEmail,
    revokedSessions,
  };
}

export function promptHiddenPassword(label: string): Promise<string> {
  return new Promise((resolve, reject) => {
    if (!process.stdin.isTTY || !process.stdout.isTTY) {
      reject(new Error("当前终端不支持交互输入，请通过命令行参数提供新密码"));
      return;
    }

    process.stdout.write(label);
    const rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
      terminal: true,
    });

    (rl as readline.Interface & { _writeToOutput(value: string): void })._writeToOutput = () => {};
    rl.question("", (answer) => {
      rl.close();
      process.stdout.write("\n");
      resolve(answer);
    });
  });
}

export async function main(argv = process.argv.slice(2)) {
  const [email, argvPassword] = argv;

  if (!email) {
    console.error("用法: npm run reset-password -- <email> [newPassword]");
    process.exitCode = 1;
    return;
  }

  const database = new DatabaseClient(env.dbPath, env.migrationsPath);

  try {
    const newPassword = await resolveNewPassword(
      argvPassword,
      Boolean(process.stdin.isTTY),
      promptHiddenPassword,
    );
    const result = resetPasswordForEmail(database, email, newPassword);
    console.log(`密码已重置: ${result.email}，已撤销 ${result.revokedSessions} 个会话`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : "密码重置失败");
    process.exitCode = 1;
  } finally {
    database.close();
  }
}

const isEntrypoint = process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href;

if (isEntrypoint) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : "密码重置失败");
    process.exitCode = 1;
  });
}
```

- [ ] **Step 4: 重新运行测试，确认通过**

Run:

```bash
cd sync_server/server
node --test --import tsx/esm src/scripts/reset-password.test.ts
```

Expected: PASS，5/5 测试通过。

- [ ] **Step 5: 提交 Task 1**

```bash
git add sync_server/server/src/db/database.ts sync_server/server/src/scripts/reset-password.ts sync_server/server/src/scripts/reset-password.test.ts
git commit -m "feat: add sync_server reset-password script core"
```

---

### Task 2: 接入 npm 脚本入口并做 CLI 冒烟验证

**Files:**
- Modify: `sync_server/server/package.json`
- Modify: `sync_server/server/src/scripts/reset-password.ts`

- [ ] **Step 1: 在 package.json 增加脚本入口**

将 `sync_server/server/package.json` 的 `scripts` 更新为：

```json
{
  "scripts": {
    "dev": "node --import tsx/esm src/main.ts",
    "dev:watch": "node --watch --import tsx/esm src/main.ts",
    "build": "tsc -p tsconfig.json",
    "start": "node dist/main.js",
    "check": "tsc -p tsconfig.json --noEmit",
    "reset-password": "node --import tsx/esm src/scripts/reset-password.ts"
  }
}
```

- [ ] **Step 2: 运行 CLI 冒烟测试，确认无参数时给出用法错误**

Run:

```bash
cd sync_server/server
npm run reset-password --
```

Expected: 退出码非 0，输出 `用法: npm run reset-password -- <email> [newPassword]`。

- [ ] **Step 3: 运行类型检查**

Run:

```bash
cd sync_server/server
npm run check
```

Expected: PASS，无 TypeScript 错误。

- [ ] **Step 4: 提交 Task 2**

```bash
git add sync_server/server/package.json sync_server/server/src/scripts/reset-password.ts
git commit -m "chore: wire sync_server reset-password command"
```

---

### Task 3: 服务器本地做一次定向人工验证

**Files:**
- Verify: `sync_server/server/src/scripts/reset-password.ts`

- [ ] **Step 1: 用命令行参数方式重置一个测试账号密码**

Run:

```bash
cd sync_server/server
npm run reset-password -- user@example.com new-password-123
```

Expected: 输出 `密码已重置: user@example.com，已撤销 N 个会话`。

- [ ] **Step 2: 验证旧密码登录失败**

Run:

```bash
curl -X POST http://127.0.0.1:8787/api/v1/auth/login ^
  -H "Content-Type: application/json" ^
  -d "{\"email\":\"user@example.com\",\"password\":\"old-password-123\"}"
```

Expected: 返回 400，消息为 `邮箱或密码错误`。

- [ ] **Step 3: 验证新密码登录成功**

Run:

```bash
curl -X POST http://127.0.0.1:8787/api/v1/auth/login ^
  -H "Content-Type: application/json" ^
  -d "{\"email\":\"user@example.com\",\"password\":\"new-password-123\"}"
```

Expected: 返回 200，`data.user.email` 为 `user@example.com`。

- [ ] **Step 4: 验证已登录会话失效**

使用重置前缓存的旧 token 调用：

```bash
curl http://127.0.0.1:8787/api/v1/auth/me ^
  -H "Authorization: Bearer <old-token>"
```

Expected: 返回 401 或等价未认证响应。

- [ ] **Step 5: 提交 Task 3**

```bash
git add sync_server/server/package.json sync_server/server/src/db/database.ts sync_server/server/src/scripts/reset-password.ts sync_server/server/src/scripts/reset-password.test.ts
git commit -m "feat: add sync_server password reset maintenance flow"
```

---

### Task 4: 最终验证

- [ ] **Step 1: 重新运行脚本测试**

Run:

```bash
cd sync_server/server
node --test --import tsx/esm src/scripts/reset-password.test.ts
```

Expected: PASS，5/5 测试通过。

- [ ] **Step 2: 重新运行类型检查**

Run:

```bash
cd sync_server/server
npm run check
```

Expected: PASS。

- [ ] **Step 3: 检查最终变更**

Run:

```bash
git diff --stat -- sync_server/server/package.json sync_server/server/src/db/database.ts sync_server/server/src/scripts/reset-password.ts sync_server/server/src/scripts/reset-password.test.ts
```

Expected: 仅包含计划内 4 个文件。

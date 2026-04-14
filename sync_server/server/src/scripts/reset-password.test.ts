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

import fs from "node:fs";
import path from "node:path";
import Database from "better-sqlite3";
import type { AdminUserSummary, PublicUser, SessionRecord, SyncDataRecord, UserConfigRecord, UserRecord } from "../types/models.js";
import { createId } from "../utils/id.js";
import { nowIso } from "../utils/time.js";
import { runMigrations } from "./migrations.js";

type UserRole = "admin" | "user";
type UserStatus = "active" | "disabled";

export interface CreateSyncItemInput {
  id?: string;
  ownerId: string;
  dataType: string;
  encryptedData: string;
  keyVersion: number;
  checksum: string;
}

export interface UpdateSyncItemInput {
  id: string;
  ownerId: string;
  encryptedData: string;
  keyVersion: number;
  checksum: string;
  version: number;
  deletedAt?: string | null;
}

export interface ListSyncItemsInput {
  ownerId: string;
  dataType?: string;
  since?: string;
  includeDeleted?: boolean;
}

function toPublicUser(user: UserRecord): PublicUser {
  return {
    id: user.id,
    email: user.email,
    nickname: user.nickname,
    role: user.role,
    status: user.status,
    createdAt: user.created_at,
    updatedAt: user.updated_at,
    lastLoginAt: user.last_login_at,
  };
}

export class DatabaseClient {
  private readonly db: Database.Database;

  constructor(dbPath: string, migrationsPath: string) {
    fs.mkdirSync(path.dirname(dbPath), { recursive: true });
    this.db = new Database(dbPath);
    this.db.pragma("journal_mode = WAL");
    this.db.pragma("foreign_keys = ON");
    this.db.pragma("busy_timeout = 5000");
    runMigrations(this.db, migrationsPath);
  }

  createUser(
    email: string,
    passwordHash: string,
    role: UserRole = "user",
    status: UserStatus = "active",
    nickname: string = email,
  ): UserRecord {
    const now = nowIso();
    const user: UserRecord = {
      id: createId(),
      email,
      nickname,
      password_hash: passwordHash,
      role,
      status,
      created_at: now,
      updated_at: now,
      last_login_at: null,
    };

    this.db
      .prepare(
        `
        INSERT INTO users (id, email, nickname, password_hash, role, status, created_at, updated_at, last_login_at)
        VALUES (@id, @email, @nickname, @password_hash, @role, @status, @created_at, @updated_at, @last_login_at)
      `,
      )
      .run(user);

    return user;
  }

  findUserByEmail(email: string): UserRecord | null {
    return (this.db.prepare("SELECT * FROM users WHERE email = ?").get(email) as UserRecord | undefined) ?? null;
  }

  findUserById(id: string): UserRecord | null {
    return (this.db.prepare("SELECT * FROM users WHERE id = ?").get(id) as UserRecord | undefined) ?? null;
  }

  touchLastLogin(userId: string) {
    const now = nowIso();
    this.db.prepare("UPDATE users SET last_login_at = ?, updated_at = ? WHERE id = ?").run(now, now, userId);
  }

  updateUserPassword(userId: string, passwordHash: string) {
    const now = nowIso();
    this.db.prepare("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?").run(passwordHash, now, userId);
  }

  createSession(userId: string, tokenHash: string, expiresAt: string): SessionRecord {
    const now = nowIso();
    const session: SessionRecord = {
      id: createId(),
      user_id: userId,
      token_hash: tokenHash,
      expires_at: expiresAt,
      created_at: now,
      last_used_at: now,
    };

    this.db
      .prepare(
        `
        INSERT INTO auth_sessions (id, user_id, token_hash, expires_at, created_at, last_used_at)
        VALUES (@id, @user_id, @token_hash, @expires_at, @created_at, @last_used_at)
      `,
      )
      .run(session);

    return session;
  }

  findSessionWithUserByTokenHash(tokenHash: string): { session: SessionRecord; user: UserRecord } | null {
    const row =
      (this.db
        .prepare(
          `
          SELECT
            s.id as s_id,
            s.user_id as s_user_id,
            s.token_hash as s_token_hash,
            s.expires_at as s_expires_at,
            s.created_at as s_created_at,
            s.last_used_at as s_last_used_at,
            u.id as u_id,
            u.email as u_email,
            u.nickname as u_nickname,
            u.password_hash as u_password_hash,
            u.role as u_role,
            u.status as u_status,
            u.created_at as u_created_at,
            u.updated_at as u_updated_at,
            u.last_login_at as u_last_login_at
          FROM auth_sessions s
          JOIN users u ON u.id = s.user_id
          WHERE s.token_hash = ?
        `,
        )
        .get(tokenHash) as Record<string, string> | undefined) ?? null;

    if (!row) {
      return null;
    }

    return {
      session: {
        id: row.s_id,
        user_id: row.s_user_id,
        token_hash: row.s_token_hash,
        expires_at: row.s_expires_at,
        created_at: row.s_created_at,
        last_used_at: row.s_last_used_at,
      },
      user: {
        id: row.u_id,
        email: row.u_email,
        nickname: row.u_nickname,
        password_hash: row.u_password_hash,
        role: row.u_role as UserRole,
        status: row.u_status as UserStatus,
        created_at: row.u_created_at,
        updated_at: row.u_updated_at,
        last_login_at: row.u_last_login_at ?? null,
      },
    };
  }

  touchSession(tokenHash: string) {
    this.db.prepare("UPDATE auth_sessions SET last_used_at = ? WHERE token_hash = ?").run(nowIso(), tokenHash);
  }

  deleteSession(tokenHash: string) {
    this.db.prepare("DELETE FROM auth_sessions WHERE token_hash = ?").run(tokenHash);
  }

  deleteExpiredSessions() {
    this.db.prepare("DELETE FROM auth_sessions WHERE expires_at <= ?").run(nowIso());
  }

  upsertUserConfig(userId: string, keyVerification: string, keyVersion: number): UserConfigRecord {
    const now = nowIso();
    this.db
      .prepare(
        `
        INSERT INTO user_configs (user_id, key_verification, key_version, updated_at)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(user_id) DO UPDATE SET
          key_verification = excluded.key_verification,
          key_version = excluded.key_version,
          updated_at = excluded.updated_at
      `,
      )
      .run(userId, keyVerification, keyVersion, now);

    return this.getUserConfig(userId)!;
  }

  getUserConfig(userId: string): UserConfigRecord | null {
    return (this.db.prepare("SELECT * FROM user_configs WHERE user_id = ?").get(userId) as UserConfigRecord | undefined) ?? null;
  }

  clearUserConfig(userId: string) {
    this.db.prepare("DELETE FROM user_configs WHERE user_id = ?").run(userId);
  }

  listSyncItems(input: ListSyncItemsInput): SyncDataRecord[] {
    const conditions = ["owner_id = ?"];
    const values: Array<string | number> = [input.ownerId];

    if (input.dataType) {
      conditions.push("data_type = ?");
      values.push(input.dataType);
    }

    if (input.since) {
      conditions.push("updated_at >= ?");
      values.push(input.since);
    }

    if (!input.includeDeleted) {
      conditions.push("deleted_at IS NULL");
    }

    const sql = `
      SELECT *
      FROM sync_data
      WHERE ${conditions.join(" AND ")}
      ORDER BY updated_at DESC
    `;

    return this.db.prepare(sql).all(...values) as SyncDataRecord[];
  }

  createSyncItem(input: CreateSyncItemInput): SyncDataRecord {
    const now = nowIso();
    const id = input.id ?? createId();

    this.db
      .prepare(
        `
        INSERT INTO sync_data (
          id, owner_id, data_type, encrypted_data, key_version, checksum, version, created_at, updated_at, deleted_at
        ) VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, NULL)
      `,
      )
      .run(id, input.ownerId, input.dataType, input.encryptedData, input.keyVersion, input.checksum, now, now);

    return this.getSyncItem(input.ownerId, id)!;
  }

  getSyncItem(ownerId: string, id: string): SyncDataRecord | null {
    return (
      this.db.prepare("SELECT * FROM sync_data WHERE owner_id = ? AND id = ?").get(ownerId, id) as SyncDataRecord | undefined
    ) ?? null;
  }

  updateSyncItem(input: UpdateSyncItemInput): SyncDataRecord | null {
    const now = nowIso();
    const result = this.db
      .prepare(
        `
        UPDATE sync_data
        SET
          encrypted_data = ?,
          key_version = ?,
          checksum = ?,
          deleted_at = ?,
          version = version + 1,
          updated_at = ?
        WHERE owner_id = ? AND id = ? AND version = ?
      `,
      )
      .run(
        input.encryptedData,
        input.keyVersion,
        input.checksum,
        input.deletedAt ?? null,
        now,
        input.ownerId,
        input.id,
        input.version,
      );

    if (result.changes === 0) {
      return null;
    }

    return this.getSyncItem(input.ownerId, input.id);
  }

  softDeleteSyncItem(ownerId: string, id: string, version?: number): SyncDataRecord | null {
    const now = nowIso();
    const sql =
      version === undefined
        ? `
          UPDATE sync_data
          SET deleted_at = ?, version = version + 1, updated_at = ?
          WHERE owner_id = ? AND id = ?
        `
        : `
          UPDATE sync_data
          SET deleted_at = ?, version = version + 1, updated_at = ?
          WHERE owner_id = ? AND id = ? AND version = ?
        `;

    const args =
      version === undefined ? [now, now, ownerId, id] : [now, now, ownerId, id, version];

    const result = this.db.prepare(sql).run(...args);
    if (result.changes === 0) {
      return null;
    }

    return this.getSyncItem(ownerId, id);
  }

  clearSyncData(ownerId: string) {
    this.db.prepare("DELETE FROM sync_data WHERE owner_id = ?").run(ownerId);
  }

  listUsers(): AdminUserSummary[] {
    const rows = this.db
      .prepare(
        `
        SELECT
          u.*,
          COUNT(s.id) AS sync_item_count,
          CASE WHEN c.user_id IS NULL THEN 0 ELSE 1 END AS has_sync_config
        FROM users u
        LEFT JOIN sync_data s ON s.owner_id = u.id
        LEFT JOIN user_configs c ON c.user_id = u.id
        GROUP BY u.id
        ORDER BY u.created_at DESC
      `,
      )
      .all() as Array<UserRecord & { sync_item_count: number; has_sync_config: number }>;

    return rows.map((row) => ({
      ...toPublicUser(row),
      syncItemCount: row.sync_item_count,
      hasSyncConfig: row.has_sync_config === 1,
    }));
  }

  countOverview() {
    const users = this.db.prepare("SELECT COUNT(*) AS count FROM users").get() as { count: number };
    const activeUsers = this.db.prepare("SELECT COUNT(*) AS count FROM users WHERE status = 'active'").get() as {
      count: number;
    };
    const syncItems = this.db.prepare("SELECT COUNT(*) AS count FROM sync_data").get() as { count: number };
    const syncConfigs = this.db.prepare("SELECT COUNT(*) AS count FROM user_configs").get() as { count: number };

    return {
      users: users.count,
      activeUsers: activeUsers.count,
      syncItems: syncItems.count,
      syncConfigs: syncConfigs.count,
    };
  }

  updateUser(userId: string, patch: { role?: UserRole; status?: UserStatus; nickname?: string }): PublicUser | null {
    const updates: string[] = [];
    const values: Array<string> = [];

    if (patch.role) {
      updates.push("role = ?");
      values.push(patch.role);
    }

    if (patch.status) {
      updates.push("status = ?");
      values.push(patch.status);
    }

    if (patch.nickname !== undefined) {
      updates.push("nickname = ?");
      values.push(patch.nickname);
    }

    if (updates.length === 0) {
      const current = this.findUserById(userId);
      return current ? toPublicUser(current) : null;
    }

    updates.push("updated_at = ?");
    values.push(nowIso());
    values.push(userId);

    this.db.prepare(`UPDATE users SET ${updates.join(", ")} WHERE id = ?`).run(...values);
    const updated = this.findUserById(userId);
    return updated ? toPublicUser(updated) : null;
  }

  purgeUserData(userId: string) {
    this.clearSyncData(userId);
    this.clearUserConfig(userId);
  }

  ensureAdmin(email: string, passwordHash: string) {
    const normalizedEmail = email.trim().toLowerCase();
    const existing = this.findUserByEmail(normalizedEmail);

    if (!existing) {
      this.createUser(normalizedEmail, passwordHash, "admin", "active");
      return;
    }

    this.db
      .prepare("UPDATE users SET role = 'admin', status = 'active', updated_at = ? WHERE id = ?")
      .run(nowIso(), existing.id);
  }

  close() {
    this.db.close();
  }
}

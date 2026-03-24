import { z } from "zod";
import { DatabaseClient } from "../db/database.js";
import type { PublicUser } from "../types/models.js";
import { createOpaqueToken, hashPassword, sha256, verifyPassword } from "../utils/crypto.js";
import { plusHoursIso } from "../utils/time.js";

export const emailSchema = z.string().trim().min(3).max(255).email().transform((value) => value.toLowerCase());
export const passwordSchema = z.string().min(8).max(128);

export interface AuthSessionPayload {
  token: string;
  refreshToken: string;
  expiresAt: string;
  user: PublicUser;
}

export class AuthService {
  constructor(private readonly database: DatabaseClient, private readonly sessionTtlHours: number) {}

  register(email: string, password: string): AuthSessionPayload {
    const normalizedEmail = emailSchema.parse(email);
    passwordSchema.parse(password);

    if (this.database.findUserByEmail(normalizedEmail)) {
      throw new Error("该邮箱已注册");
    }

    const user = this.database.createUser(normalizedEmail, hashPassword(password));
    return this.createSessionForUser(user.id);
  }

  login(email: string, password: string): AuthSessionPayload {
    const normalizedEmail = emailSchema.parse(email);
    passwordSchema.parse(password);

    const user = this.database.findUserByEmail(normalizedEmail);
    if (!user || !verifyPassword(password, user.password_hash)) {
      throw new Error("邮箱或密码错误");
    }

    if (user.status !== "active") {
      throw new Error("账号已被禁用");
    }

    const payload = this.createSessionForUser(user.id);
    this.database.touchLastLogin(user.id);

    return payload;
  }

  changePassword(userId: string, currentPassword: string, nextPassword: string) {
    passwordSchema.parse(currentPassword);
    passwordSchema.parse(nextPassword);

    const user = this.database.findUserById(userId);
    if (!user || !verifyPassword(currentPassword, user.password_hash)) {
      throw new Error("当前密码错误");
    }

    this.database.updateUserPassword(userId, hashPassword(nextPassword));
  }

  authenticateBearerToken(token: string): { tokenHash: string; user: PublicUser } | null {
    this.database.deleteExpiredSessions();
    const tokenHash = sha256(token);
    const sessionWithUser = this.database.findSessionWithUserByTokenHash(tokenHash);
    if (!sessionWithUser) {
      return null;
    }

    if (sessionWithUser.user.status !== "active") {
      this.database.deleteSession(tokenHash);
      return null;
    }

    if (sessionWithUser.session.expires_at <= new Date().toISOString()) {
      this.database.deleteSession(tokenHash);
      return null;
    }

    this.database.touchSession(tokenHash);

    return {
      tokenHash,
      user: {
        id: sessionWithUser.user.id,
        email: sessionWithUser.user.email,
        role: sessionWithUser.user.role,
        status: sessionWithUser.user.status,
        createdAt: sessionWithUser.user.created_at,
        updatedAt: sessionWithUser.user.updated_at,
        lastLoginAt: sessionWithUser.user.last_login_at,
      },
    };
  }

  revokeSession(tokenHash: string) {
    this.database.deleteSession(tokenHash);
  }

  refresh(refreshToken: string): AuthSessionPayload {
    const current = this.authenticateBearerToken(refreshToken);
    if (!current) {
      throw new Error("刷新令牌无效");
    }

    this.revokeSession(current.tokenHash);
    return this.createSessionForUser(current.user.id);
  }

  private createSessionForUser(userId: string): AuthSessionPayload {
    const user = this.database.findUserById(userId);
    if (!user) {
      throw new Error("用户不存在");
    }

    const token = createOpaqueToken();
    const tokenHash = sha256(token);
    const expiresAt = plusHoursIso(this.sessionTtlHours);
    this.database.createSession(userId, tokenHash, expiresAt);

    return {
      token,
      refreshToken: token,
      expiresAt,
      user: {
        id: user.id,
        email: user.email,
        role: user.role,
        status: user.status,
        createdAt: user.created_at,
        updatedAt: user.updated_at,
        lastLoginAt: user.last_login_at,
      },
    };
  }
}

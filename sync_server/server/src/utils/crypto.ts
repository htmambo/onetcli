import { createHash, randomBytes, scryptSync, timingSafeEqual } from "node:crypto";

export function hashPassword(password: string): string {
  const salt = randomBytes(16).toString("hex");
  // N=32768, r=8, p=1 -> ~32ms on modern hardware (高于默认值 16384)
  const derived = scryptSync(password, salt, 64, { N: 32768, r: 8, p: 1 }).toString("hex");
  return `${salt}:${derived}`;
}

export function verifyPassword(password: string, storedHash: string): boolean {
  const [salt, hash] = storedHash.split(":");
  if (!salt || !hash) {
    return false;
  }

  // 验证时复用写入时的参数
  const derived = scryptSync(password, salt, 64, { N: 32768, r: 8, p: 1 });
  const expected = Buffer.from(hash, "hex");
  if (derived.length !== expected.length) {
    return false;
  }

  return timingSafeEqual(derived, expected);
}

export function createOpaqueToken(): string {
  return randomBytes(48).toString("base64url");
}

export function sha256(input: string): string {
  return createHash("sha256").update(input).digest("hex");
}

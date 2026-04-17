import { createHash, randomBytes, scryptSync, timingSafeEqual } from "node:crypto";

const SCRYPT_KEY_LENGTH = 64;
const SCRYPT_OPTIONS = {
    N: 32768,
    r: 8,
    p: 1,
    maxmem: 64 * 1024 * 1024,
};

export function hashPassword(password: string): string {
    const salt = randomBytes(16).toString("hex");
    const derived = scryptSync(password, salt, SCRYPT_KEY_LENGTH, SCRYPT_OPTIONS).toString("hex");
    return `${salt}:${derived}`;
}

export function verifyPassword(password: string, storedHash: string): boolean {
    const [salt, hash] = storedHash.split(":");
    if (!salt || !hash) {
        return false;
    }

    const derived = scryptSync(password, salt, SCRYPT_KEY_LENGTH, SCRYPT_OPTIONS);
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
const ENCRYPTED_PREFIX = "ENC:";
const ENCRYPTED_PREFIX_V2 = "ENC:V2:";
const DERIVE_SALT = "onehub_password_encryption_salt_v1";
const VERIFICATION_MAGIC = "ONEHUB_KEY_VERIFY_V1";
const NONCE_LENGTH = 12;
const SALT_LENGTH = 16;

export class SyncCryptoError extends Error {}

function ensureWebCryptoSupport() {
  if (!globalThis.crypto?.subtle) {
    throw new SyncCryptoError("当前浏览器不支持本地解密，请更换现代浏览器后重试");
  }
}

function toUtf8Bytes(value: string) {
  return new TextEncoder().encode(value);
}

function toUtf8String(value: ArrayBuffer) {
  return new TextDecoder().decode(value);
}

function decodeBase64(value: string): Uint8Array {
  try {
    const binary = atob(value);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return bytes;
  } catch {
    throw new SyncCryptoError("密文编码无效，无法完成解密");
  }
}

/// 使用 SHA-256 派生 AES-256 密钥（与 Rust 后端 derive_key 一致）
async function deriveKey(masterKey: string): Promise<CryptoKey> {
  ensureWebCryptoSupport();
  const material = toUtf8Bytes(masterKey + DERIVE_SALT);
  const digest = await crypto.subtle.digest("SHA-256", material);
  return crypto.subtle.importKey("raw", digest, "AES-GCM", false, ["decrypt", "encrypt"]);
}

/// 解密 V2 格式密文（salt(16) + nonce(12) + ciphertext）
async function decryptV2Payload(encodedPayload: string, key: CryptoKey): Promise<string> {
  const combined = decodeBase64(encodedPayload);
  if (combined.length <= SALT_LENGTH + NONCE_LENGTH) {
    throw new SyncCryptoError("密文格式无效，无法完成解密");
  }

  const nonce = combined.slice(SALT_LENGTH, SALT_LENGTH + NONCE_LENGTH);
  const ciphertext = combined.slice(SALT_LENGTH + NONCE_LENGTH);

  try {
    const plaintext = await crypto.subtle.decrypt(
      {
        name: "AES-GCM",
        iv: nonce,
      },
      key,
      ciphertext,
    );
    return toUtf8String(plaintext);
  } catch {
    throw new SyncCryptoError("主密钥错误，或当前密文已损坏");
  }
}

/// 解密 V1 格式密文（nonce(12) + ciphertext，用于向后兼容）
async function decryptV1Payload(encodedPayload: string, key: CryptoKey): Promise<string> {
  const combined = decodeBase64(encodedPayload);
  if (combined.length <= NONCE_LENGTH) {
    throw new SyncCryptoError("密文格式无效，无法完成解密");
  }

  const nonce = combined.slice(0, NONCE_LENGTH);
  const ciphertext = combined.slice(NONCE_LENGTH);

  try {
    const plaintext = await crypto.subtle.decrypt(
      {
        name: "AES-GCM",
        iv: nonce,
      },
      key,
      ciphertext,
    );
    return toUtf8String(plaintext);
  } catch {
    throw new SyncCryptoError("主密钥错误，或当前密文已损坏");
  }
}

/// 验证主密钥是否正确（仅支持 V1 格式）
export async function verifySyncMasterKey(
  masterKey: string,
  verificationData: string,
): Promise<boolean> {
  if (!masterKey.trim() || !verificationData) {
    return false;
  }

  try {
    const key = await deriveKey(masterKey);
    const plaintext = await decryptV1Payload(verificationData, key);
    return plaintext === VERIFICATION_MAGIC;
  } catch {
    return false;
  }
}

/// 解密云同步加密数据（支持 ENC:V2: 和 ENC: 前缀）
export async function decryptSyncEncryptedData(
  encryptedData: string,
  masterKey: string,
): Promise<string> {
  if (!masterKey.trim()) {
    throw new SyncCryptoError("请输入主密钥");
  }

  if (!encryptedData) {
    return "";
  }

  if (encryptedData.startsWith(ENCRYPTED_PREFIX_V2)) {
    // V2 格式: ENC:V2: + base64(salt(16) + nonce(12) + ciphertext)
    const encodedPayload = encryptedData.slice(ENCRYPTED_PREFIX_V2.length);
    const key = await deriveKey(masterKey);
    return decryptV2Payload(encodedPayload, key);
  }

  if (encryptedData.startsWith(ENCRYPTED_PREFIX)) {
    // V1 格式: ENC: + base64(nonce(12) + ciphertext)
    const encodedPayload = encryptedData.slice(ENCRYPTED_PREFIX.length);
    const key = await deriveKey(masterKey);
    return decryptV1Payload(encodedPayload, key);
  }

  // 未加密，直接返回
  return encryptedData;
}

export function formatDecryptedPayload(plaintext: string): string {
  if (!plaintext) {
    return "";
  }

  try {
    return JSON.stringify(JSON.parse(plaintext), null, 2);
  } catch {
    return plaintext;
  }
}

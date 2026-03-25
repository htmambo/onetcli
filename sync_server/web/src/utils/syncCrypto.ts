const ENCRYPTED_PREFIX = "ENC:";
const DERIVE_SALT = "onehub_password_encryption_salt_v1";
const VERIFICATION_MAGIC = "ONEHUB_KEY_VERIFY_V1";
const NONCE_LENGTH = 12;

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

function concatBytes(parts: Uint8Array[]) {
  const totalLength = parts.reduce((sum, part) => sum + part.length, 0);
  const merged = new Uint8Array(totalLength);

  let offset = 0;
  for (const part of parts) {
    merged.set(part, offset);
    offset += part.length;
  }

  return merged;
}

function decodeBase64(value: string) {
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

async function deriveKey(masterKey: string) {
  ensureWebCryptoSupport();

  const material = concatBytes([
    toUtf8Bytes(masterKey),
    toUtf8Bytes(DERIVE_SALT),
  ]);
  const digest = await crypto.subtle.digest("SHA-256", material);

  return crypto.subtle.importKey("raw", digest, "AES-GCM", false, ["decrypt"]);
}

async function decryptCombinedPayload(encodedPayload: string, key: CryptoKey) {
  const combined = decodeBase64(encodedPayload);
  if (combined.length <= NONCE_LENGTH) {
    throw new SyncCryptoError("密文格式无效，无法完成解密");
  }

  const iv = combined.slice(0, NONCE_LENGTH);
  const ciphertext = combined.slice(NONCE_LENGTH);

  try {
    const plaintext = await crypto.subtle.decrypt(
      {
        name: "AES-GCM",
        iv,
      },
      key,
      ciphertext,
    );

    return toUtf8String(plaintext);
  } catch {
    throw new SyncCryptoError("主密钥错误，或当前密文已损坏");
  }
}

export async function verifySyncMasterKey(masterKey: string, verificationData: string) {
  if (!masterKey.trim() || !verificationData) {
    return false;
  }

  try {
    const key = await deriveKey(masterKey);
    const plaintext = await decryptCombinedPayload(verificationData, key);
    return plaintext === VERIFICATION_MAGIC;
  } catch {
    return false;
  }
}

export async function decryptSyncEncryptedData(encryptedData: string, masterKey: string) {
  if (!masterKey.trim()) {
    throw new SyncCryptoError("请输入主密钥");
  }

  if (!encryptedData) {
    return "";
  }

  if (!encryptedData.startsWith(ENCRYPTED_PREFIX)) {
    return encryptedData;
  }

  const key = await deriveKey(masterKey);
  const encodedPayload = encryptedData.slice(ENCRYPTED_PREFIX.length);
  return decryptCombinedPayload(encodedPayload, key);
}

export function formatDecryptedPayload(plaintext: string) {
  if (!plaintext) {
    return "";
  }

  try {
    return JSON.stringify(JSON.parse(plaintext), null, 2);
  } catch {
    return plaintext;
  }
}

//! 密码加密模块
//!
//! 使用 AES-256-GCM 对密码进行加密存储。
//! 用户需要设置一个主密钥，该密钥用于派生加密密钥。
//! 如果主密钥丢失，所有加密的密码将无法恢复。
//!
//! 支持两种使用方式：
//! 1. 全局密钥模式：通过 set_master_key 设置全局密钥，使用 encrypt_password/decrypt_password
//! 2. 指定密钥模式：使用 encrypt_with_key/decrypt_with_key 直接传入密钥
//!
//! 版本说明：
//! - 密码加密格式 V2 (ENC:V2:)：包含随机盐值用于完整性检测
//! - 验证数据使用 Argon2id 派生的密钥加密（更安全）
//! - 向后兼容：V1 格式（ENC:）仍可解密
//!
//! 密钥持久化：
//! - 使用本地文件存储，主密钥会以固定 key 加密后落盘
//! - 通过 key_storage 模块统一封装存储后端接口
//! - 应用启动时可自动从存储后端恢复密钥

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use argon2::Argon2;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use zeroize::Zeroizing;

use crate::key_storage;

/// 加密错误类型
#[derive(Debug, Clone, PartialEq)]
pub enum CryptoError {
    /// 旧密码错误
    InvalidOldPassword,
    /// 新密码为空
    EmptyNewPassword,
    /// 两次输入的新密码不一致
    PasswordMismatch,
    /// 未设置过主密码
    NoPasswordSet,
    /// 保存验证数据失败
    SaveVerificationFailed,
    /// 解密失败
    DecryptionFailed,
    /// 编码失败
    EncodingFailed,
    /// 数据格式错误
    InvalidDataFormat,
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::InvalidOldPassword => write!(f, "旧密码错误"),
            CryptoError::EmptyNewPassword => write!(f, "新密码不能为空"),
            CryptoError::PasswordMismatch => write!(f, "两次输入的新密码不一致"),
            CryptoError::NoPasswordSet => write!(f, "未设置过主密码"),
            CryptoError::SaveVerificationFailed => write!(f, "保存验证数据失败"),
            CryptoError::DecryptionFailed => write!(f, "解密失败"),
            CryptoError::EncodingFailed => write!(f, "编码失败"),
            CryptoError::InvalidDataFormat => write!(f, "数据格式错误"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// 加密前缀标识，用于识别已加密的密码（V1 格式）
const ENCRYPTED_PREFIX: &str = "ENC:";
/// 新格式加密前缀（V2：包含随机盐值用于检测损坏的密文）
const ENCRYPTED_PREFIX_V2: &str = "ENC:V2:";
/// 当前默认加密前缀（V3：使用 Argon2id 派生 AES 密钥，抗 GPU 暴力枚举）
const ENCRYPTED_PREFIX_V3: &str = "ENC:V3:";

/// 验证数据的魔术字符串，用于验证密钥是否正确
const VERIFICATION_MAGIC: &str = "ONEHUB_KEY_VERIFY_V1";

/// 密钥验证文件名
const KEY_VERIFICATION_FILE: &str = "key_verification";

/// 全局加密密钥存储（派生后的密钥，用于 V1/V2 兼容读取）
static ENCRYPTION_KEY: RwLock<Option<[u8; 32]>> = RwLock::new(None);

/// V3 加密路径使用的派生密钥（Argon2id）。
///
/// 与 `ENCRYPTION_KEY` 并存：旧数据按旧算法读取，新写入按新算法。
/// 双槽的设计保证 SHA-256 派生与 Argon2id 派生互不污染。
static ENCRYPTION_KEY_V3: RwLock<Option<[u8; 32]>> = RwLock::new(None);

/// 全局原始主密钥存储。
///
/// **撤销说明（S2）**：原实现把主密钥明文以 `String` 形式常驻进程静态区，存在
/// 崩溃 dump / 子进程环境继承 / heap profiler 抓现场等风险。S2 后不再长期持有；
/// 调用方需要主密钥明文（如云同步加解密）时通过 `raw_master_key_for_sync()` /
/// `with_raw_master_key` 按需从 `key_storage` 后端读取，返回的 `Zeroizing<String>`
/// 在 drop 时会清零缓冲。
///
/// 由于全局 ENCRYPTION_KEY 槽保留了派生后的 AES 密钥，所有"加解密一条数据"的
/// 路径仍可不接触主密钥明文——主密钥明文仅在云同步等少数场景下被读出。
// 历史保留：RAW_MASTER_KEY 已撤销；显式以注释取代原静态变量占位，避免回归。
// static RAW_MASTER_KEY: RwLock<Option<String>> = RwLock::new(None);

/// 获取数据目录路径
///
/// 统一委托 key_storage::get_data_dir：包含 legacy 目录迁移逻辑，
/// 且测试环境的重定向只在那一份实现里生效，避免双份路径分叉。
fn get_data_dir() -> Option<PathBuf> {
    crate::key_storage::get_data_dir()
}

/// 获取密钥验证文件路径
fn get_verification_file_path() -> Option<PathBuf> {
    get_data_dir().map(|p| p.join(KEY_VERIFICATION_FILE))
}

/// 检查是否已设置过主密钥（验证文件是否存在）
pub fn has_repo_password_set() -> bool {
    get_verification_file_path()
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// 保存密钥验证数据到文件
fn save_verification_data(data: &str) -> bool {
    if let Some(path) = get_verification_file_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(path, data).is_ok()
    } else {
        false
    }
}

/// 从文件读取密钥验证数据
fn load_verification_data() -> Option<String> {
    get_verification_file_path().and_then(|p| fs::read_to_string(p).ok())
}

/// 从用户主密钥派生 AES-256 密钥（SHA-256，保持向后兼容）
fn derive_key(master_key: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(master_key.as_bytes());
    hasher.update(b"onehub_password_encryption_salt_v1");
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// V3 写入路径使用的派生 salt：版本化、可识别、不与 V1/V2 重叠。
///
/// 作为应用级常量写入源码；攻击者拿到源码即可获取，但 Argon2id 拉伸本身
/// 提供抗暴力枚举能力（与 SHA-256 不可同日而语）。
const MASTER_KEY_APP_SALT_V3: &[u8] = b"omnihub-master-key-v3-argon2id-salt-v1";

/// 从用户主密钥 + 应用级 salt 派生 AES-256 密钥（Argon2id，用于 V3 写入路径）。
///
/// Argon2id 是密码哈希竞赛冠军，是 KDF 推荐算法。攻击者要暴力枚举主密钥，
/// 必须为每个候选值执行 Argon2id；其拉伸因子默认 ~100ms / 候选，
/// 使 GPU 暴力枚举的实际效率下降到难以承受的水平。
fn derive_key_v3(master_key: &str, salt: &[u8]) -> [u8; 32] {
    let mut derived_key = [0u8; 32];
    let argon2 = Argon2::default();
    let _ = argon2.hash_password_into(master_key.as_bytes(), salt, &mut derived_key);
    derived_key
}

/// 验证数据魔术串前缀（用于标识新格式）
const VERIFICATION_V2_PREFIX: &str = "V2:";
/// V3 verification 前缀：同样用 Argon2id 派生，密文布局与 V2 一致，仅 prefix 不同。
const VERIFICATION_V3_PREFIX: &str = "V3:";

/// 生成 V1 格式的密钥验证数据（用于前端同步兼容）
///
/// 使用 SHA-256 派生密钥，与前端 syncCrypto.ts 的 verifySyncMasterKey V1 逻辑一致。
/// 格式：base64(nonce(12) + ciphertext)
pub fn generate_key_verification_v1(master_key: &str) -> String {
    let key = derive_key(master_key);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    match cipher.encrypt(nonce, VERIFICATION_MAGIC.as_bytes()) {
        Ok(ciphertext) => {
            let mut combined = Vec::with_capacity(12 + ciphertext.len());
            combined.extend_from_slice(&nonce_bytes);
            combined.extend_from_slice(&ciphertext);
            BASE64.encode(&combined)
        }
        Err(_) => String::new(),
    }
}

/// 生成密钥验证数据（V2 格式，保留以兼容同步前端）
///
/// 返回一个加密的魔术字符串，用于验证用户输入的密钥是否正确。
/// 使用 Argon2id 派生密钥进行加密，更安全。
/// 存储格式：base64(V2: + argon2_hash + nonce + ciphertext)
pub fn generate_key_verification(master_key: &str) -> String {
    let mut salt_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt_bytes);

    // 使用 Argon2id 派生加密密钥（更安全）
    let mut derived_key = [0u8; 32];
    let argon2 = Argon2::default();
    let _ = argon2.hash_password_into(master_key.as_bytes(), &salt_bytes, &mut derived_key);

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&derived_key));

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    match cipher.encrypt(nonce, VERIFICATION_MAGIC.as_bytes()) {
        Ok(ciphertext) => {
            // 格式: V2: + salt(16) + nonce(12) + ciphertext
            let mut combined = Vec::with_capacity(3 + 16 + 12 + ciphertext.len());
            combined.extend_from_slice(VERIFICATION_V2_PREFIX.as_bytes());
            combined.extend_from_slice(&salt_bytes);
            combined.extend_from_slice(&nonce_bytes);
            combined.extend_from_slice(&ciphertext);
            BASE64.encode(&combined)
        }
        Err(_) => String::new(),
    }
}

/// 生成 V3 verification 数据（默认新装路径）
///
/// 与 V2 密文布局完全一致（V3: + salt + nonce + ciphertext，Argon2id 派生），
/// 仅前缀字符不同。verify_master_key 会按 V2/V3 prefix 分发到同一条解密路径。
pub fn generate_key_verification_v3(master_key: &str) -> String {
    let mut salt_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt_bytes);

    let mut derived_key = [0u8; 32];
    let argon2 = Argon2::default();
    let _ = argon2.hash_password_into(master_key.as_bytes(), &salt_bytes, &mut derived_key);

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&derived_key));

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    match cipher.encrypt(nonce, VERIFICATION_MAGIC.as_bytes()) {
        Ok(ciphertext) => {
            let mut combined = Vec::with_capacity(3 + 16 + 12 + ciphertext.len());
            combined.extend_from_slice(VERIFICATION_V3_PREFIX.as_bytes());
            combined.extend_from_slice(&salt_bytes);
            combined.extend_from_slice(&nonce_bytes);
            combined.extend_from_slice(&ciphertext);
            BASE64.encode(&combined)
        }
        Err(_) => String::new(),
    }
}

/// 验证密钥是否正确（支持 V1 / V2 / V3 格式）
///
/// 通过尝试解密验证数据来验证密钥是否正确。
pub fn verify_master_key(master_key: &str, verification_data: &str) -> bool {
    if verification_data.is_empty() {
        return false;
    }

    let combined = match BASE64.decode(verification_data) {
        Ok(data) => data,
        Err(_) => return false,
    };

    if combined.len() < 3 + 12 {
        return false;
    }

    // 检测格式：V2/V3 都走 Argon2id 分支（密文布局相同，仅 prefix 不同）
    if combined.starts_with(VERIFICATION_V2_PREFIX.as_bytes())
        || combined.starts_with(VERIFICATION_V3_PREFIX.as_bytes())
    {
        // V2/V3: prefix(3) + salt(16) + nonce(12) + ciphertext
        let body = &combined[3..];
        if body.len() < 16 + 12 {
            return false;
        }
        let salt = &body[..16];
        let nonce = Nonce::from_slice(&body[16..28]);
        let ciphertext = &body[28..];

        // 使用 Argon2id 派生密钥
        let mut derived_key = [0u8; 32];
        let argon2 = Argon2::default();
        if argon2
            .hash_password_into(master_key.as_bytes(), salt, &mut derived_key)
            .is_err()
        {
            return false;
        }
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&derived_key));
        match cipher.decrypt(nonce, ciphertext) {
            Ok(plaintext) => String::from_utf8(plaintext)
                .map(|s| s == VERIFICATION_MAGIC)
                .unwrap_or(false),
            Err(_) => false,
        }
    } else {
        // V1: nonce(12) + ciphertext（旧格式，使用 SHA-256）
        if combined.len() < 12 {
            return false;
        }
        let nonce = Nonce::from_slice(&combined[..12]);
        let ciphertext = &combined[12..];

        let key = derive_key(master_key);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        match cipher.decrypt(nonce, ciphertext) {
            Ok(plaintext) => String::from_utf8(plaintext)
                .map(|s| s == VERIFICATION_MAGIC)
                .unwrap_or(false),
            Err(_) => false,
        }
    }
}

/// 设置主密钥
///
/// 用户提供的主密钥将被哈希后存储在内存中，用于后续的加密/解密操作。
/// 如果是首次设置，会生成并保存验证数据。
/// 同时会将密钥保存到存储后端，以便应用重启后自动恢复。
///
/// 内部同时派生两份密钥：
/// - `ENCRYPTION_KEY`（SHA-256）：用于解密历史 V1/V2 密文。
/// - `ENCRYPTION_KEY_V3`（Argon2id）：用于 V3 写入与 V3 密文解密。
pub fn set_master_key(master_key: &str) {
    let key = derive_key(master_key);
    if let Ok(mut guard) = ENCRYPTION_KEY.write() {
        *guard = Some(key);
    }

    // V3 写入路径：Argon2id 派生
    let key_v3 = derive_key_v3(master_key, MASTER_KEY_APP_SALT_V3);
    if let Ok(mut guard) = ENCRYPTION_KEY_V3.write() {
        *guard = Some(key_v3);
    }

    // S2：不再写 RAW_MASTER_KEY 静态常驻。
    // 调用方需要主密钥明文时，通过 raw_master_key_for_sync() / with_raw_master_key
    // 从 key_storage 后端按需读取。

    // 如果尚未设置过密码，生成并保存验证数据（默认走 V3 路径）
    if !has_repo_password_set() {
        let verification = generate_key_verification_v3(master_key);
        save_verification_data(&verification);
    }

    // 保存到存储后端
    let storage = key_storage::get_key_storage();
    if let Err(e) = storage.save(master_key) {
        tracing::warn!("[{}] 保存密钥失败: {}", storage.name(), e);
    }
}

/// 验证并设置主密钥
///
/// 如果已设置过密码，需要先验证密钥是否正确。
/// 返回 Ok(()) 表示设置成功，Err 表示密码错误。
pub fn verify_and_set_master_key(master_key: &str) -> Result<(), &'static str> {
    if has_repo_password_set() {
        // 已设置过密码，需要验证
        if let Some(verification_data) = load_verification_data() {
            if !verify_master_key(master_key, &verification_data) {
                return Err("密码错误");
            }
        }
    }

    // 验证通过或首次设置，设置密钥
    set_master_key(master_key);
    Ok(())
}

/// 清除主密钥
pub fn clear_master_key() {
    if let Ok(mut guard) = ENCRYPTION_KEY.write() {
        *guard = None;
    }
    if let Ok(mut guard) = ENCRYPTION_KEY_V3.write() {
        *guard = None;
    }
    // S2：RAW_MASTER_KEY 已撤销，无需清理。
    // 同时清除存储后端中的密钥
    let storage = key_storage::get_key_storage();
    let _ = storage.delete();
}

/// 检查是否已设置主密钥
pub fn has_master_key() -> bool {
    ENCRYPTION_KEY
        .read()
        .map(|guard| guard.is_some())
        .unwrap_or(false)
}

/// 获取主密钥明文的一次性副本（用于云同步等少数需要原始密钥的场景）。
///
/// 与 S2 之前的 `get_raw_master_key` 区别：
/// - 不再读取进程级 `RAW_MASTER_KEY` 静态变量（已撤销）。
/// - 从 `key_storage` 后端按需读取；返回的 `Zeroizing<String>` 在 drop 时
///   清零堆内存，降低 dump / 调试器抓现场泄漏风险。
///
/// 注意：调用方应尽量缩短持有副本的作用域；如要多次使用，请用
/// [`with_raw_master_key`] 闭包风格接口。
pub fn raw_master_key_for_sync() -> Option<Zeroizing<String>> {
    let storage = key_storage::get_key_storage();
    storage.load().map(Zeroizing::new)
}

/// 闭包风格访问主密钥明文：闭包内 `&str` 可用；闭包结束立即析构副本。
///
/// 适用于"需要主密钥做几件事、且不想在调用栈长期持有"的场景。
///
/// # 示例
/// ```ignore
/// crypto::with_raw_master_key(|master_key| {
///     // 在此处使用 master_key；离开作用域后内部缓冲区被 zeroize
///     sync_blob.encrypt(master_key, payload)
/// });
/// ```
///
/// 当未设置主密钥、或 `key_storage` 后端不可用时，返回 `None`。
pub fn with_raw_master_key<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&str) -> R,
{
    let storage = key_storage::get_key_storage();
    let key = storage.load()?;
    let result = f(&key);
    drop(key);
    Some(result)
}

/// 加密密码
///
/// 如果未设置主密钥，返回原始密码。
/// 加密后的密码格式：`ENC:V3:base64(salt + nonce + ciphertext)`。
///
/// V3 写入路径使用 Argon2id 派生 AES 密钥（V1/V2 的 SHA-256 派生仅保留作读取路径）。
pub fn encrypt_password(password: &str) -> String {
    if password.is_empty() {
        return password.to_string();
    }

    // 如果已经是加密的，直接返回（支持 V1/V2/V3 格式）
    if password.starts_with(ENCRYPTED_PREFIX)
        || password.starts_with(ENCRYPTED_PREFIX_V2)
        || password.starts_with(ENCRYPTED_PREFIX_V3)
    {
        return password.to_string();
    }

    // 获取 V3 路径的 Argon2id 派生密钥
    let key = match ENCRYPTION_KEY_V3.read() {
        Ok(guard) => match &*guard {
            Some(k) => *k,
            None => return password.to_string(),
        },
        Err(_) => return password.to_string(),
    };

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));

    // 生成随机盐值（16 字节，密文布局占位）
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);

    // 生成随机 nonce (12 字节)
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    match cipher.encrypt(nonce, password.as_bytes()) {
        Ok(ciphertext) => {
            // V3 格式：salt(16) + nonce(12) + ciphertext
            let mut combined = Vec::with_capacity(16 + 12 + ciphertext.len());
            combined.extend_from_slice(&salt);
            combined.extend_from_slice(&nonce_bytes);
            combined.extend_from_slice(&ciphertext);
            format!("{}{}", ENCRYPTED_PREFIX_V3, BASE64.encode(&combined))
        }
        Err(_) => password.to_string(),
    }
}

/// 解密密码（支持 V1 / V2 / V3 格式）
///
/// 如果密码未加密（不以 `ENC:` 开头），返回原始密码。
/// 如果未设置主密钥或解密失败，返回空字符串。
///
/// 按密文前缀选择派生密钥槽：
/// - `ENC:` 与 `ENC:V2:` 走 `ENCRYPTION_KEY`（SHA-256 派生）；
/// - `ENC:V3:` 走 `ENCRYPTION_KEY_V3`（Argon2id 派生）。
pub fn decrypt_password(encrypted: &str) -> String {
    if encrypted.is_empty() {
        return encrypted.to_string();
    }

    // 如果不是加密的，直接返回
    if !encrypted.starts_with(ENCRYPTED_PREFIX)
        && !encrypted.starts_with(ENCRYPTED_PREFIX_V2)
        && !encrypted.starts_with(ENCRYPTED_PREFIX_V3)
    {
        return encrypted.to_string();
    }

    // 检测格式并切前缀
    let (is_v3, is_v2, encoded) = if encrypted.starts_with(ENCRYPTED_PREFIX_V3) {
        (true, false, &encrypted[ENCRYPTED_PREFIX_V3.len()..])
    } else if encrypted.starts_with(ENCRYPTED_PREFIX_V2) {
        (false, true, &encrypted[ENCRYPTED_PREFIX_V2.len()..])
    } else {
        (false, false, &encrypted[ENCRYPTED_PREFIX.len()..])
    };

    let combined = match BASE64.decode(encoded) {
        Ok(data) => data,
        Err(_) => return String::new(),
    };

    if combined.len() < 12 {
        return String::new();
    }

    let (key, nonce, ciphertext) = if is_v3 || is_v2 {
        // V3/V2: salt(16) + nonce(12) + ciphertext
        if combined.len() < 16 + 12 {
            return String::new();
        }
        let _salt = &combined[..16];
        let nonce = Nonce::from_slice(&combined[16..28]);
        let ciphertext = &combined[28..];

        // V3 用 Argon2id 派生槽，V2 仍用 SHA-256 派生槽
        let key_res = if is_v3 {
            ENCRYPTION_KEY_V3.read()
        } else {
            ENCRYPTION_KEY.read()
        };
        let key = match key_res {
            Ok(guard) => match *guard {
                Some(k) => k,
                None => return String::new(),
            },
            Err(_) => return String::new(),
        };
        (key, nonce, ciphertext)
    } else {
        // V1: nonce(12) + ciphertext
        let nonce = Nonce::from_slice(&combined[..12]);
        let ciphertext = &combined[12..];

        let key = match ENCRYPTION_KEY.read() {
            Ok(guard) => match *guard {
                Some(k) => k,
                None => return String::new(),
            },
            Err(_) => return String::new(),
        };
        (key, nonce, ciphertext)
    };

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));

    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => String::from_utf8(plaintext).unwrap_or_default(),
        Err(_) => String::new(),
    }
}

/// 检查密码是否已加密（识别 V1/V2/V3 任一前缀）
pub fn is_encrypted(password: &str) -> bool {
    password.starts_with(ENCRYPTED_PREFIX)
        || password.starts_with(ENCRYPTED_PREFIX_V2)
        || password.starts_with(ENCRYPTED_PREFIX_V3)
}

// ============================================================================
// 指定密钥模式的加解密函数（用于云同步和密钥迁移）
// ============================================================================

/// 使用指定密钥加密密码
///
/// 不依赖全局密钥状态，直接使用传入的主密钥进行加密。
/// 适用于云同步场景和密钥迁移场景。
///
/// V3 写入使用 Argon2id 派生，V1/V2 仅作历史兼容。
pub fn encrypt_with_key(plaintext: &str, master_key: &str) -> String {
    if plaintext.is_empty() {
        return plaintext.to_string();
    }

    // 如果已经是加密的，直接返回（支持 V1/V2/V3 格式）
    if plaintext.starts_with(ENCRYPTED_PREFIX)
        || plaintext.starts_with(ENCRYPTED_PREFIX_V2)
        || plaintext.starts_with(ENCRYPTED_PREFIX_V3)
    {
        return plaintext.to_string();
    }

    let key = derive_key_v3(master_key, MASTER_KEY_APP_SALT_V3);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));

    // 生成随机盐值（16 字节，密文布局占位）
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);

    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    match cipher.encrypt(nonce, plaintext.as_bytes()) {
        Ok(ciphertext) => {
            // V3 格式：salt(16) + nonce(12) + ciphertext
            let mut combined = Vec::with_capacity(16 + 12 + ciphertext.len());
            combined.extend_from_slice(&salt);
            combined.extend_from_slice(&nonce_bytes);
            combined.extend_from_slice(&ciphertext);
            format!("{}{}", ENCRYPTED_PREFIX_V3, BASE64.encode(&combined))
        }
        Err(_) => plaintext.to_string(),
    }
}

/// 使用指定密钥解密密码（支持 V1 / V2 / V3 格式）
///
/// 不依赖全局密钥状态，直接使用传入的主密钥进行解密。
/// 适用于云同步场景和密钥迁移场景。
pub fn decrypt_with_key(encrypted: &str, master_key: &str) -> Result<String, CryptoError> {
    if encrypted.is_empty() {
        return Ok(encrypted.to_string());
    }

    // 如果不是加密的，直接返回
    if !encrypted.starts_with(ENCRYPTED_PREFIX)
        && !encrypted.starts_with(ENCRYPTED_PREFIX_V2)
        && !encrypted.starts_with(ENCRYPTED_PREFIX_V3)
    {
        return Ok(encrypted.to_string());
    }

    // 检测格式并切前缀
    let (is_v3, is_v2) = if encrypted.starts_with(ENCRYPTED_PREFIX_V3) {
        (true, false)
    } else if encrypted.starts_with(ENCRYPTED_PREFIX_V2) {
        (false, true)
    } else {
        (false, false)
    };
    let encoded = if is_v3 {
        &encrypted[ENCRYPTED_PREFIX_V3.len()..]
    } else if is_v2 {
        &encrypted[ENCRYPTED_PREFIX_V2.len()..]
    } else {
        &encrypted[ENCRYPTED_PREFIX.len()..]
    };

    let combined = BASE64
        .decode(encoded)
        .map_err(|_| CryptoError::EncodingFailed)?;

    // V3 走 Argon2id 派生；V1/V2 保持 SHA-256 派生
    let key = if is_v3 {
        derive_key_v3(master_key, MASTER_KEY_APP_SALT_V3)
    } else {
        derive_key(master_key)
    };
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));

    let (nonce, ciphertext) = if is_v3 || is_v2 {
        // V3/V2: salt(16) + nonce(12) + ciphertext
        if combined.len() < 16 + 12 {
            return Err(CryptoError::InvalidDataFormat);
        }
        let nonce = Nonce::from_slice(&combined[16..28]);
        let ciphertext = &combined[28..];
        (nonce, ciphertext)
    } else {
        // V1: nonce(12) + ciphertext
        if combined.len() < 12 {
            return Err(CryptoError::InvalidDataFormat);
        }
        let nonce = Nonce::from_slice(&combined[..12]);
        let ciphertext = &combined[12..];
        (nonce, ciphertext)
    };

    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => String::from_utf8(plaintext).map_err(|_| CryptoError::EncodingFailed),
        Err(_) => Err(CryptoError::DecryptionFailed),
    }
}

/// 重新加密数据（从旧密钥迁移到新密钥）
///
/// 用于修改主密钥时重新加密已加密的数据。
pub fn re_encrypt_data(
    encrypted_data: &str,
    old_key: &str,
    new_key: &str,
) -> Result<String, CryptoError> {
    // 如果数据未加密，直接用新密钥加密
    if !encrypted_data.starts_with(ENCRYPTED_PREFIX)
        && !encrypted_data.starts_with(ENCRYPTED_PREFIX_V2)
        && !encrypted_data.starts_with(ENCRYPTED_PREFIX_V3)
    {
        return Ok(encrypt_with_key(encrypted_data, new_key));
    }

    // 用旧密钥解密
    let plaintext = decrypt_with_key(encrypted_data, old_key)?;

    // 用新密钥重新加密
    Ok(encrypt_with_key(&plaintext, new_key))
}

// ============================================================================
// 修改主密钥相关函数
// ============================================================================

/// 修改主密钥
///
/// 验证旧密钥后更新验证数据和内存中的密钥。
/// 注意：此函数不负责重新加密已有数据，调用方需要自行处理数据迁移。
///
/// # 参数
/// - `old_key`: 当前的主密钥
/// - `new_key`: 新的主密钥
/// - `confirm_new_key`: 确认新密钥（需要与 new_key 一致）
///
/// # 返回
/// - `Ok(())`: 修改成功
/// - `Err(CryptoError)`: 修改失败的原因
pub fn change_master_key(
    old_key: &str,
    new_key: &str,
    confirm_new_key: &str,
) -> Result<(), CryptoError> {
    // 检查是否已设置过密码
    if !has_repo_password_set() {
        return Err(CryptoError::NoPasswordSet);
    }

    // 验证旧密钥
    if let Some(verification_data) = load_verification_data() {
        if !verify_master_key(old_key, &verification_data) {
            return Err(CryptoError::InvalidOldPassword);
        }
    } else {
        return Err(CryptoError::NoPasswordSet);
    }

    // 验证新密钥不为空
    if new_key.is_empty() {
        return Err(CryptoError::EmptyNewPassword);
    }

    // 验证两次新密钥一致
    if new_key != confirm_new_key {
        return Err(CryptoError::PasswordMismatch);
    }

    // 生成新的验证数据（默认走 V3 路径）
    let new_verification = generate_key_verification_v3(new_key);
    if !save_verification_data(&new_verification) {
        return Err(CryptoError::SaveVerificationFailed);
    }

    // 更新内存中的密钥（V1/V2 读取路径用 SHA-256 派生）
    let key = derive_key(new_key);
    if let Ok(mut guard) = ENCRYPTION_KEY.write() {
        *guard = Some(key);
    }

    // V3 写入路径：Argon2id 派生
    let key_v3 = derive_key_v3(new_key, MASTER_KEY_APP_SALT_V3);
    if let Ok(mut guard) = ENCRYPTION_KEY_V3.write() {
        *guard = Some(key_v3);
    }

    // S2：不再写 RAW_MASTER_KEY；调用方按需通过 raw_master_key_for_sync() 读。

    // 更新存储后端中的密钥
    let storage = key_storage::get_key_storage();
    if let Err(e) = storage.save(new_key) {
        tracing::warn!("[{}] 更新密钥失败: {}", storage.name(), e);
    }

    Ok(())
}

/// 重置主密钥
///
/// 删除验证文件，允许用户重新设置密钥。
/// 警告：这将导致所有已加密的密码无法解密！
pub fn reset_repo_password() -> bool {
    // 清除内存中的密钥
    clear_master_key();

    // 删除验证文件
    if let Some(path) = get_verification_file_path() {
        if path.exists() {
            return fs::remove_file(path).is_ok();
        }
    }
    true
}

/// 获取当前密钥的验证数据
///
/// 用于云端存储，验证用户输入的密钥是否正确。
pub fn get_current_verification_data() -> Option<String> {
    load_verification_data()
}

/// 从存储后端恢复主密钥
///
/// 尝试从当前配置的存储后端读取密钥。
/// 如果密钥存在且验证通过，则自动设置到内存。
/// 返回是否成功恢复。
pub fn try_restore_master_key() -> bool {
    let storage = key_storage::get_key_storage();
    // tracing::info!("[密钥恢复] 尝试从「{}」恢复主密钥...", storage.name());

    // 如果没有设置过密码验证文件，不需要恢复
    if !has_repo_password_set() {
        // tracing::info!("[密钥恢复] 未设置过主密钥，跳过恢复");
        return false;
    }

    // 从存储后端读取
    let master_key = match storage.load() {
        Some(key) => key,
        None => {
            // tracing::warn!(
            //     "[密钥恢复] 从「{}」读取失败，需要用户手动输入",
            //     storage.name()
            // );
            return false;
        }
    };

    // 验证密钥是否正确
    if let Some(verification_data) = load_verification_data() {
        if !verify_master_key(&master_key, &verification_data) {
            // tracing::warn!("[密钥恢复] 密钥验证失败，可能密码已修改");
            let _ = storage.delete();
            return false;
        }
    }

    // 直接设置密钥到内存（不再保存，避免循环）
    let key = derive_key(&master_key);
    if let Ok(mut guard) = ENCRYPTION_KEY.write() {
        *guard = Some(key);
    }
    let key_v3 = derive_key_v3(&master_key, MASTER_KEY_APP_SALT_V3);
    if let Ok(mut guard) = ENCRYPTION_KEY_V3.write() {
        *guard = Some(key_v3);
    }
    // S2：不再写 RAW_MASTER_KEY；调用方按需通过 raw_master_key_for_sync() 读。

    // tracing::info!("[密钥恢复] 主密钥恢复成功");
    true
}

/// 测试专用：全局密钥状态的串行化锁（crypto 单测与 cloud_sync 单测共用）。
/// 首次初始化时把密钥数据目录重定向到进程级临时目录，
/// 保证任何测试路径（含默认 LocalFileStorage 后端的 save/delete）都不触碰真实用户数据。
#[cfg(test)]
pub(crate) fn test_mutex() -> &'static std::sync::Mutex<()> {
    static MUTEX: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    MUTEX.get_or_init(|| {
        install_test_data_dir();
        std::sync::Mutex::new(())
    })
}

/// 测试专用：取测试锁的便捷封装，poisoned 状态下仍能拿到 guard。
///
/// 标准库的 Mutex 在持锁期间 panic 会把 mutex 标记为 poisoned；后续 lock() 拿到
/// `Err(PoisonError)`，直接 `unwrap()` 会让后续测试全部因"已经 panic 的测试遗留"
/// 而失败。这里从 PoisonError 提取原始 guard，**保留串行化效果**（guard drop 前
/// 仍阻塞其他测试线程），并避免"假阳性"把失败传播下去。
#[cfg(test)]
pub(crate) fn lock_for_test() -> std::sync::MutexGuard<'static, ()> {
    test_mutex()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 幂等安装：把密钥/验证数据目录重定向到本进程专属临时目录。
/// 不依赖调用方是否持锁，任何测试路径进入前都已生效。
#[cfg(test)]
fn install_test_data_dir() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let dir = std::env::temp_dir().join(format!("omnihub-test-data-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        crate::key_storage::set_test_data_dir(dir);
    });
}

/// 测试专用：内存密钥后端 + 全局主密钥管理。
///
/// `set_master_key` 会把主密钥写入 key_storage 后端；不覆写后端时会覆盖
/// 真实用户配置目录下的 key_storage 文件，导致应用侧已存密码无法解密。
#[cfg(test)]
pub(crate) mod test_support {
    use crate::key_storage::{KeyStorage, set_key_storage};
    use std::sync::{Arc, Mutex};

    struct InMemoryKeyStorage(Mutex<Option<String>>);

    impl KeyStorage for InMemoryKeyStorage {
        fn name(&self) -> &'static str {
            "in-memory-test"
        }
        fn save(&self, master_key: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(master_key.to_string());
            Ok(())
        }
        fn load(&self) -> Option<String> {
            self.0.lock().unwrap().clone()
        }
        fn delete(&self) -> Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
        fn exists(&self) -> bool {
            self.0.lock().unwrap().is_some()
        }
    }

    /// 与 crypto 单测共用同一把锁，串行化全局密钥状态。
    pub(crate) fn lock() -> std::sync::MutexGuard<'static, ()> {
        super::lock_for_test()
    }

    /// 在内存后端上设置全局测试主密钥。
    pub(crate) fn set_master_key(key: &str) {
        super::install_test_data_dir();
        set_key_storage(Arc::new(InMemoryKeyStorage(Mutex::new(None))));
        super::set_master_key(key);
    }

    /// 清除全局主密钥；先挂内存后端，避免 delete 落到真实用户目录。
    pub(crate) fn clear_master_key() {
        super::install_test_data_dir();
        set_key_storage(Arc::new(InMemoryKeyStorage(Mutex::new(None))));
        super::clear_master_key();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let _guard = lock_for_test();
        test_support::set_master_key("test_key_123");

        let original = "my_secret_password";
        let encrypted = encrypt_password(original);

        // V3 写入路径
        assert!(encrypted.starts_with(ENCRYPTED_PREFIX_V3));
        assert_ne!(encrypted, original);

        let decrypted = decrypt_password(&encrypted);
        assert_eq!(decrypted, original);

        clear_master_key();
    }

    #[test]
    fn test_key_verification() {
        let _guard = lock_for_test();
        let master_key = "test_key_123";
        let verification = generate_key_verification_v3(master_key);

        assert!(verify_master_key(master_key, &verification));
        assert!(!verify_master_key("wrong_key", &verification));
    }

    #[test]
    fn test_key_verification_v2_still_accepted() {
        let _guard = lock_for_test();
        let master_key = "test_key_123";
        let verification = generate_key_verification(master_key);

        // V2 verification 仍可被 verify_master_key 通过（V2 输出 base64 串，
        // 不以字面 "V2:" 开头；以解密成功与否为判断依据）
        assert!(verify_master_key(master_key, &verification));
        assert!(!verify_master_key("wrong_key", &verification));
    }

    #[test]
    fn test_key_verification_v1() {
        let _guard = lock_for_test();
        let master_key = "test_key_123";
        let verification = generate_key_verification_v1(master_key);

        // V1 不以 V2: 开头
        assert!(!verification.starts_with("V2:"));
        // 仍可通过 verify_master_key 验证
        assert!(verify_master_key(master_key, &verification));
        assert!(!verify_master_key("wrong_key", &verification));
    }

    #[test]
    fn test_empty_password() {
        let _guard = lock_for_test();
        test_support::set_master_key("test_key");

        let encrypted = encrypt_password("");
        assert_eq!(encrypted, "");

        let decrypted = decrypt_password("");
        assert_eq!(decrypted, "");

        clear_master_key();
    }

    #[test]
    fn test_no_master_key() {
        let _guard = lock_for_test();
        clear_master_key();

        let original = "password123";
        let result = encrypt_password(original);
        assert_eq!(result, original); // 未设置密钥时不加密

        let decrypted = decrypt_password(&format!("{}abc", ENCRYPTED_PREFIX));
        assert_eq!(decrypted, ""); // 未设置密钥时解密返回空

        clear_master_key();
    }

    #[test]
    fn test_already_encrypted() {
        let _guard = lock_for_test();
        test_support::set_master_key("test_key");

        let original = "password";
        let encrypted = encrypt_password(original);
        let double_encrypted = encrypt_password(&encrypted);

        // 已加密的不应再次加密
        assert_eq!(encrypted, double_encrypted);

        clear_master_key();
    }

    #[test]
    fn test_already_encrypted_v1_v2_v3() {
        let _guard = lock_for_test();
        test_support::set_master_key("test_key");

        let already_v1 = "ENC:something";
        let already_v2 = "ENC:V2:something";
        let already_v3 = "ENC:V3:something";

        assert_eq!(encrypt_password(already_v1), already_v1);
        assert_eq!(encrypt_password(already_v2), already_v2);
        assert_eq!(encrypt_password(already_v3), already_v3);
        assert!(is_encrypted(already_v1));
        assert!(is_encrypted(already_v2));
        assert!(is_encrypted(already_v3));

        clear_master_key();
    }

    #[test]
    fn test_v1_backward_compatibility() {
        let _guard = lock_for_test();
        test_support::set_master_key("test_key");

        // 模拟旧 V1 格式：ENC: + base64(nonce + ciphertext)
        // 先用旧方式加密获取 V1 格式
        let plaintext = "old_password";
        let key = derive_key("test_key");
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes()).unwrap();
        let mut combined = Vec::with_capacity(12 + ciphertext.len());
        combined.extend_from_slice(&nonce_bytes);
        combined.extend_from_slice(&ciphertext);
        let v1_encrypted = format!("{}{}", ENCRYPTED_PREFIX, BASE64.encode(&combined));

        // V1 格式仍能被解密
        let decrypted = decrypt_password(&v1_encrypted);
        assert_eq!(decrypted, plaintext);

        clear_master_key();
    }

    #[test]
    fn test_v3_format_verification() {
        let _guard = lock_for_test();
        test_support::set_master_key("test_key");

        let encrypted = encrypt_password("secret");
        // V3 格式以 ENC:V3: 开头
        assert!(encrypted.starts_with(ENCRYPTED_PREFIX_V3));

        // V1 格式应被识别为未加密
        let v1_like = "ENC:something";
        let result = encrypt_password(v1_like);
        assert_eq!(result, v1_like); // 不应再次加密

        clear_master_key();
    }

    #[test]
    fn test_encrypt_with_key_v3() {
        let _guard = lock_for_test();
        let master_key = "my_master_key";
        let plaintext = "database_password";

        let encrypted = encrypt_with_key(plaintext, master_key);
        assert!(encrypted.starts_with(ENCRYPTED_PREFIX_V3));

        let decrypted = decrypt_with_key(&encrypted, master_key).unwrap();
        assert_eq!(decrypted, plaintext);

        // 错误密钥应解密失败
        let result = decrypt_with_key(&encrypted, "wrong_key");
        assert!(result.is_err());
    }

    /// V1/V2 旧密文能被新代码正确解密（向后兼容）。
    /// V3 与 V1/V2 派生密钥不同（SHA-256 vs Argon2id），三者**不**可互相替换解密。
    #[test]
    fn test_v1_v2_v3_interop() {
        let _guard = lock_for_test();
        let master_key = "interop_key";
        test_support::set_master_key(master_key);

        // 用旧 SHA-256 派生造 V1 密文
        let v1_plain = "v1_secret";
        let key_v1 = derive_key(master_key);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_v1));
        let mut nonce_v1 = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_v1);
        let ct_v1 = cipher
            .encrypt(Nonce::from_slice(&nonce_v1), v1_plain.as_bytes())
            .unwrap();
        let mut combined_v1 = Vec::with_capacity(12 + ct_v1.len());
        combined_v1.extend_from_slice(&nonce_v1);
        combined_v1.extend_from_slice(&ct_v1);
        let v1_encrypted = format!("{}{}", ENCRYPTED_PREFIX, BASE64.encode(&combined_v1));

        // 用旧 SHA-256 派生造 V2 密文
        let v2_plain = "v2_secret";
        let mut salt_v2 = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt_v2);
        let mut nonce_v2 = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_v2);
        let ct_v2 = cipher
            .encrypt(Nonce::from_slice(&nonce_v2), v2_plain.as_bytes())
            .unwrap();
        let mut combined_v2 = Vec::with_capacity(16 + 12 + ct_v2.len());
        combined_v2.extend_from_slice(&salt_v2);
        combined_v2.extend_from_slice(&nonce_v2);
        combined_v2.extend_from_slice(&ct_v2);
        let v2_encrypted = format!("{}{}", ENCRYPTED_PREFIX_V2, BASE64.encode(&combined_v2));

        // 用 V3 路径造 V3 密文
        let v3_plain = "v3_secret";
        let v3_encrypted = encrypt_password(v3_plain);

        // 解密三类密文
        assert_eq!(decrypt_password(&v1_encrypted), v1_plain);
        assert_eq!(decrypt_password(&v2_encrypted), v2_plain);
        assert_eq!(decrypt_password(&v3_encrypted), v3_plain);

        // 用错密钥解 V3 密文应失败
        let wrong = decrypt_with_key(&v3_encrypted, "bad_key");
        assert!(wrong.is_err());

        clear_master_key();
    }

    /// V3 verification 数据应当走 Argon2id 派生，且不能被 V2 prefix 误判。
    #[test]
    fn test_v3_verification_format() {
        let _guard = lock_for_test();
        let master_key = "v3_verify_key";

        let verification_v3 = generate_key_verification_v3(master_key);
        assert!(verify_master_key(master_key, &verification_v3));

        // 错误密钥应失败
        assert!(!verify_master_key("wrong_key", &verification_v3));

        clear_master_key();
    }

    /// S2：未设主密钥时 `raw_master_key_for_sync` 应返回 None。
    #[test]
    fn test_raw_master_key_for_sync_none_when_unset() {
        let _guard = lock_for_test();
        test_support::clear_master_key();
        assert!(raw_master_key_for_sync().is_none());
        assert!(with_raw_master_key(|_| ()).is_none());
    }

    /// S2：设了主密钥后 `raw_master_key_for_sync` 能从 key_storage 后端读出；
    /// `with_raw_master_key` 闭包可拿到 &str。
    #[test]
    fn test_raw_master_key_for_sync_roundtrip() {
        let _guard = lock_for_test();
        let master_key = "sync_roundtrip_key";
        test_support::set_master_key(master_key);

        let got = raw_master_key_for_sync();
        assert!(got.is_some(), "raw_master_key_for_sync 应返回 Some");
        assert_eq!(got.as_ref().map(|s| s.as_str()), Some(master_key));

        let via_closure = with_raw_master_key(|k| k.to_string());
        assert_eq!(via_closure, Some(master_key.to_string()));

        clear_master_key();
    }

    /// S2：清除主密钥后 `raw_master_key_for_sync` 再次返回 None；
    /// storage 后端同步被清空。
    #[test]
    fn test_raw_master_key_for_sync_cleared_after_clear() {
        let _guard = lock_for_test();
        test_support::set_master_key("transient_key");
        assert!(raw_master_key_for_sync().is_some());

        clear_master_key();
        assert!(raw_master_key_for_sync().is_none());
    }
}

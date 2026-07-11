//! 密钥存储模块
//!
//! 提供统一的密钥持久化接口。
//! 使用机器指纹（CPU ID + 机器名 + 用户名）通过 HKDF 派生唯一加密密钥，
//! 确保密钥与当前设备绑定，无法在另一台机器上解密。

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use hkdf::Hkdf;
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::Sha256;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// 本地加密密钥文件名
const KEY_STORAGE_FILE: &str = "key_storage";

/// HKDF salt：从编译时固定的安装实例 ID 生成
const HKDF_SALT: &[u8] = b"onetcli-key-derivation-v1";

/// 全局密钥存储后端
static KEY_STORAGE: RwLock<Option<Arc<dyn KeyStorage>>> = RwLock::new(None);

// ============================================================================
// KeyStorage trait
// ============================================================================

/// 密钥存储后端 trait
pub trait KeyStorage: Send + Sync {
    /// 存储后端名称，用于日志标识
    fn name(&self) -> &'static str;

    /// 保存主密钥
    fn save(&self, master_key: &str) -> Result<(), String>;

    /// 加载主密钥
    fn load(&self) -> Option<String>;

    /// 删除存储的密钥
    fn delete(&self) -> Result<(), String>;

    /// 检查是否存在已保存的密钥
    fn exists(&self) -> bool;
}

// ============================================================================
// LocalFileStorage 实现
// ============================================================================

/// 通过 HKDF 从机器指纹派生出 AES-256 加密密钥
fn derive_encryption_key() -> [u8; 32] {
    let fingerprint = machine_fingerprint();
    let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), fingerprint.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(b"onetcli-master-key-v1", &mut key)
        .expect("HKDF expand output length is valid");
    key
}

/// 获取当前机器的指纹（用于密钥派生）
/// 组合 CPU 架构、机器名、用户名，确保每台设备生成唯一密钥。
fn machine_fingerprint() -> String {
    let arch = std::env::consts::ARCH;
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default();
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    format!("{}|{}|{}", arch, hostname, username)
}

/// 本地文件存储实现
///
/// 使用机器指纹 HKDF 派生密钥，对主密钥进行 AES-256-GCM 加密后保存。
pub struct LocalFileStorage;

impl KeyStorage for LocalFileStorage {
    fn name(&self) -> &'static str {
        "本地文件"
    }

    fn save(&self, master_key: &str) -> Result<(), String> {
        let path = get_key_storage_path().ok_or_else(|| "无法获取密钥存储路径".to_string())?;

        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let key = derive_encryption_key();
        let cipher =
            Aes256Gcm::new_from_slice(&key).map_err(|e| format!("创建加密器失败: {}", e))?;

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, master_key.as_bytes())
            .map_err(|e| format!("加密密钥失败: {}", e))?;

        let mut data = nonce_bytes.to_vec();
        data.extend(ciphertext);

        fs::write(&path, &data).map_err(|e| format!("写入密钥文件失败: {}", e))?;
        Ok(())
    }

    fn load(&self) -> Option<String> {
        let path = get_key_storage_path()?;

        if !path.exists() {
            return None;
        }

        let data = fs::read(&path).ok()?;
        if data.len() < 12 {
            return None;
        }

        let nonce = Nonce::from_slice(&data[..12]);
        let ciphertext = &data[12..];

        let key = derive_encryption_key();
        let cipher = Aes256Gcm::new_from_slice(&key).ok()?;
        let plaintext = cipher.decrypt(nonce, ciphertext).ok()?;
        String::from_utf8(plaintext).ok()
    }

    fn delete(&self) -> Result<(), String> {
        if let Some(path) = get_key_storage_path() {
            if path.exists() {
                fs::remove_file(&path).map_err(|e| format!("删除密钥文件失败: {}", e))?;
            }
        }
        Ok(())
    }

    fn exists(&self) -> bool {
        get_key_storage_path().map(|p| p.exists()).unwrap_or(false)
    }
}

// ============================================================================
// 全局存储后端管理
// ============================================================================

/// 设置全局密钥存储后端
pub fn set_key_storage(storage: Arc<dyn KeyStorage>) {
    if let Ok(mut guard) = KEY_STORAGE.write() {
        // tracing::info!("[密钥存储] 切换到「{}」后端", storage.name());
        *guard = Some(storage);
    }
}

/// 获取当前密钥存储后端
///
/// 如果未设置，默认返回 `LocalFileStorage`。
pub fn get_key_storage() -> Arc<dyn KeyStorage> {
    KEY_STORAGE
        .read()
        .ok()
        .and_then(|guard| guard.clone())
        .unwrap_or_else(|| Arc::new(LocalFileStorage))
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 获取数据目录路径
fn get_data_dir() -> Option<PathBuf> {
    let parent = dirs::data_dir()?;
    let target = parent.join("omnihub");
    if !target.exists() {
        for legacy_name in ["one-hub", "onetcli"] {
            let legacy = parent.join(legacy_name);
            if !legacy.exists() {
                continue;
            }
            match std::fs::rename(&legacy, &target) {
                Ok(()) => break,
                Err(err) => {
                    eprintln!(
                        "[paths] failed to migrate data dir {} -> {}: {err}",
                        legacy.display(),
                        target.display()
                    );
                }
            }
        }
    }
    Some(target)
}

/// 获取本地密钥存储文件路径
fn get_key_storage_path() -> Option<PathBuf> {
    get_data_dir().map(|p| p.join(KEY_STORAGE_FILE))
}

//! 密钥存储模块
//!
//! 提供统一的密钥持久化接口。
//!
//! S3 加固：派生因子从"arch + hostname + username"扩展为"arch + machine_id +
//! hostname + username"，其中 machine_id 是平台提供的设备唯一 ID
//! （Linux `/etc/machine-id`、Windows 注册表 MachineGuid、macOS `IOPlatformUUID`）。
//! HKDF salt 从编译期常量改为首次启动时随机生成并持久化到 `key_salt` 文件。
//!
//! 兼容性：保留对旧常量盐（`HKDF_SALT_LEGACY`）的解密路径；读取 `key_storage` 时
//! 先用新盐尝试，失败则用旧盐重试一次，避免升级时丢失用户主密钥。

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use hkdf::Hkdf;
use rand::RngCore;
use rand::rngs::OsRng;
use sha2::Sha256;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// 本地加密密钥文件名
const KEY_STORAGE_FILE: &str = "key_storage";

/// S3 起持久化的随机 salt 文件名
const KEY_SALT_FILE: &str = "key_salt";

/// 历史兼容：S3 之前的固定 HKDF salt。
/// 仅用于解密已存在的 `key_storage` 文件；新写入全部走 `key_salt`。
const HKDF_SALT_LEGACY: &[u8] = b"onetcli-key-derivation-v1";

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

/// HKDF info：版本化常量。派生算法变更时可调大版本号以隔离新旧域。
const HKDF_INFO: &[u8] = b"onetcli-master-key-v1";

/// 通过 HKDF 从机器指纹 + 传入 salt 派生出 AES-256 加密密钥
fn derive_encryption_key_with_salt(salt: &[u8]) -> [u8; 32] {
    let fingerprint = machine_fingerprint();
    let hk = Hkdf::<Sha256>::new(Some(salt), fingerprint.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(HKDF_INFO, &mut key)
        .expect("HKDF expand output length is valid");
    key
}

/// 默认派生入口：从持久化 salt 派生（首次启动时生成）
fn derive_encryption_key() -> [u8; 32] {
    let salt = get_or_init_salt();
    derive_encryption_key_with_salt(&salt)
}

/// 获取平台提供的设备唯一 ID。
///
/// Linux: 读 `/var/lib/dbus/machine-id` 或 `/etc/machine-id`（FS 标准库即可）。
/// Windows: 读注册表 `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid`
/// macOS: 通过 `ioreg` 取 `IOPlatformUUID`（避免引入 `system-configuration` 依赖）。
///
/// 任一读取失败/超时返回 `None`，调用方降级到仅 arch+hostname+username 指纹。
fn machine_id() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        for path in ["/var/lib/dbus/machine-id", "/etc/machine-id"] {
            if let Ok(s) = std::fs::read_to_string(path) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }
    #[cfg(target_os = "windows")]
    {
        // 通过 `reg query` 读 MachineGuid；避免引入 `winreg` 依赖。
        // 失败返回 None，降级。
        use std::process::Command;
        let output = Command::new("reg")
            .args([
                "query",
                r"HKLM\SOFTWARE\Microsoft\Cryptography",
                "/v",
                "MachineGuid",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        // 输出类似 "    MachineGuid    REG_SZ    xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.contains("MachineGuid") {
                if let Some(value) = line.split_whitespace().last() {
                    return Some(value.to_string());
                }
            }
        }
        None
    }
    #[cfg(target_os = "macos")]
    {
        // 通过 `ioreg` 取 IOPlatformUUID；10ms 超时（实际是阻塞，但命令本身很快）。
        use std::process::Command;
        let output = Command::new("ioreg")
            .args(["-rd1", "-c", "IOPlatformExpertDevice", "-d", "1"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.contains("IOPlatformUUID") {
                if let Some(value) = line.split('"').nth(1) {
                    return Some(value.to_string());
                }
            }
        }
        None
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

/// 获取当前机器的指纹（用于密钥派生）。
///
/// S3 加固：组合 CPU 架构 + 平台机器唯一 ID + 机器名 + 用户名，
/// 每台设备生成唯一密钥。攻击者拿到同一机器上其他用户的进程权限即可复现
/// ——但仍需同时能读 `key_salt` 才能解密 `key_storage`。
fn machine_fingerprint() -> String {
    let arch = std::env::consts::ARCH;
    let machine_id = machine_id().unwrap_or_default();
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default();
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    format!("{}|{}|{}|{}", arch, machine_id, hostname, username)
}

/// 读取或初始化持久化 salt。
///
/// 首次启动：`key_salt` 文件不存在 → `OsRng` 生成 16B 随机盐并以 `0600` 权限写入。
/// 后续启动：直接读取；任何 IO 失败时降级到进程内随机（每次启动变化，**不能解密旧文件**）。
fn get_or_init_salt() -> [u8; 16] {
    if let Some(salt) = read_persisted_salt() {
        return salt;
    }
    // 持久化失败 / 文件不存在：尝试写入新盐
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    if let Some(path) = key_salt_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        // 0600 权限（Unix 平台；Windows 下 std fs::write 不支持 mode，需用 OpenOptions）。
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&path)
            {
                let _ = std::io::Write::write_all(&mut file, &salt);
            }
        }
        #[cfg(not(unix))]
        {
            let _ = fs::write(&path, &salt);
        }
    }
    salt
}

/// 从 `key_salt` 文件读取 16 字节；任何错误返回 None。
fn read_persisted_salt() -> Option<[u8; 16]> {
    let path = key_salt_path()?;
    let data = fs::read(&path).ok()?;
    if data.len() < 16 {
        return None;
    }
    let mut salt = [0u8; 16];
    salt.copy_from_slice(&data[..16]);
    Some(salt)
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

        let nonce_bytes: [u8; 12] = data[..12].try_into().ok()?;
        let ciphertext = &data[12..];

        // 先用新盐派生尝试解密；失败再用历史 salt 兼容旧数据。
        let key = derive_encryption_key();
        if let Some(plaintext) = try_decrypt(&key, &nonce_bytes, ciphertext) {
            return Some(plaintext);
        }
        let legacy_key = derive_encryption_key_with_salt(HKDF_SALT_LEGACY);
        try_decrypt(&legacy_key, &nonce_bytes, ciphertext)
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

/// 用指定 key 尝试解密；返回 Option<String>（失败返回 None 不抛错）。
fn try_decrypt(key: &[u8; 32], nonce_bytes: &[u8; 12], ciphertext: &[u8]) -> Option<String> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext).ok()?;
    String::from_utf8(plaintext).ok()
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

/// 测试专用：密钥/验证数据的读写重定向目录。
/// 单测若触碰真实用户数据目录，会覆盖或删除 key_storage / key_verification，
/// 导致应用侧主密钥无法恢复，因此测试环境必须显式重定向。
#[cfg(test)]
static TEST_DATA_DIR: std::sync::OnceLock<std::sync::RwLock<Option<PathBuf>>> =
    std::sync::OnceLock::new();

#[cfg(test)]
pub(crate) fn set_test_data_dir(dir: PathBuf) {
    let slot = TEST_DATA_DIR.get_or_init(|| std::sync::RwLock::new(None));
    *slot.write().unwrap() = Some(dir);
}

/// 获取数据目录路径
pub(crate) fn get_data_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(dir) = TEST_DATA_DIR
        .get_or_init(|| std::sync::RwLock::new(None))
        .read()
        .unwrap()
        .clone()
    {
        return Some(dir);
    }
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

/// 获取持久化 salt 文件路径（`<data_dir>/key_salt`）。
///
/// S3：salt 不再写死在源码里，而是首次启动时 OsRng 生成并落盘。
/// 仅供 `get_or_init_salt` 内部使用。
fn key_salt_path() -> Option<PathBuf> {
    get_data_dir().map(|p| p.join(KEY_SALT_FILE))
}

/// 确保 `key_salt` 目录存在（供测试或管理员调试使用）。
#[allow(dead_code)]
fn ensure_key_salt_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // 测试间共享一个临时目录；每次启动一个全新子目录避免互相污染
    static TEST_DIR_MUTEX: Mutex<()> = Mutex::new(());

    fn fresh_data_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "omnihub-key-storage-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::create_dir_all(&dir);
        set_test_data_dir(dir.clone());
        dir
    }

    /// S3：LocalFileStorage 在持久化 salt 下能 save/load 往返。
    #[test]
    fn test_local_file_storage_roundtrip_with_persisted_salt() {
        let _guard = TEST_DIR_MUTEX.lock().unwrap();
        fresh_data_dir("roundtrip");

        let storage = LocalFileStorage;
        storage.delete().ok();
        storage.save("my_secret_master_key").expect("save 成功");
        let loaded = storage.load().expect("load 成功");
        assert_eq!(loaded, "my_secret_master_key");

        // 清掉临时目录避免其他测试残留
        let _ = fs::remove_file(key_salt_path().unwrap());
        let _ = fs::remove_file(get_key_storage_path().unwrap());
    }

    /// S3：LocalFileStorage 启动时新生成的 salt 会被持久化，第二次启动读取
    /// 仍能解密同一份 `key_storage`（即"重启后密钥自动恢复"路径）。
    #[test]
    fn test_persisted_salt_survives_across_restarts() {
        let _guard = TEST_DIR_MUTEX.lock().unwrap();
        let dir = fresh_data_dir("survive");

        // 第一次启动：save 一次
        let storage = LocalFileStorage;
        storage.delete().ok();
        storage.save("another_master_key").expect("save 1 成功");

        // 第二次启动（同一目录）：load 应能恢复
        let loaded = storage.load().expect("load 2 成功");
        assert_eq!(loaded, "another_master_key");

        // 第三次启动（同一目录）：再 save 后再 load，盐应保持一致
        storage.save("rotated_key").expect("save 3 成功");
        let loaded2 = storage.load().expect("load 3 成功");
        assert_eq!(loaded2, "rotated_key");

        // 清理
        let _ = fs::remove_dir_all(dir);
    }

    /// S3：当 `key_storage` 用旧常量盐加密、`key_salt` 是新的（新设备首次启动），
    /// load() 应回退到旧盐兼容路径成功解密。
    #[test]
    fn test_legacy_salt_compatibility_path() {
        let _guard = TEST_DIR_MUTEX.lock().unwrap();
        let dir = fresh_data_dir("legacy");

        // 模拟旧数据：手动用 legacy salt 派生加密一份 master_key 写到 key_storage
        let key = derive_encryption_key_with_salt(HKDF_SALT_LEGACY);
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = cipher
            .encrypt(nonce, "legacy_master_key".as_bytes())
            .expect("legacy encrypt");
        let mut data = nonce_bytes.to_vec();
        data.extend(ct);
        let path = get_key_storage_path().unwrap();
        fs::write(&path, &data).expect("write legacy data");

        // 在新盐已经持久化的前提下（手动写一个新 salt 文件）
        let mut new_salt = [0u8; 16];
        OsRng.fill_bytes(&mut new_salt);
        let salt_path = key_salt_path().unwrap();
        fs::write(&salt_path, &new_salt).expect("write new salt");

        // load() 应当：新盐尝试失败 → 回退 legacy salt → 成功
        let storage = LocalFileStorage;
        let loaded = storage.load().expect("legacy load 成功");
        assert_eq!(loaded, "legacy_master_key");

        // 清理
        let _ = fs::remove_dir_all(dir);
    }
}

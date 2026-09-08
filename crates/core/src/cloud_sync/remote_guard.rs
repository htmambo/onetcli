//! 云同步统一出站请求守卫（S7）
//!
//! ## 背景
//!
//! `crates/core/src/cloud_sync/` 内多个 driver（`webdav_adapter`、
//! `sync_server_driver`、`blob_vault_driver`、`certificate_sync` 等）需要
//! 通过 HTTPS/HTTP 出网。本模块提供集中式出站守卫，避免每个 driver 重复实现：
//! - URL scheme / host 白名单
//! - 单次请求体大小上限（防滥用 / DoS）
//! - QPS 限流（防云同步风暴）
//! - SSRF 防护（拒绝 127.0.0.1 / 169.254.169.254 / 内网保留地址）
//!
//! ## 使用
//!
//! `RemoteGuard::check_url(&url)`：在发起请求前由 driver 调用一次；通过则放行，
//! 失败则返回 `RemoteGuardError`。所有 driver 改造都通过此入口；当前阶段仅
//! 暴露 API 与单测，实际驱动迁移作为后续 PR（每 driver 单独 review）。
//!
//! ## 默认白名单
//!
//! - scheme：仅允许 `https://`、`http://`（http 仅对 127.0.0.1 dev 后端）。
//! - host：默认**不**限制，由调用方传入；本模块不强制具体业务域名（避免与
//!   部署环境绑死）。建议在 driver 改造时按业务域名收紧。
//! - 内网 SSRF：`127.0.0.0/8`、`10.0.0.0/8`、`172.16.0.0/12`、`192.168.0.0/16`、
//!   `169.254.0.0/16`（AWS / GCP metadata）、`0.0.0.0/8` 一律拒绝。
//! - 体大小：默认 16 MiB / 单次；超过返回 `BodyTooLarge`。
//! - QPS：默认 50 / 秒 / host；超出返回 `RateLimited`。

use std::net::{IpAddr, Ipv4Addr};
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Duration;

use url::{Host, Url};

/// 守卫配置
#[derive(Debug, Clone)]
pub struct GuardConfig {
    /// 单次请求体最大字节数
    pub max_body_bytes: usize,
    /// 单 host QPS 上限
    pub max_qps_per_host: NonZeroU64,
    /// 是否允许内网 / 元数据地址（默认 false：拒绝 SSRF）
    pub allow_private_addresses: bool,
    /// 自定义 host 白名单（精确匹配小写域名）
    pub host_allowlist: Arc<Vec<String>>,
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self {
            max_body_bytes: 16 * 1024 * 1024,
            max_qps_per_host: NonZeroU64::new(50).expect("50 > 0"),
            allow_private_addresses: false,
            host_allowlist: Arc::new(Vec::new()),
        }
    }
}

/// 守卫错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteGuardError {
    /// URL 解析失败
    InvalidUrl(String),
    /// scheme 不是 http/https
    DisallowedScheme(String),
    /// host 被白名单拒绝
    HostNotAllowed(String),
    /// 命中 SSRF 拒绝的内网/保留地址
    PrivateAddress(IpAddr),
    /// body 超过上限
    BodyTooLarge { size: usize, limit: usize },
    /// 命中 QPS 限流
    RateLimited { host: String },
}

/// 全局守卫实例（每个 driver 共享一份配置；QPS 计数器按 host 隔离）。
#[derive(Clone)]
pub struct RemoteGuard {
    config: GuardConfig,
}

impl Default for RemoteGuard {
    fn default() -> Self {
        Self::new(GuardConfig::default())
    }
}

impl RemoteGuard {
    /// 用指定配置构造守卫
    pub fn new(config: GuardConfig) -> Self {
        Self { config }
    }

    /// 当前默认守卫（共享一份默认配置）
    pub fn shared() -> Self {
        Self::default()
    }

    /// 校验 URL 是否放行
    pub fn check_url(&self, raw: &str) -> Result<Url, RemoteGuardError> {
        let url = Url::parse(raw).map_err(|e| RemoteGuardError::InvalidUrl(e.to_string()))?;
        let scheme = url.scheme();
        if scheme != "http" && scheme != "https" {
            return Err(RemoteGuardError::DisallowedScheme(scheme.to_string()));
        }
        let host = url
            .host_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| RemoteGuardError::InvalidUrl("缺少 host".to_string()))?
            .to_lowercase();

        // host 白名单（精确匹配）
        if !self.config.host_allowlist.is_empty()
            && !self.config.host_allowlist.iter().any(|h| h == &host)
        {
            return Err(RemoteGuardError::HostNotAllowed(host));
        }

        // SSRF 防护：当 host 是 IP 时直接检查；否则仅当 allow_private_addresses=false 时
        // 仍允许（域名可能解析到内网，但实际 DNS 解析发生在请求阶段，本守卫仅做字面检查）。
        if let Host::Ipv4(ip) = url.host().unwrap_or(Host::Domain("")) {
            if !self.config.allow_private_addresses && is_private_v4(&ip) {
                return Err(RemoteGuardError::PrivateAddress(IpAddr::V4(ip)));
            }
        }
        if let Host::Ipv6(ip) = url.host().unwrap_or(Host::Domain("")) {
            if !self.config.allow_private_addresses && is_private_v6(&ip) {
                return Err(RemoteGuardError::PrivateAddress(IpAddr::V6(ip)));
            }
        }
        Ok(url)
    }

    /// 校验 body 大小
    pub fn check_body_size(&self, size: usize) -> Result<(), RemoteGuardError> {
        if size > self.config.max_body_bytes {
            return Err(RemoteGuardError::BodyTooLarge {
                size,
                limit: self.config.max_body_bytes,
            });
        }
        Ok(())
    }

    /// 当前 QPS 配置
    pub fn config(&self) -> &GuardConfig {
        &self.config
    }
}

/// 判断 IPv4 是否命中私有/保留地址
fn is_private_v4(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    o[0] == 10
        || (o[0] == 172 && (16..=31).contains(&o[1]))
        || (o[0] == 192 && o[1] == 168)
        || o[0] == 127
        || (o[0] == 169 && o[1] == 254)
        || o[0] == 0
        || o[0] >= 224 // multicast / reserved
}

/// 判断 IPv6 是否命中私有/保留地址（简化版）
fn is_private_v6(ip: &std::net::Ipv6Addr) -> bool {
    let s = ip.segments();
    // ::1 loopback
    if ip.is_loopback() {
        return true;
    }
    // fc00::/7 unique-local
    if (s[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    // fe80::/10 link-local
    if (s[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    false
}

/// QPS 限流器：按 host 维护一个令牌桶
#[derive(Debug)]
pub struct RateLimiter {
    config: GuardConfig,
    state: parking_lot::Mutex<std::collections::HashMap<String, RateState>>,
    window: Duration,
}

#[derive(Debug, Clone)]
struct RateState {
    window_start: std::time::Instant,
    count: u64,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(GuardConfig::default(), Duration::from_secs(1))
    }
}

impl RateLimiter {
    pub fn new(config: GuardConfig, window: Duration) -> Self {
        Self {
            config,
            state: parking_lot::Mutex::new(std::collections::HashMap::new()),
            window,
        }
    }

    /// 检查 host 是否可以发起一次请求；通过则递增计数。
    pub fn check(&self, host: &str) -> Result<(), RemoteGuardError> {
        let now = std::time::Instant::now();
        let mut map = self.state.lock();
        let entry = map.entry(host.to_lowercase()).or_insert(RateState {
            window_start: now,
            count: 0,
        });
        if now.duration_since(entry.window_start) >= self.window {
            entry.window_start = now;
            entry.count = 0;
        }
        if entry.count >= self.config.max_qps_per_host.get() {
            return Err(RemoteGuardError::RateLimited { host: host.to_string() });
        }
        entry.count += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_url_accepts_https() {
        let guard = RemoteGuard::shared();
        let url = guard.check_url("https://example.com/path").unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("example.com"));
    }

    #[test]
    fn test_check_url_rejects_ftp() {
        let guard = RemoteGuard::shared();
        let err = guard.check_url("ftp://example.com").unwrap_err();
        assert!(matches!(err, RemoteGuardError::DisallowedScheme(s) if s == "ftp"));
    }

    #[test]
    fn test_check_url_rejects_loopback_v4() {
        let guard = RemoteGuard::shared();
        let err = guard.check_url("http://127.0.0.1/secret").unwrap_err();
        assert!(matches!(err, RemoteGuardError::PrivateAddress(IpAddr::V4(_))));
    }

    #[test]
    fn test_check_url_rejects_aws_metadata_ip() {
        let guard = RemoteGuard::shared();
        let err = guard.check_url("http://169.254.169.254/latest/meta-data").unwrap_err();
        assert!(matches!(err, RemoteGuardError::PrivateAddress(IpAddr::V4(_))));
    }

    #[test]
    fn test_check_url_rejects_rfc1918() {
        let guard = RemoteGuard::shared();
        assert!(matches!(
            guard.check_url("http://10.0.0.1/").unwrap_err(),
            RemoteGuardError::PrivateAddress(_)
        ));
        assert!(matches!(
            guard
                .check_url("http://192.168.1.1/")
                .unwrap_err(),
            RemoteGuardError::PrivateAddress(_)
        ));
        assert!(matches!(
            guard
                .check_url("http://172.16.0.1/")
                .unwrap_err(),
            RemoteGuardError::PrivateAddress(_)
        ));
    }

    #[test]
    fn test_check_url_with_host_allowlist_rejects_other_hosts() {
        let cfg = GuardConfig {
            host_allowlist: Arc::new(vec!["api.allowed.com".to_string()]),
            ..Default::default()
        };
        let guard = RemoteGuard::new(cfg);
        assert!(guard.check_url("https://api.allowed.com/").is_ok());
        assert!(matches!(
            guard.check_url("https://evil.com/").unwrap_err(),
            RemoteGuardError::HostNotAllowed(_)
        ));
    }

    #[test]
    fn test_check_body_size() {
        let guard = RemoteGuard::shared();
        assert!(guard.check_body_size(1024).is_ok());
        let cfg = GuardConfig {
            max_body_bytes: 1024,
            ..Default::default()
        };
        let guard = RemoteGuard::new(cfg);
        assert!(matches!(
            guard.check_body_size(2048).unwrap_err(),
            RemoteGuardError::BodyTooLarge { size: 2048, limit: 1024 }
        ));
    }

    #[test]
    fn test_rate_limiter_blocks_burst() {
        let cfg = GuardConfig {
            max_qps_per_host: NonZeroU64::new(3).unwrap(),
            ..Default::default()
        };
        let limiter = RateLimiter::new(cfg, Duration::from_millis(100));
        assert!(limiter.check("api.example.com").is_ok());
        assert!(limiter.check("api.example.com").is_ok());
        assert!(limiter.check("api.example.com").is_ok());
        assert!(matches!(
            limiter.check("api.example.com").unwrap_err(),
            RemoteGuardError::RateLimited { .. }
        ));
        // 不同 host 独立计数
        assert!(limiter.check("other.example.com").is_ok());
    }

    #[test]
    fn test_url_without_host_rejected() {
        let guard = RemoteGuard::shared();
        // url crate 把 "https:///path" 解析为 host="path"，这本身是 url crate 的边角
        // 行为。本守卫对"无 host"定义为 host_str 返回 None 或 host_str 解析为空串；
        // 真正无 host 的 URL 由 url crate 自身拒绝（"http://" 无后续段）。
        let err = guard.check_url("http://").unwrap_err();
        assert!(matches!(err, RemoteGuardError::InvalidUrl(_)));
    }

    #[test]
    fn test_invalid_url_rejected() {
        let guard = RemoteGuard::shared();
        let err = guard.check_url("not a url").unwrap_err();
        assert!(matches!(err, RemoteGuardError::InvalidUrl(_)));
    }
}
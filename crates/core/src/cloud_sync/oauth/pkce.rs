//! PKCE (Proof Key for Code Exchange) 辅助函数
//!
//! 实现 RFC 7636 定义的 PKCE 流程：
//! 1. 生成 code_verifier（随机字符串，43-128字符）
//! 2. 计算 code_challenge = BASE64URL(SHA256(code_verifier))

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;

/// 生成 PKCE code_verifier
///
/// 随机字符串，43-128字符，包含 [A-Z] / [a-z] / [0-9] / "-" / "." / "_" / "~"
pub fn generate_code_verifier() -> String {
    // 43-128 随机字符，PKCE 要求最小 43 字符
    let mut rng = rand::thread_rng();
    let len = rng.gen_range(43..=128);
    let chars: String = (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..66);
            if idx < 26 {
                (b'A' + idx) as char
            } else if idx < 52 {
                (b'a' + idx - 26) as char
            } else if idx < 62 {
                (b'0' + idx - 52) as char
            } else if idx == 62 {
                '-'
            } else if idx == 63 {
                '.'
            } else if idx == 64 {
                '_'
            } else {
                '~'
            }
        })
        .collect();
    chars
}

/// 计算 PKCE code_challenge
///
/// code_challenge = BASE64URL(SHA256(code_verifier))
pub fn generate_code_challenge(verifier: &str) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_verifier_length() {
        for _ in 0..100 {
            let v = generate_code_verifier();
            assert!(v.len() >= 43, "verifier too short: {}", v.len());
            assert!(v.len() <= 128, "verifier too long: {}", v.len());
        }
    }

    #[test]
    fn test_code_verifier_valid_chars() {
        let v = generate_code_verifier();
        for c in v.chars() {
            assert!(
                c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '~',
                "invalid char: {}",
                c
            );
        }
    }

    #[test]
    fn test_code_challenge_consistent() {
        let verifier = "test_verifier_string_12345678901234567890123456789012345";
        let c1 = generate_code_challenge(verifier);
        let c2 = generate_code_challenge(verifier);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_code_challenge_different_for_different_verifiers() {
        let c1 = generate_code_challenge("verifier_one_1234567890123456789012345678901234567");
        let c2 = generate_code_challenge("verifier_two_1234567890123456789012345678901234568");
        assert_ne!(c1, c2);
    }
}

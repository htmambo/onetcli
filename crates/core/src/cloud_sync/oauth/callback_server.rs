//! 本地 OAuth 回调服务器
//!
//! 用于 PKCE 授权码流程中，接收 OAuth provider 的回调。
//!
//! ## 使用流程
//! 1. 启动服务器（随机可用端口）
//! 2. 打开浏览器到授权 URL
//! 3. 等待回调（含 auth code）
//! 4. 返回 auth code，关闭服务器

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

/// 回调服务器错误
#[derive(Debug)]
pub enum CallbackServerError {
    /// 找不到可用端口
    NoAvailablePort,
    /// 服务器启动失败
    StartFailed(String),
    /// 超时
    Timeout,
    /// 连接错误
    ConnectionError(String),
}

impl std::fmt::Display for CallbackServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAvailablePort => write!(f, "找不到可用端口"),
            Self::StartFailed(msg) => write!(f, "服务器启动失败: {}", msg),
            Self::Timeout => write!(f, "回调超时"),
            Self::ConnectionError(msg) => write!(f, "连接错误: {}", msg),
        }
    }
}

/// OAuth 回调结果
#[derive(Debug)]
pub struct OAuthCallback {
    /// 授权码
    pub code: String,
    /// 状态参数（原样返回）
    pub state: Option<String>,
    /// 错误（如果有）
    pub error: Option<String>,
    /// 错误描述（如果有）
    pub error_description: Option<String>,
}

/// 启动本地回调服务器
///
/// `port` 为 0 时自动选择可用端口。
/// 返回实际使用的端口和 auth code。
pub fn start_callback_server(
    port: u16,
    timeout_secs: u64,
) -> Result<(u16, OAuthCallback), CallbackServerError> {
    // 尝试绑定端口
    let addr = if port == 0 {
        // 尝试随机端口
        (0..100)
            .find_map(|_| {
                let p = 15000 + rand::random::<u16>() % 50000;
                TcpListener::bind(("127.0.0.1", p)).ok().map(|l| (p, l))
            })
            .ok_or(CallbackServerError::NoAvailablePort)?
    } else {
        let listener =
            TcpListener::bind(("127.0.0.1", port)).map_err(|e| CallbackServerError::StartFailed(e.to_string()))?;
        let local_addr = listener.local_addr().map_err(|e| CallbackServerError::StartFailed(e.to_string()))?;
        (local_addr.port(), listener)
    };

    let (actual_port, listener) = addr;
    listener
        .set_nonblocking(false)
        .map_err(|e| CallbackServerError::StartFailed(e.to_string()))?;

    let timeout = Duration::from_secs(timeout_secs);
    let deadline = std::time::Instant::now() + timeout;

    // 接受一个连接
    let mut stream = match accept_with_deadline(&listener, deadline) {
        Ok(s) => s,
        Err(e) => return Err(e),
    };

    // 读取 HTTP 请求
    let mut buffer = [0u8; 4096];
    let _ = stream.read(&mut buffer);

    // 解析 auth code
    let callback = parse_callback_from_request(&buffer);

    // 返回成功页面
    let response = if callback.error.is_none() {
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/html\r\n\
         Content-Length: 60\r\n\
         \r\n\
         <html><body>授权成功，可以关闭此页面。</body></html>"
    } else {
        "HTTP/1.1 400 Bad Request\r\n\
         Content-Type: text/html\r\n\
         Content-Length: 80\r\n\
         \r\n\
         <html><body>授权失败，请关闭页面重试。</body></html>"
    };
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    Ok((actual_port, callback))
}

/// 带超时的 accept
fn accept_with_deadline(
    listener: &TcpListener,
    deadline: std::time::Instant,
) -> Result<std::net::TcpStream, CallbackServerError> {
    loop {
        // 检查超时
        if std::time::Instant::now() >= deadline {
            return Err(CallbackServerError::Timeout);
        }

        // 尝试 accept
        match listener.accept() {
            Ok((stream, _)) => return Ok(stream),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(CallbackServerError::ConnectionError(e.to_string())),
        }
    }
}

/// 从 HTTP 请求中解析 OAuth 回调参数
fn parse_callback_from_request(buffer: &[u8]) -> OAuthCallback {
    let request = String::from_utf8_lossy(buffer);

    // 查找 ? 后的查询字符串
    let query_start = match request.find('?') {
        Some(pos) => pos + 1,
        None => return OAuthCallback::error("missing_params", "Missing query parameters"),
    };

    let query_end = request[query_start..]
        .find(' ')
        .map(|p| query_start + p)
        .unwrap_or(request.len());

    let query = &request[query_start..query_end];

    let mut code = None;
    let mut state = None;
    let mut error = None;
    let mut error_description = None;

    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("");
        let value = percent_decode(parts.next().unwrap_or(""));
        match key {
            "code" => code = Some(value),
            "state" => state = Some(value),
            "error" => error = Some(value),
            "error_description" => error_description = Some(value),
            _ => {}
        }
    }

    if let Some(e) = error {
        OAuthCallback {
            code: String::new(),
            state,
            error: Some(e),
            error_description,
        }
    } else if let Some(c) = code {
        OAuthCallback { code: c, state, error: None, error_description: None }
    } else {
        OAuthCallback::error("missing_code", "Authorization code not found in callback")
    }
}

/// 简单的 percent decode
fn percent_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            result.push('%');
            result.push_str(&hex);
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

impl OAuthCallback {
    fn error(code: &str, desc: &str) -> Self {
        Self {
            code: String::new(),
            state: None,
            error: Some(code.to_string()),
            error_description: Some(desc.to_string()),
        }
    }

    pub fn is_success(&self) -> bool {
        self.error.is_none() && !self.code.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_callback_success() {
        let request = b"GET /?code=abc123&state=xyz HTTP/1.1\r\n\r\n";
        let cb = parse_callback_from_request(request);
        assert!(cb.is_success());
        assert_eq!(cb.code, "abc123");
        assert_eq!(cb.state.as_deref(), Some("xyz"));
    }

    #[test]
    fn test_callback_error() {
        let request = b"GET /?error=access_denied&error_description=user+denied HTTP/1.1\r\n\r\n";
        let cb = parse_callback_from_request(request);
        assert!(!cb.is_success());
        assert_eq!(cb.error.as_deref(), Some("access_denied"));
        assert_eq!(cb.error_description.as_deref(), Some("user denied"));
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("a%2Fb%2Bc"), "a/b+c");
        assert_eq!(percent_decode("%%"), "%%");
    }
}

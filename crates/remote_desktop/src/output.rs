use crate::capabilities::RemoteDesktopCapabilities;
use crate::helper_protocol::FrameRect;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteDesktopOutput {
    Connected {
        width: u16,
        height: u16,
        capabilities: RemoteDesktopCapabilities,
    },
    Frame {
        width: u16,
        height: u16,
        rgba: Vec<u8>,
    },
    FrameBgra {
        width: u16,
        height: u16,
        bgra: Vec<u8>,
    },
    /// 增量帧：整帧尺寸 + 若干脏矩形，bgra 为各矩形按顺序拼接的 BGRA 字节。
    FrameRectsBgra {
        width: u16,
        height: u16,
        rects: Vec<FrameRect>,
        bgra: Vec<u8>,
    },
    CursorDefault,
    CursorHidden,
    CursorPosition {
        x: u16,
        y: u16,
    },
    ClipboardText {
        text: String,
    },
    Status(String),
    ConnectionFailure(String),
    Terminated(String),
}

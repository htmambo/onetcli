pub fn scale_coordinate(local: f32, local_size: f32, remote_size: u16) -> u16 {
    if local_size <= 0.0 || remote_size == 0 {
        return 0;
    }

    let scaled = (local / local_size) * remote_size as f32;
    scaled.clamp(0.0, remote_size.saturating_sub(1) as f32) as u16
}

pub fn scale_pointer_position(
    local_x: f32,
    local_y: f32,
    local_width: f32,
    local_height: f32,
    remote_width: u16,
    remote_height: u16,
) -> Option<(u16, u16)> {
    if local_width <= 0.0 || local_height <= 0.0 || remote_width == 0 || remote_height == 0 {
        return None;
    }

    let scale =
        (local_width / f32::from(remote_width)).min(local_height / f32::from(remote_height));
    let frame_width = f32::from(remote_width) * scale;
    let frame_height = f32::from(remote_height) * scale;
    let offset_x = (local_width - frame_width) / 2.0;
    let offset_y = (local_height - frame_height) / 2.0;
    let frame_x = local_x - offset_x;
    let frame_y = local_y - offset_y;

    if frame_x < 0.0 || frame_y < 0.0 || frame_x > frame_width || frame_y > frame_height {
        return None;
    }

    Some((
        scale_coordinate(frame_x, frame_width, remote_width),
        scale_coordinate(frame_y, frame_height, remote_height),
    ))
}

pub fn scale_filled_pointer_position(
    local_x: f32,
    local_y: f32,
    local_width: f32,
    local_height: f32,
    remote_width: u16,
    remote_height: u16,
) -> Option<(u16, u16)> {
    if local_width <= 0.0 || local_height <= 0.0 || remote_width == 0 || remote_height == 0 {
        return None;
    }

    Some((
        scale_coordinate(local_x, local_width, remote_width),
        scale_coordinate(local_y, local_height, remote_height),
    ))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalBounds {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

pub fn scale_window_pointer_position(
    window_x: f32,
    window_y: f32,
    bounds: LocalBounds,
    remote_width: u16,
    remote_height: u16,
) -> Option<(u16, u16)> {
    scale_pointer_position(
        window_x - bounds.left,
        window_y - bounds.top,
        bounds.width,
        bounds.height,
        remote_width,
        remote_height,
    )
}

pub fn scale_filled_window_pointer_position(
    window_x: f32,
    window_y: f32,
    bounds: LocalBounds,
    remote_width: u16,
    remote_height: u16,
) -> Option<(u16, u16)> {
    scale_filled_pointer_position(
        window_x - bounds.left,
        window_y - bounds.top,
        bounds.width,
        bounds.height,
        remote_width,
        remote_height,
    )
}

/// Cover 模式：等比缩放至填满视图（取较大缩放比，居中裁剪超出部分）。
/// 远端坐标 = (本地坐标 + 裁剪偏移) / 缩放比。
pub fn scale_cover_pointer_position(
    local_x: f32,
    local_y: f32,
    local_width: f32,
    local_height: f32,
    remote_width: u16,
    remote_height: u16,
) -> Option<(u16, u16)> {
    if local_width <= 0.0 || local_height <= 0.0 || remote_width == 0 || remote_height == 0 {
        return None;
    }

    let scale =
        (local_width / f32::from(remote_width)).max(local_height / f32::from(remote_height));
    // 图像按 scale 缩放后大于等于视图，居中放置，超出部分对称裁剪。
    let frame_width = f32::from(remote_width) * scale;
    let frame_height = f32::from(remote_height) * scale;
    // 图像左上角相对视图的偏移（≤0）。
    let offset_x = (local_width - frame_width) / 2.0;
    let offset_y = (local_height - frame_height) / 2.0;
    let frame_x = local_x - offset_x;
    let frame_y = local_y - offset_y;

    Some((
        scale_coordinate(frame_x, frame_width, remote_width),
        scale_coordinate(frame_y, frame_height, remote_height),
    ))
}

pub fn scale_cover_window_pointer_position(
    window_x: f32,
    window_y: f32,
    bounds: LocalBounds,
    remote_width: u16,
    remote_height: u16,
) -> Option<(u16, u16)> {
    scale_cover_pointer_position(
        window_x - bounds.left,
        window_y - bounds.top,
        bounds.width,
        bounds.height,
        remote_width,
        remote_height,
    )
}

/// Original(1:1) 模式：图像原始尺寸放入可滚动容器，本地坐标加滚动偏移即远端坐标。
/// 超出远端分辨率范围的坐标返回 None。
pub fn scale_original_pointer_position(
    local_x: f32,
    local_y: f32,
    scroll_x: f32,
    scroll_y: f32,
    remote_width: u16,
    remote_height: u16,
) -> Option<(u16, u16)> {
    if remote_width == 0 || remote_height == 0 {
        return None;
    }
    let x = local_x + scroll_x;
    let y = local_y + scroll_y;
    if x < 0.0 || y < 0.0 || x >= f32::from(remote_width) || y >= f32::from(remote_height) {
        return None;
    }
    Some((x as u16, y as u16))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_and_clamps_coordinate() {
        assert_eq!(500, scale_coordinate(50.0, 100.0, 1000));
        assert_eq!(0, scale_coordinate(-10.0, 100.0, 1000));
        assert_eq!(999, scale_coordinate(120.0, 100.0, 1000));
    }

    #[test]
    fn scales_pointer_inside_centered_letterboxed_frame() {
        assert_eq!(
            Some((640, 360)),
            scale_pointer_position(640.0, 360.0, 1280.0, 720.0, 1280, 720)
        );
        assert_eq!(
            Some((0, 0)),
            scale_pointer_position(280.0, 0.0, 1280.0, 720.0, 720, 720)
        );
        assert_eq!(
            None,
            scale_pointer_position(279.0, 0.0, 1280.0, 720.0, 720, 720)
        );
    }

    #[test]
    fn scales_cover_pointer_with_centered_crop() {
        // 视图 1280x720，远端 720x720（方形）：cover 取 width 方向缩放，
        // 图像宽铺满，高度超出被上下裁剪，中心点对中心点。
        // scale = max(1280/720, 720/720) = 1280/720 ≈ 1.778
        // frame = 720*1.778 = 1280x1280，offset_y = (720-1280)/2 = -280
        // 视图中心 (640, 360) -> frame_y = 360-(-280) = 640 -> remote 640/1280*720 = 360
        assert_eq!(
            Some((360, 360)),
            scale_cover_pointer_position(640.0, 360.0, 1280.0, 720.0, 720, 720)
        );
        // 视图左上角 (0,0) -> frame=(0,280) -> remote (0, 280/1280*720=157)
        assert_eq!(
            Some((0, 157)),
            scale_cover_pointer_position(0.0, 0.0, 1280.0, 720.0, 720, 720)
        );
    }

    #[test]
    fn cover_window_position_subtracts_bounds() {
        assert_eq!(
            Some((360, 360)),
            scale_cover_window_pointer_position(
                640.0,
                456.0,
                LocalBounds {
                    left: 0.0,
                    top: 96.0,
                    width: 1280.0,
                    height: 720.0,
                },
                720,
                720,
            )
        );
    }

    #[test]
    fn original_pointer_adds_scroll_offset() {
        // 1:1，滚动偏移 (100, 50)。
        assert_eq!(
            Some((200, 150)),
            scale_original_pointer_position(100.0, 100.0, 100.0, 50.0, 2880, 1620)
        );
        // 未滚动时直接坐标。
        assert_eq!(
            Some((400, 300)),
            scale_original_pointer_position(400.0, 300.0, 0.0, 0.0, 2880, 1620)
        );
        // 超出远端分辨率 -> None。
        assert_eq!(
            None,
            scale_original_pointer_position(2900.0, 0.0, 0.0, 0.0, 2880, 1620)
        );
        assert_eq!(
            None,
            scale_original_pointer_position(-1.0, 0.0, 0.0, 0.0, 2880, 1620)
        );
    }

    #[test]
    fn subtracts_content_bounds_before_scaling_window_position() {
        assert_eq!(
            Some((640, 360)),
            scale_window_pointer_position(
                640.0,
                456.0,
                LocalBounds {
                    left: 0.0,
                    top: 96.0,
                    width: 1280.0,
                    height: 720.0,
                },
                1280,
                720,
            )
        );
    }

    #[test]
    fn scales_filled_pointer_against_full_content_bounds() {
        assert_eq!(
            Some((0, 0)),
            scale_filled_pointer_position(0.0, 0.0, 1280.0, 720.0, 1024, 768)
        );
        assert_eq!(
            Some((512, 384)),
            scale_filled_pointer_position(640.0, 360.0, 1280.0, 720.0, 1024, 768)
        );
    }

    #[test]
    fn filled_window_position_subtracts_header_bounds() {
        assert_eq!(
            Some((512, 384)),
            scale_filled_window_pointer_position(
                640.0,
                456.0,
                LocalBounds {
                    left: 0.0,
                    top: 96.0,
                    width: 1280.0,
                    height: 720.0,
                },
                1024,
                768,
            )
        );
    }
}

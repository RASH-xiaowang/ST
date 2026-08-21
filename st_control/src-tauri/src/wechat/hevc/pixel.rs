// ============================================================
// wxgf / HEVC 图片转码 — 容器/像素/JPEG 域
// 自 hevc.rs 拆分：wxgf 头剥离、NV12→RGB、JPEG 编码。
// ============================================================

/// 剥掉 wxgf 头：找到第一个 VPS NAL 起始码（00 00 00 01 40 01）
pub(crate) fn strip_wxgf_header(data: &[u8]) -> Option<&[u8]> {
    if !data.starts_with(b"wxgf") {
        // 可能已是裸 HEVC 码流，直接找起始码
    }
    let pat = [0u8, 0, 0, 1, 0x40, 0x01];
    let pos = data.windows(pat.len()).position(|w| w == pat)?;
    Some(&data[pos..])
}

/// NV12 → RGB24（BT.601 limited range）
pub(crate) fn nv12_to_rgb(
    nv12: &[u8],
    width: usize,
    height: usize,
    stride: usize,
) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || stride < width {
        return None;
    }
    let y_size = stride.checked_mul(height)?;
    let uv_size = stride.checked_mul(height / 2)?;
    if nv12.len() < y_size + uv_size {
        return None;
    }
    let (y_plane, uv_plane) = nv12.split_at(y_size);
    let mut rgb = vec![0u8; width * height * 3];
    for row in 0..height {
        let y_row = &y_plane[row * stride..row * stride + width];
        let uv_row = &uv_plane[(row / 2) * stride..(row / 2) * stride + width];
        let out_row = &mut rgb[row * width * 3..(row + 1) * width * 3];
        for col in 0..width {
            let y = y_row[col] as i32;
            let u = uv_row[(col / 2) * 2] as i32 - 128;
            let v = uv_row[(col / 2) * 2 + 1] as i32 - 128;
            let r = y + ((91881 * v) >> 16);
            let g = y - ((22554 * u + 46802 * v) >> 16);
            let b = y + ((116130 * u) >> 16);
            out_row[col * 3] = r.clamp(0, 255) as u8;
            out_row[col * 3 + 1] = g.clamp(0, 255) as u8;
            out_row[col * 3 + 2] = b.clamp(0, 255) as u8;
        }
    }
    Some(rgb)
}

pub(crate) fn encode_jpeg(rgb: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let encoder = jpeg_encoder::Encoder::new(&mut out, 85);
    encoder
        .encode(
            rgb,
            width as u16,
            height as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .ok()?;
    Some(out)
}

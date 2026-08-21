//! wxgf / HEVC 图片转码为 JPEG（Windows Media Foundation）
//!
//! 微信 4.x 部分图片以 wxgf 格式存储：`wxgf` 自定义头 + 原始 HEVC
//! Annex-B 码流（无 HEIF 容器）。本模块：
//!   1. 剥掉 wxgf 头，定位第一个 VPS NAL（00 00 00 01 40 01）
//!   2. 通过系统 HEVC 解码器 MFT（HEVC Video Extensions / 硬件解码）解码出 NV12 帧
//!   3. NV12 → RGB（BT.601）→ JPEG（纯 Rust jpeg-encoder）
//!
//! 系统无 HEVC 解码能力或解码失败时返回 None，调用方回退占位显示，
//! 不影响其他格式图片。

#![cfg(target_os = "windows")]
#![allow(non_snake_case)]

mod mft;
pub(crate) use mft::decode_hevc_to_rgb;
mod sps;
pub(crate) use sps::parse_sps_dimensions;
mod pixel;
pub(crate) use pixel::{encode_jpeg, nv12_to_rgb, strip_wxgf_header};

/// wxgf 解密数据 → JPEG 字节；失败返回 None
pub fn wxgf_to_jpeg(data: &[u8]) -> Option<Vec<u8>> {
    let bitstream = strip_wxgf_header(data)?;
    if bitstream.len() < 64 {
        return None;
    }
    match unsafe { decode_hevc_to_rgb(bitstream) } {
        Some((rgb, w, h)) => encode_jpeg(&rgb, w, h),
        None => None,
    }
}

#[cfg(test)]
mod tests;

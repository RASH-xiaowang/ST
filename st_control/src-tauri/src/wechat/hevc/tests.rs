// ============================================================
// wxgf / HEVC 图片转码 — 测试
// 自 hevc.rs 拆分：头剥离 / Exp-Golomb / NV12→RGB。
// ============================================================

use super::*;

#[test]
fn test_strip_wxgf_header() {
    let mut data = b"wxgf\x13\x00\x02\x05".to_vec();
    data.extend_from_slice(&[0, 0, 0, 1, 0x40, 0x01, 0xAA, 0xBB]);
    let bs = strip_wxgf_header(&data).unwrap();
    assert_eq!(&bs[..6], &[0, 0, 0, 1, 0x40, 0x01]);
    assert_eq!(bs.len(), 8);
    assert!(strip_wxgf_header(b"jpeg-not-wxgf").is_none());
}

#[test]
fn test_bitreader_ue() {
    // ue(0)=1, ue(1)=010, ue(2)=011, ue(3)=00100
    let data = [0b1_010_011_0u8, 0b0100_0000u8];
    let mut r = super::sps::BitReader::new(&data);
    assert_eq!(r.ue(), Some(0));
    assert_eq!(r.ue(), Some(1));
    assert_eq!(r.ue(), Some(2));
    assert_eq!(r.ue(), Some(3));
}

#[test]
fn test_nv12_to_rgb_gray() {
    // 2x2 NV12：Y=128, U=V=128 → 中灰
    let nv12 = [128u8, 128, 128, 128, 128, 128];
    let rgb = nv12_to_rgb(&nv12, 2, 2, 2).unwrap();
    assert_eq!(rgb.len(), 12);
    for px in rgb.chunks(3) {
        assert!((px[0] as i32 - 128).abs() <= 1);
        assert!((px[1] as i32 - 128).abs() <= 1);
        assert!((px[2] as i32 - 128).abs() <= 1);
    }
}

// ============================================================
// 朋友圈图片解密模块 — 测试
// 自 sns_image.rs 拆分：密钥流参考值 / URL 拼接 / 格式嗅探。
// ============================================================

use super::*;

#[test]
fn keystream_matches_wechat_wasm() {
    // 参考值来自微信官方 wasm_video_decode.wasm（key=8318676762518462546）
    // 前 256 字节密钥流（WeFlow 反转后的 XOR 密钥流）
    let expected = hex::decode(
        "4d8238bef2ac3c83d719c9bf3eec2e054f1c7ec47d9007c98e3b46cb9a604e05\
         5fdbe8b3dc8ca254e72e5a22b4f5b29c957df08ed6ee13c84796579c66ddd07\
         6074d68d087d9f1a2789d546410f73260d57eb83bb6c1dd75ef72d9c5b1dc2\
         8d77404305294c2410c89f77aa33c7c919f8b8df45a24573c31b54e4825dea\
         9d0b47123e25fd44aea56f18e41ec821fcb13ec5eb1c89d0105a4b1caf7404\
         be94ccb05b4196d7bf9a15cc7874f1cb7e52cac646288c883f25d384adaa44\
         e6ccbd089dc4ba4c82d98a64d4607c7c6c451c43ca77777c62948b7d1be70\
         61a1dcdf630fb0739d67c9069bea864ac5f439a6fa01c55d55b847c7ccb041\
         dd9203df280f31",
    )
    .unwrap();
    let ks = Isaac64::new(8318676762518462546).keystream(256);
    assert_eq!(ks, expected);
}

#[test]
fn normalize_appends_token() {
    let u = normalize_cdn_url(
        "http://szmmsns.qpic.cn/mmsns/abc/150",
        "r3MUdKBTQtokBVGyMoJg7qz1",
    );
    assert!(u.starts_with("https://szmmsns.qpic.cn/mmsns/abc/150?token="));
    assert!(u.ends_with("&idx=1"));
    // 已带 token 不再重复拼接
    let u2 = normalize_cdn_url("https://x/0?token=abc", "xyz");
    assert_eq!(u2, "https://x/0?token=abc");
}

#[test]
fn sniff_common_formats() {
    assert_eq!(sniff_image(b"\xFF\xD8\xFF\xE0").map(|x| x.0), Some("jpg"));
    assert_eq!(
        sniff_image(b"\x89PNG\r\n\x1a\n\x00\x00").map(|x| x.0),
        Some("png")
    );
    assert_eq!(sniff_image(b"GIF89a").map(|x| x.0), Some("gif"));
    assert_eq!(
        sniff_image(b"RIFF\x00\x00\x00\x00WEBP").map(|x| x.0),
        Some("webp")
    );
    assert_eq!(sniff_image(b"BM\x00\x00").map(|x| x.0), Some("bmp"));
    assert_eq!(sniff_image(b"junk data").map(|x| x.0), None);
}

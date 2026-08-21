// ============================================================
// 消息通道 — 单元测试
// ============================================================

use super::utils::qr_svg_data_url;

#[test]
fn qr_svg_generation() {
    let url =
        "https://liteapp.weixin.qq.com/q/7GiQu1?qrcode=c7795674b366f486844e07ccc338d467&bot_type=3";
    let data = qr_svg_data_url(url).unwrap();
    assert!(data.starts_with("data:image/svg+xml;base64,"));
    use base64::Engine;
    let svg = base64::engine::general_purpose::STANDARD
        .decode(data.trim_start_matches("data:image/svg+xml;base64,"))
        .unwrap();
    let svg = String::from_utf8(svg).unwrap();
    assert!(
        svg.contains("<svg"),
        "unexpected svg head: {:?}",
        &svg[..svg.len().min(160)]
    );
    assert!(svg.contains("path"));
}

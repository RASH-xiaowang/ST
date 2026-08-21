// ============================================================
// 消息原图官方通道回退 — 测试
// 自 origin_ilink.rs 拆分：真实图片消息端到端回退测试。
// ============================================================

use super::*;
use md5::{Digest, Md5};

/// 端到端：真实图片消息走 ilink 官方通道回退（需本机微信登录态 + 解密库）
///
/// 2026-08-16 本机实机验证记录（微信 4.1.12.26）：
///   Msg_5a8f5ec9ef550505c625c39c3e6d4c9b:2966 → 874,943 字节，
///   MD5 5eb4eeb125563b8a56548e8cdd63e88c 校验通过，PNG 872×608。
/// 旧消息（local_id=9）官方返回 -5103059（CDN 已清理），校验层正确拒绝。
#[test]
#[ignore = "需要本机微信登录态与真实解密库（本地验证用 --ignored 运行）"]
fn ilink_fallback_real_image() {
    let bytes =
        download_origin_via_ilink("23005727013@chatroom", 105990).expect("ilink 原图回退应成功");
    assert_eq!(bytes.len(), 140_809, "原图大小应与 hdlength 一致");
    let actual = hex::encode(Md5::digest(&bytes));
    assert!(
        actual.eq_ignore_ascii_case("77509e4475cb097b7c85cc88c2f98883"),
        "原图 MD5 应与消息记录一致（实际 {actual}）"
    );
}

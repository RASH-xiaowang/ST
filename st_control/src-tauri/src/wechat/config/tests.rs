// ============================================================
// 微信配置 — 单元测试
// ============================================================

use super::detect::read_ini_content;

#[test]
fn test_read_ini_content_utf8() {
    let dir = std::env::temp_dir().join("wechat_config_test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let ini = dir.join("test.ini");
    std::fs::write(&ini, b"D:\\xwechat_files").unwrap();

    let content = read_ini_content(&ini);
    assert_eq!(content.as_deref(), Some("D:\\xwechat_files"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_read_ini_content_with_nulls() {
    let dir = std::env::temp_dir().join("wechat_config_test2");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let ini = dir.join("config.ini");
    // Windows ini often has null bytes in padding
    let mut bytes = b"D:\\xwechat_files".to_vec();
    bytes.push(0);
    bytes.push(0);
    bytes.push(0);
    std::fs::write(&ini, &bytes).unwrap();

    let content = read_ini_content(&ini);
    assert_eq!(content.as_deref(), Some("D:\\xwechat_files"));

    let _ = std::fs::remove_dir_all(&dir);
}

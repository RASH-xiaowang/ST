//! 微信语音消息解码（对标 WeChatDataAnalysis 的 chat/media/voice）
//!
//! 数据源：`decrypted/message/media_0.db` 的 `VoiceInfo` 表（silk v3 编码的 voice_data）。
//! 流程：消息 local_id → svr_id → VoiceInfo.voice_data → SILK 解码 → WAV（浏览器可播）。

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

mod video;
pub(crate) use video::*;

/// 消息 local_id → server_id 内存缓存（避免每次扫全部消息分库）
static MSG_SVR_CACHE: OnceLock<Mutex<std::collections::HashMap<(String, i64), i64>>> =
    OnceLock::new();

/// 语音解码缓存目录（decoded_image_dir/voices/<svr_id>.wav）
fn voice_cache_dir(decoded_image_dir: &Path) -> std::path::PathBuf {
    let d = decoded_image_dir.join("voices");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// 解码 silk → WAV 并磁盘缓存（缓存键 svr_id，避免重复查库与 CPU 解码）
fn decode_silk_cached(cache_dir: &Path, svr_id: i64, silk: &[u8]) -> Option<Vec<u8>> {
    if silk.is_empty() {
        return None;
    }
    let cached = cache_dir.join(format!("{}.wav", svr_id));
    if cached.is_file() {
        if let Ok(b) = std::fs::read(&cached) {
            if b.starts_with(b"RIFF") && !b.is_empty() {
                return Some(b);
            }
        }
    }
    // 微信 voice_data 前有 \x02 前缀，裁剪到标准 SILK_V3 头
    let silk = if let Some(pos) = silk.windows(9).position(|w| w == b"#!SILK_V3") {
        &silk[pos..]
    } else {
        silk
    };
    let tmp_file =
        std::env::temp_dir().join(format!("wx_voice_{}_{}.silk", std::process::id(), svr_id));
    std::fs::write(&tmp_file, silk).ok()?;
    let wav = silk_decoder_rs::silk_to_wav(24000, &tmp_file.to_string_lossy()).ok();
    let _ = std::fs::remove_file(&tmp_file);
    if let Some(w) = wav.as_ref() {
        if w.starts_with(b"RIFF") {
            let _ = std::fs::write(&cached, w);
        }
    }
    wav
}

fn media_db_path(decrypted_dir: &Path) -> Option<std::path::PathBuf> {
    let p = decrypted_dir.join("message").join("media_0.db");
    p.is_file().then_some(p)
}

/// 从消息表按 local_id 取 server_id
fn message_server_id(decrypted_dir: &Path, username: &str, local_id: i64) -> Option<i64> {
    let cache_key = (username.to_string(), local_id);
    let cache = MSG_SVR_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(sid) = guard.get(&cache_key) {
            return Some(*sid);
        }
    }
    let table = crate::wechat::modules::common::msg_table_name(username);
    let msg_dir = decrypted_dir.join("message");
    let Ok(entries) = std::fs::read_dir(&msg_dir) else {
        return None;
    };
    let mut dbs: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()) == Some("db")
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| {
                        n.starts_with("message_")
                            && !n.contains("fts")
                            && !n.contains("resource")
                            && !n.contains("media")
                    })
                    .unwrap_or(false)
        })
        .collect();
    dbs.sort();
    for db in dbs {
        let Ok(conn) = Connection::open_with_flags(
            &db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) else {
            continue;
        };
        let sql = format!(
            "SELECT server_id FROM \"{}\" WHERE local_id = ?1 LIMIT 1",
            table
        );
        let sid: Option<i64> = conn.prepare(&sql).ok().and_then(|mut stmt| {
            stmt.query_row(rusqlite::params![local_id], |r| r.get(0))
                .optional()
                .ok()
                .flatten()
        });
        drop(conn);
        if let Some(sid_val) = sid {
            if let Ok(mut guard) = cache.lock() {
                guard.insert(cache_key, sid_val);
            }
            return sid;
        }
    }
    None
}

/// 按 svr_id 从 VoiceInfo 取 voice_data（silk 字节）
pub fn voice_data_by_svr(decrypted_dir: &Path, svr_id: i64) -> Option<Vec<u8>> {
    let p = media_db_path(decrypted_dir)?;
    let conn = Connection::open_with_flags(
        &p,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let data: Option<Vec<u8>> = conn
        .prepare(
            "SELECT voice_data FROM VoiceInfo WHERE svr_id = ?1 ORDER BY create_time DESC LIMIT 1",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_row(rusqlite::params![svr_id], |r| r.get::<_, Vec<u8>>(0))
                .optional()
                .ok()
                .flatten()
        });
    drop(conn);
    data
}

/// 按 (chat_name_id, local_id) 直查 VoiceInfo（本地权威映射，免消息表扫描）
fn voice_data_by_chat_local(
    decrypted_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<(Vec<u8>, i64)> {
    let p = media_db_path(decrypted_dir)?;
    let conn = Connection::open_with_flags(
        &p,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let chat_id: Option<i64> = conn
        .prepare("SELECT rowid FROM Name2Id WHERE user_name = ?1")
        .ok()
        .and_then(|mut stmt| {
            stmt.query_row(rusqlite::params![username], |r| r.get::<_, i64>(0))
                .optional()
                .ok()
                .flatten()
        });
    let mut out = None;
    if let Some(cid) = chat_id {
        out = conn
            .prepare(
                "SELECT svr_id, voice_data FROM VoiceInfo WHERE chat_name_id = ?1 AND local_id = ?2 LIMIT 1",
            )
            .ok()
            .and_then(|mut stmt| {
                stmt.query_row(rusqlite::params![cid, local_id], |r| {
                    Ok((r.get::<_, Vec<u8>>(1)?, r.get::<_, i64>(0)?))
                })
                .optional()
                .ok()
                .flatten()
            });
    }
    drop(conn);
    out
}

/// 消息语音数据：(voice_data, svr_id)。优先 (chat_name_id, local_id) 直查，
/// 失败回退 svr_id 路径（老数据 / Name2Id 缺失时）。
pub fn message_voice_data(
    decrypted_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<(Vec<u8>, i64)> {
    if let Some((data, svr)) = voice_data_by_chat_local(decrypted_dir, username, local_id) {
        return Some((data, svr));
    }
    let svr_id = message_server_id(decrypted_dir, username, local_id)?;
    let data = voice_data_by_svr(decrypted_dir, svr_id)?;
    Some((data, svr_id))
}

/// 收藏语音：按 server_id 解码为 WAV
pub fn get_favorite_voice(decrypted_dir: &Path, server_id: i64) -> Option<Vec<u8>> {
    let silk = voice_data_by_svr(decrypted_dir, server_id)?;
    let cache_dir = crate::wechat::config::WeChatConfig::load()
        .ok()
        .map(|c| voice_cache_dir(&c.decoded_image_dir))?;
    decode_silk_cached(&cache_dir, server_id, &silk)
}

/// 取消息语音并解码为 WAV（浏览器可播）
pub fn get_message_voice(decrypted_dir: &Path, username: &str, local_id: i64) -> Option<Vec<u8>> {
    let (silk, svr_id) = message_voice_data(decrypted_dir, username, local_id)?;
    let cache_dir = crate::wechat::config::WeChatConfig::load()
        .ok()
        .map(|c| voice_cache_dir(&c.decoded_image_dir))?;
    decode_silk_cached(&cache_dir, svr_id, &silk)
}

/// 取消息语音 WAV + svr_id（转写用，复用解码磁盘缓存）
pub fn message_voice_wav_and_svr(
    decrypted_dir: &Path,
    username: &str,
    local_id: i64,
) -> Option<(Vec<u8>, i64)> {
    let (silk, svr_id) = message_voice_data(decrypted_dir, username, local_id)?;
    let cache_dir = crate::wechat::config::WeChatConfig::load()
        .ok()
        .map(|c| voice_cache_dir(&c.decoded_image_dir))?;
    let wav = decode_silk_cached(&cache_dir, svr_id, &silk)?;
    Some((wav, svr_id))
}

/// 语音转写结果磁盘缓存（decoded_image_dir/voices/<svr_id>.txt）
pub fn transcript_cache_path(decoded_image_dir: &Path, svr_id: i64) -> std::path::PathBuf {
    voice_cache_dir(decoded_image_dir).join(format!("{}.txt", svr_id))
}

pub fn cached_transcript(decoded_image_dir: &Path, svr_id: i64) -> Option<String> {
    let p = transcript_cache_path(decoded_image_dir, svr_id);
    std::fs::read_to_string(&p)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn save_transcript(decoded_image_dir: &Path, svr_id: i64, text: &str) {
    let p = transcript_cache_path(decoded_image_dir, svr_id);
    let _ = std::fs::write(&p, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_voice_decode() {
        let Some(cfg) = crate::wechat::config::WeChatConfig::load().ok() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let Some(media) = media_db_path(&cfg.decrypted_dir) else {
            eprintln!("未找到 media_0.db，跳过");
            return;
        };
        let _ = &media;
        // 遍历所有账号消息表，找一条语音消息（local_type=34）
        let msg_dir = cfg.decrypted_dir.join("message");
        let mut found: Option<(String, i64)> = None;
        let usernames = crate::wechat::annual::load_session_usernames(&cfg.decrypted_dir);
        if let Ok(entries) = std::fs::read_dir(&msg_dir) {
            let mut dbs: Vec<std::path::PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().and_then(|e| e.to_str()) == Some("db")
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| {
                                n.starts_with("message_")
                                    && !n.contains("fts")
                                    && !n.contains("resource")
                                    && !n.contains("media")
                            })
                            .unwrap_or(false)
                })
                .collect();
            dbs.sort();
            'outer: for username in &usernames {
                let table = crate::wechat::modules::common::msg_table_name(username);
                for db in &dbs {
                    let Ok(conn) = Connection::open_with_flags(
                        db,
                        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                    ) else {
                        continue;
                    };
                    let sql = format!(
                        "SELECT local_id FROM \"{}\" WHERE local_type = 34 ORDER BY create_time DESC LIMIT 1",
                        table
                    );
                    let lid: Option<i64> = conn.prepare(&sql).ok().and_then(|mut stmt| {
                        stmt.query_row([], |r| r.get::<_, i64>(0))
                            .optional()
                            .ok()
                            .flatten()
                    });
                    drop(conn);
                    if let Some(lid) = lid {
                        found = Some((username.clone(), lid));
                        break 'outer;
                    }
                }
            }
        }
        let Some((username, local_id)) = found else {
            eprintln!("消息表无语音消息，跳过");
            return;
        };
        let svr_id = message_server_id(&cfg.decrypted_dir, &username, local_id).unwrap_or(0);
        println!(
            "语音 username={} local_id={} svr_id={}",
            username, local_id, svr_id
        );
        let wav = get_message_voice(&cfg.decrypted_dir, &username, local_id);
        match wav {
            Some(w) => {
                println!("WAV 字节: {}（RIFF={}）", w.len(), w.starts_with(b"RIFF"));
                assert!(w.starts_with(b"RIFF"), "WAV 头缺失");
            }
            None => panic!("语音解码失败"),
        }
    }

    /// 群聊语音：遍历多个群聊，验证解码成功率与 (chat_name_id, local_id) 直查路径
    #[test]
    #[cfg(target_os = "windows")]
    fn smoke_voice_group_chats() {
        let Ok(cfg) = crate::wechat::config::WeChatConfig::load() else {
            eprintln!("未找到配置，跳过");
            return;
        };
        let usernames = crate::wechat::annual::load_session_usernames(&cfg.decrypted_dir);
        let msg_dir = cfg.decrypted_dir.join("message");
        let mut chats_ok = 0usize;
        let mut chats_total = 0usize;
        let mut decoded = 0usize;
        let mut scanned = 0usize;
        if let Ok(entries) = std::fs::read_dir(&msg_dir) {
            let mut dbs: Vec<std::path::PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().and_then(|e| e.to_str()) == Some("db")
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| {
                                (n.starts_with("message_") || n.starts_with("biz_message_"))
                                    && !n.contains("fts")
                                    && !n.contains("resource")
                                    && !n.contains("media")
                            })
                            .unwrap_or(false)
                })
                .collect();
            dbs.sort();
            for username in &usernames {
                // 只测群聊
                if !(username.ends_with("@chatroom") || username.contains("@im.chatroom")) {
                    continue;
                }
                let table = crate::wechat::modules::common::msg_table_name(username);
                let mut chat_found = false;
                for db in &dbs {
                    let Ok(conn) = Connection::open_with_flags(
                        db,
                        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                    ) else {
                        continue;
                    };
                    let sql = format!(
                        "SELECT local_id FROM \"{}\" WHERE local_type = 34 ORDER BY create_time DESC LIMIT 20",
                        table
                    );
                    let lids: Vec<i64> = conn
                        .prepare(&sql)
                        .ok()
                        .and_then(|mut stmt| {
                            stmt.query_map([], |r| r.get::<_, i64>(0))
                                .ok()
                                .map(|rows| rows.flatten().collect())
                        })
                        .unwrap_or_default();
                    drop(conn);
                    if lids.is_empty() {
                        continue;
                    }
                    chats_total += 1;
                    let mut ok_in_chat = 0usize;
                    for lid in lids {
                        scanned += 1;
                        // 验证 (chat_name_id, local_id) 直查
                        let direct = message_voice_data(&cfg.decrypted_dir, username, lid);
                        if direct.is_none() {
                            continue;
                        }
                        if let Some(wav) = get_message_voice(&cfg.decrypted_dir, username, lid) {
                            if wav.starts_with(b"RIFF") {
                                decoded += 1;
                                ok_in_chat += 1;
                            }
                        }
                    }
                    if ok_in_chat > 0 {
                        chats_ok += 1;
                        chat_found = true;
                    }
                    if chat_found {
                        break;
                    }
                }
            }
        }
        println!(
            "群聊语音：{} 个群有语音记录，{} 个群解码成功；扫描 {} 条、解码 {} 条",
            chats_total, chats_ok, scanned, decoded
        );
        if chats_ok == 0 || decoded == 0 {
            eprintln!("无群聊语音数据（或解码为空），跳过");
            return;
        }
    }
}

// ============================================================
// Harness — 附件（DSH attachment 迁移）
//
// 附件 = 会话级文件记录：文件复制进工作区 attachments/ 目录，
// AttachmentAdded 会话事件落日志（列表/回放同源）；文本类附件
// 内容预览注入请求上下文（模型可见 ⟺ 落日志）。
// ============================================================

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AttachmentMeta {
    pub id: String,
    pub name: String,
    /// 工作区内副本路径
    pub path: String,
    /// text / image / other
    pub kind: String,
    /// 图片内容寻址哈希（sha256；DSH attachment 图片 seam，非图片为空）
    #[serde(default)]
    pub sha256: String,
    pub size: u64,
    pub created_at: String,
}

fn attachments_dir(session_id: &str) -> std::path::PathBuf {
    crate::llm::agent::workspace_root()
        .join("attachments")
        .join(session_id)
}

/// 图片内容寻址对象目录（DSH attachment-local：objects/<sha256-prefix>/<sha256>）
fn image_objects_dir() -> std::path::PathBuf {
    crate::common::st_data_dir()
        .join("harness")
        .join("attachments")
        .join("objects")
}

/// 附加一个本地文件：复制进工作区并返回元信息（由会话层落日志）。
/// 图片附件额外落内容寻址对象（sha256），可稳定解析为模型图像输入。
pub fn attach_file(session_id: &str, source_path: &str) -> Result<AttachmentMeta, String> {
    let src = std::path::Path::new(source_path);
    if !src.is_file() {
        return Err("源文件不存在".to_string());
    }
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "attachment".to_string());
    let dir = attachments_dir(session_id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建附件目录失败: {}", e))?;
    let dest = dir.join(&name);
    std::fs::copy(src, &dest).map_err(|e| format!("复制附件失败: {}", e))?;
    let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    let ext = std::path::Path::new(&name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let kind = match ext.as_str() {
        "txt" | "md" | "json" | "log" | "csv" | "rs" | "ts" | "js" | "svelte" | "py" | "toml"
        | "yaml" | "yml" | "xml" | "html" | "css" => "text",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" => "image",
        _ => "other",
    };
    // 图片内容寻址（DSH attachment 图片 seam）：sha256 对象副本
    let mut sha256 = String::new();
    if kind == "image" {
        if let Ok(bytes) = std::fs::read(&dest) {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(&bytes);
            sha256 = format!("{hash:x}");
            let objects = image_objects_dir();
            let obj_dir = objects.join(&sha256[..2.min(sha256.len())]);
            std::fs::create_dir_all(&obj_dir).ok();
            let obj = obj_dir.join(&sha256);
            std::fs::write(&obj, &bytes).ok();
        }
    }
    Ok(AttachmentMeta {
        id: format!("att-{}", uuid::Uuid::new_v4().simple()),
        name,
        path: dest.display().to_string(),
        kind: kind.to_string(),
        sha256,
        size,
        created_at: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

/// 从日志投影附件列表
pub fn attachments_from_events(
    events: &[(i64, crate::harness::session::HarnessEvent)],
) -> Vec<AttachmentMeta> {
    let mut out = Vec::new();
    for (_seq, ev) in events {
        if let crate::harness::session::HarnessEvent::AttachmentAdded { meta } = ev {
            out.push(meta.clone());
        }
    }
    out
}

/// 附件上下文注入：文本附件内容预览（≤500 字符/个，≤3 个）；
/// 图片附件注入引用提示（模型可用 read_image 读取副本路径解析为图像输入）
pub fn context_block(attachments: &[AttachmentMeta]) -> String {
    let mut parts: Vec<String> = attachments
        .iter()
        .filter(|a| a.kind == "text")
        .take(3)
        .filter_map(|a| {
            std::fs::read_to_string(&a.path).ok().map(|content| {
                let truncated = if content.chars().count() > 500 {
                    format!("{}…", content.chars().take(500).collect::<String>())
                } else {
                    content
                };
                format!("附件「{}」内容：\n{}", a.name, truncated)
            })
        })
        .collect();
    let image_note = attachments
        .iter()
        .filter(|a| a.kind == "image")
        .take(3)
        .map(|a| {
            format!(
                "图片附件「{}」路径 {}（sha256:{}），可经 read_image 工具读取为视觉输入",
                a.name, a.path, a.sha256
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !image_note.is_empty() {
        parts.push(image_note);
    }
    parts.join("\n\n")
}

// ─── IPC ───

#[tauri::command]
pub async fn harness_attach_file(
    session_id: String,
    source_path: String,
) -> Result<AttachmentMeta, String> {
    let meta = attach_file(&session_id, &source_path)?;
    let store =
        crate::harness::registry::get::<crate::harness::session::SessionStore>("harness.sessions")
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    store
        .append(
            &session_id,
            &crate::harness::session::HarnessEvent::AttachmentAdded { meta: meta.clone() },
        )
        .map_err(|e| format!("附件事件落日志失败: {}", e))?;
    Ok(meta)
}

#[tauri::command]
pub async fn harness_list_attachments(session_id: String) -> Result<Vec<AttachmentMeta>, String> {
    let store =
        crate::harness::registry::get::<crate::harness::session::SessionStore>("harness.sessions")
            .ok_or_else(|| "Harness 运行时未初始化".to_string())?;
    let events = store.events(&session_id, 0)?;
    Ok(attachments_from_events(&events))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_and_kind_detection() {
        let dir = crate::llm::agent::workspace_root();
        std::fs::create_dir_all(&dir).ok();
        let src = dir.join("att_src_test.txt");
        std::fs::write(&src, "附件内容").unwrap();
        let meta = attach_file("att-test-session", src.to_str().unwrap()).unwrap();
        assert_eq!(meta.kind, "text");
        assert!(meta.path.contains("attachments"));
        assert!(std::fs::read_to_string(&meta.path)
            .unwrap()
            .contains("附件内容"));
        // 清理
        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_dir_all(attachments_dir("att-test-session"));
    }

    #[test]
    fn image_attachment_sha256_content_addressed() {
        // B10 图片 seam：图片附件生成 sha256 内容寻址对象副本（幂等）
        let dir = crate::llm::agent::workspace_root();
        std::fs::create_dir_all(&dir).ok();
        let src = dir.join("att_img_test.png");
        std::fs::write(&src, [0x89u8, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]).unwrap();
        let meta = attach_file("att-img-session", src.to_str().unwrap()).unwrap();
        assert_eq!(meta.kind, "image");
        // sha256 非空且为 64 位十六进制
        assert_eq!(meta.sha256.len(), 64, "sha256 应 64 位: {}", meta.sha256);
        // 内容寻址对象已落盘（image_objects/<前2位>/<完整hash>）
        let obj_dir = image_objects_dir().join(&meta.sha256[..2]);
        let obj = obj_dir.join(&meta.sha256);
        assert!(obj.exists(), "内容寻址对象应存在: {}", obj.display());
        // 相同内容 → 相同 sha256（幂等寻址）
        let meta2 = attach_file("att-img-session2", src.to_str().unwrap()).unwrap();
        assert_eq!(meta.sha256, meta2.sha256, "同内容应同 hash");
        // 清理
        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_dir_all(attachments_dir("att-img-session"));
        let _ = std::fs::remove_dir_all(attachments_dir("att-img-session2"));
    }

    #[test]
    fn context_block_previews_text() {
        let meta = AttachmentMeta {
            id: "a".into(),
            name: "n.txt".into(),
            path: {
                let p = std::env::temp_dir().join("hns-att-preview.txt");
                std::fs::write(&p, "预览内容").unwrap();
                p.display().to_string()
            },
            kind: "text".into(),
            sha256: String::new(),
            size: 0,
            created_at: String::new(),
        };
        let ctx = context_block(&[meta]);
        assert!(ctx.contains("n.txt"));
        assert!(ctx.contains("预览内容"));
        let _ = std::fs::remove_file(std::env::temp_dir().join("hns-att-preview.txt"));
    }

    #[test]
    fn context_block_image_note_and_sha256() {
        // 图片附件：注入 read_image 引用提示 + sha256 内容寻址（B10）
        let meta = AttachmentMeta {
            id: "img".into(),
            name: "photo.png".into(),
            path: "C:/tmp/photo.png".into(),
            kind: "image".into(),
            sha256: "abc123".into(),
            size: 0,
            created_at: String::new(),
        };
        let ctx = context_block(&[meta.clone()]);
        assert!(ctx.contains("photo.png"), "应含图片附件名: {ctx}");
        assert!(
            ctx.contains("read_image"),
            "应提示可经 read_image 读取: {ctx}"
        );
        assert!(ctx.contains("abc123"), "应含 sha256 内容寻址: {ctx}");
        // 图片不注入内容预览（无文本读取）
        assert!(!ctx.contains("内容："), "图片附件不应有文本预览: {ctx}");
        // 混合：文本 + 图片都在
        let text_meta = AttachmentMeta {
            id: "t".into(),
            name: "note.txt".into(),
            path: {
                let p = std::env::temp_dir().join("hns-att-mixed.txt");
                std::fs::write(&p, "混合内容").unwrap();
                p.display().to_string()
            },
            kind: "text".into(),
            sha256: String::new(),
            size: 0,
            created_at: String::new(),
        };
        let ctx = context_block(&[text_meta.clone(), meta.clone()]);
        assert!(ctx.contains("混合内容") && ctx.contains("read_image"));
        let _ = std::fs::remove_file(std::env::temp_dir().join("hns-att-mixed.txt"));
    }

    #[test]
    fn attachments_from_events_filters_and_orders() {
        // 从事件日志投影附件：AttachmentAdded 提取、按事件序保持
        let ev = crate::harness::session::HarnessEvent::AttachmentAdded {
            meta: AttachmentMeta {
                id: "a1".into(),
                name: "one.txt".into(),
                path: "p1".into(),
                kind: "text".into(),
                sha256: "s1".into(),
                size: 1,
                created_at: "t".into(),
            },
        };
        let other = crate::harness::session::HarnessEvent::UserMessage {
            id: "u".into(),
            content: "x".into(),
        };
        // 事件流：user / attachment / user / attachment
        let events: Vec<(i64, crate::harness::session::HarnessEvent)> = vec![
            (1, other.clone()),
            (2, ev.clone()),
            (3, other),
            (4, ev.clone()),
        ];
        let list = attachments_from_events(&events);
        assert_eq!(list.len(), 2, "应提取 2 个附件: {list:?}");
        assert_eq!(list[0].id, "a1");
    }
}

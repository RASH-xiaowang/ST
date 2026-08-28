// ============================================================
// Harness — 工具输出溢写（DSH spill 迁移）
//
// spill-policy 语义：工具结果超过内联上限时，完整文本落盘到
// data/harness/spills/<session>/，模型可见结果替换为有界 head/tail
// 预览 + 定位符 + 检索提示；模型可经 spill_read 工具取回完整值。
// 溢写文件与定位符持久化，跨回合可检索（DSH SpillStore 等价）。
// ============================================================

/// 内联上限（字符）：超过即溢写
pub const MAX_INLINE_CHARS: usize = 2000;
/// 预览头/尾各保留字符数
const PREVIEW_CHARS: usize = 600;

/// 溢写引用（模型可见）
#[derive(serde::Serialize, Clone, Debug)]
pub struct SpillRef {
    pub locator: String,
    pub retrieval_hint: String,
}

fn spills_root() -> std::path::PathBuf {
    crate::common::st_data_dir().join("harness").join("spills")
}

fn session_dir(session_id: &str) -> std::path::PathBuf {
    spills_root().join(sanitize(session_id))
}

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 溢写存储（进程内无需状态：文件即真相；定位符 = 文件绝对路径）
pub struct SpillStore;

impl SpillStore {
    /// 落盘完整文本，返回定位符与检索提示
    pub fn save(session_id: &str, text: &str) -> Result<SpillRef, String> {
        let dir = session_dir(session_id);
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建溢写目录失败: {}", e))?;
        let file = dir.join(format!("spill-{}.txt", uuid::Uuid::new_v4().simple()));
        std::fs::write(&file, text.as_bytes()).map_err(|e| format!("写入溢写失败: {}", e))?;
        Ok(SpillRef {
            locator: file.display().to_string(),
            retrieval_hint: "完整输出已落盘：用 spill_read 工具并传入 locator 取回".to_string(),
        })
    }

    /// 共享溢写（无会话归属）：供无会话上下文的工具（glob/grep 溢出完整
    /// 列表）使用；文件仍位于 spills_root 下，spill_read 可跨会话取回。
    pub fn save_shared(text: &str) -> Result<SpillRef, String> {
        let dir = spills_root().join("shared");
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建共享溢写目录失败: {}", e))?;
        let file = dir.join(format!("spill-{}.txt", uuid::Uuid::new_v4().simple()));
        std::fs::write(&file, text.as_bytes()).map_err(|e| format!("写入共享溢写失败: {}", e))?;
        Ok(SpillRef {
            locator: file.display().to_string(),
            retrieval_hint: "完整输出已落盘：用 spill_read 工具并传入 locator 取回".to_string(),
        })
    }

    /// 按定位符读取（校验必须位于溢写目录内）
    pub fn read(locator: &str) -> Result<String, String> {
        let p = std::path::PathBuf::from(locator);
        let root = spills_root();
        let canon_root = root
            .canonicalize()
            .map_err(|_| "溢写目录不可用".to_string())?;
        let canon = p
            .canonicalize()
            .map_err(|_| format!("溢写文件不存在: {locator}"))?;
        if !canon.starts_with(&canon_root) {
            return Err("定位符不在溢写目录内".to_string());
        }
        let text = std::fs::read_to_string(&canon).map_err(|e| format!("读取溢写失败: {}", e))?;
        // 取回上限 32KB，超出截断并注明（字符边界安全）
        if text.len() > 32 * 1024 {
            let end = text.floor_char_boundary(32 * 1024);
            Ok(format!(
                "{}…（内容过长已截断，完整内容见 {locator}）",
                &text[..end]
            ))
        } else {
            Ok(text)
        }
    }
}

/// 溢写策略入口：未超限原样返回；超限落盘并返回预览 + 定位符
pub fn maybe_spill(session_id: &str, text: &str) -> String {
    let len = text.chars().count();
    if len <= MAX_INLINE_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(PREVIEW_CHARS).collect();
    let tail: String = text
        .chars()
        .rev()
        .take(PREVIEW_CHARS)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    match SpillStore::save(session_id, text) {
        Ok(r) => format!(
            "[工具输出已溢写（{} 字符）\n定位符 locator: {}\n{}]\n{}\n…（中间省略）…\n{}",
            len, r.locator, r.retrieval_hint, head, tail
        ),
        Err(e) => format!("[溢写失败: {e}]\n{}", text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spill_saves_and_reads_roundtrip() {
        let sid = format!("spill-sess-{}", uuid::Uuid::new_v4().simple());
        let long = "x".repeat(5000);
        let out = maybe_spill(&sid, &long);
        assert!(out.contains("已溢写"));
        assert!(out.contains("locator"));
        assert!(!out.contains(&"x".repeat(5000)));
        // 从预览里提取 locator 并取回完整值
        let loc = out
            .lines()
            .find_map(|l| l.strip_prefix("定位符 locator: "))
            .unwrap()
            .to_string();
        let back = SpillStore::read(&loc).unwrap();
        assert_eq!(back, long);
        // 清理
        let _ = std::fs::remove_dir_all(session_dir(&sid));
    }

    #[test]
    fn spill_short_text_inline() {
        let sid = "short";
        let short = "hello".to_string();
        assert_eq!(maybe_spill(&sid, &short), "hello");
    }

    #[test]
    fn spill_read_rejects_outside_paths() {
        assert!(SpillStore::read("C:/Windows/win.ini").is_err());
    }

    #[test]
    fn spill_read_result_is_not_re_spilled() {
        // 递归防护：spill_read 取回的内容（> 内联上限）不再次溢写，
        // 直接以完整文本返回（agent 层 spill_result 语义）
        let sid = format!("spill-rec-{}", uuid::Uuid::new_v4().simple());
        let long = "y".repeat(5000);
        let out = crate::harness::agent::spill_result(&sid, "spill_read", &long);
        assert_eq!(out.len(), long.len());
        assert!(!out.contains("已溢写"));
        // 其他工具超限仍溢写
        let out2 = crate::harness::agent::spill_result(&sid, "read_file", &long);
        assert!(out2.contains("已溢写"));
        let _ = std::fs::remove_dir_all(session_dir(&sid));
    }
}

// ============================================================
// 微信数据各界面自动化审计
//
// 直接调用与 UI 完全同路径的模块函数（配置加载 → 解密库查询 → JSON），
// 逐个界面检查数据链路是否正常。只读，不修改任何业务数据。
//
// 运行：cargo test --manifest-path src-tauri/Cargo.toml --test wechat_ui_smoke -- --nocapture
// ============================================================

use std::path::{Path, PathBuf};

use st_control_lib::wechat::config::WeChatConfig;
use st_control_lib::wechat::modules::{
    avatar, contacts, emoticons, favorites, files, messages, moments, official, sessions, settings,
};
use st_control_lib::wechat::{annual, backup, chat_search_index, daily_summary, privacy, voice};

struct Report {
    rows: Vec<(String, String, String)>,
    failures: usize,
    warns: usize,
}

impl Report {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            failures: 0,
            warns: 0,
        }
    }

    fn pass(&mut self, name: &str, detail: impl Into<String>) {
        self.rows
            .push((name.to_owned(), "PASS".to_owned(), detail.into()));
    }

    fn warn(&mut self, name: &str, detail: impl Into<String>) {
        self.warns += 1;
        self.rows
            .push((name.to_owned(), "WARN".to_owned(), detail.into()));
    }

    fn fail(&mut self, name: &str, detail: impl Into<String>) {
        self.failures += 1;
        self.rows
            .push((name.to_owned(), "FAIL".to_owned(), detail.into()));
    }

    fn print(&self) {
        println!("\n===== 微信数据界面自动化审计 =====");
        println!("{:<20} {:<5} {}", "界面", "状态", "说明");
        println!("{}", "-".repeat(110));
        for (name, status, detail) in &self.rows {
            let d: String = detail.chars().take(100).collect();
            println!("{:<20} {:<5} {}", name, status, d);
        }
        println!("{}", "-".repeat(110));
        println!(
            "合计: {} 项 · PASS {} · WARN {} · FAIL {}",
            self.rows.len(),
            self.rows.len() - self.warns - self.failures,
            self.warns,
            self.failures
        );
    }
}

fn val_len(v: &serde_json::Value, keys: &[&str]) -> Option<usize> {
    for k in keys {
        if let Some(n) = v.get(*k).and_then(|x| x.as_u64()) {
            return Some(n as usize);
        }
        if let Some(arr) = v.get(*k).and_then(|x| x.as_array()) {
            return Some(arr.len());
        }
    }
    None
}

fn first_friend_username(cfg: &WeChatConfig) -> Option<String> {
    let page = contacts::get_contacts_page(&cfg.decrypted_dir, "friend", 0, 200, None).ok()?;
    page.contacts
        .into_iter()
        .find(|c| !c.username.is_empty())
        .map(|c| c.username)
}

fn default_st_result_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("st_result")
}

#[test]
fn wechat_ui_smoke_all_interfaces() {
    let mut report = Report::new();

    let cfg = match WeChatConfig::load() {
        Ok(c) => c,
        Err(e) => {
            println!("SKIP: 微信配置不可用，跳过审计（{e}）");
            return;
        }
    };
    let dir = &cfg.decrypted_dir;
    let base = &cfg.wechat_base_dir;
    let self_username = cfg.wxid().unwrap_or_default();

    report.pass(
        "基础配置",
        format!("解密目录={} wxid={}", dir.display(), self_username),
    );
    if !dir.is_dir() {
        report.fail("基础配置", "解密目录不存在，请先完成微信数据库解密");
        report.print();
        assert_eq!(report.failures, 0);
        return;
    }

    // ── 会话 / 聊天 ──
    let sessions_res = sessions::get_session_list(dir);
    let session_list = match &sessions_res {
        Ok(list) => list,
        Err(e) => {
            report.fail("聊天-会话列表", e.clone());
            &vec![]
        }
    };
    if sessions_res.is_ok() {
        report.pass("聊天-会话列表", format!("共 {} 个会话", session_list.len()));
    }

    // ── 客服（按 UI 同规则从会话识别）──
    let kefu_count = session_list
        .iter()
        .filter(|s| {
            s.username.contains("@kefu.openim")
                || s.username.contains("@weclaw")
                || s.username.contains("brand")
        })
        .count();
    if kefu_count > 0 {
        report.pass("客服", format!("识别出 {kefu_count} 个客服会话"));
    } else {
        report.warn("客服", "未识别到客服会话（当前账号可能没有客服会话）");
    }

    let sample_talkers: Vec<String> = session_list
        .iter()
        .take(8)
        .map(|s| s.username.clone())
        .collect();
    if sample_talkers.is_empty() {
        report.warn("聊天-消息流", "没有可用的会话样本");
    } else {
        let mut loaded = 0usize;
        let mut voice_found: Option<(String, i64)> = None;
        for talker in &sample_talkers {
            match messages::get_conversation_messages(dir, talker, &self_username, None, 100) {
                Ok(page) => {
                    loaded += page.messages.len();
                    if voice_found.is_none() {
                        voice_found = page
                            .messages
                            .iter()
                            .find(|m| m.msg_type == 34 && m.local_id > 0)
                            .map(|m| (talker.clone(), m.local_id));
                    }
                }
                Err(e) => report.fail("聊天-消息流", format!("会话 {talker}: {e}")),
            }
        }
        if loaded > 0 {
            report.pass(
                "聊天-消息流",
                format!("{} 个会话共加载 {loaded} 条消息", sample_talkers.len()),
            );
        } else {
            report.warn("聊天-消息流", "样本会话均为空（可能无消息或解密不完整）");
        }

        // ── 语音解码 ──
        if let Some((username, local_id)) = voice_found {
            match voice::get_message_voice(dir, &username, local_id) {
                Some(wav) if !wav.is_empty() => report.pass(
                    "聊天-语音解码",
                    format!(
                        "会话 {username} local_id={local_id} 解码出 {} 字节 WAV",
                        wav.len()
                    ),
                ),
                _ => report.warn("聊天-语音解码", format!("local_id={local_id} 解码失败")),
            }
        } else {
            report.warn("聊天-语音解码", "样本会话中没有语音消息，跳过");
        }
    }

    // ── 头像 ──
    if let Some(username) = first_friend_username(&cfg) {
        let av = avatar::get_user_avatar(dir, base, &username, None, cfg.image_xor_key);
        let kind = av.get("kind").and_then(|k| k.as_str()).unwrap_or("none");
        if kind == "data" || kind == "url" {
            report.pass("聊天-头像", format!("{username} → {kind}"));
        } else {
            report.warn("聊天-头像", format!("{username} 未取到头像（kind={kind}）"));
        }
    } else {
        report.warn("聊天-头像", "没有好友样本");
    }

    // ── AI 问答 / 搜索索引 ──
    let idx = chat_search_index::get_search_index_status();
    let idx_desc = serde_json::to_string(&idx).unwrap_or_default();
    let idx_ready = idx.get("exists").and_then(|v| v.as_bool()).unwrap_or(false)
        && idx.get("rows").and_then(|v| v.as_u64()).unwrap_or(0) > 0;
    if idx_ready {
        report.pass("AI问答-搜索索引", format!("就绪 {idx_desc}"));
    } else {
        report.warn("AI问答-搜索索引", format!("未就绪 {idx_desc}"));
    }
    let llm_db = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("st-control")
        .join("llm_gateway.db");
    if llm_db.exists() {
        report.pass("AI问答-大模型配置", "llm_gateway.db 存在");
    } else {
        report.warn(
            "AI问答-大模型配置",
            "llm_gateway.db 不存在（AI 问答需先配置大模型）",
        );
    }

    // ── 关系图谱 ──
    let graph_path = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("st-control")
        .join("relationship_graph.json");
    match std::fs::read_to_string(&graph_path) {
        Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(v) => {
                let nodes = v
                    .get("nodes")
                    .and_then(|n| n.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let links = v
                    .get("edges")
                    .or_else(|| v.get("links"))
                    .and_then(|l| l.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                let summary = v.get("summary").map(|s| s.to_string()).unwrap_or_default();
                report.pass(
                    "关系图谱-缓存",
                    format!("{nodes} 节点 / {links} 连线（{summary}）"),
                );
            }
            Err(e) => report.fail("关系图谱-缓存", format!("缓存解析失败: {e}")),
        },
        Err(e) => report.warn(
            "关系图谱-缓存",
            format!("无缓存文件（首次进入会重新聚合）: {e}"),
        ),
    }

    // ── 群监控 ──
    let monitor_cache = dir.join("monitor_cache");
    let need_files = [
        "session/session.db",
        "message_message_0.db",
        "contact_contact.db",
    ];
    let missing: Vec<&str> = need_files
        .iter()
        .filter(|f| !monitor_cache.join(f).exists())
        .copied()
        .collect();
    if missing.is_empty() {
        report.pass("群监控-缓存", "monitor_cache 三库齐全");
    } else {
        report.warn("群监控-缓存", format!("缺少: {}", missing.join(", ")));
    }
    if let Ok(conn) = rusqlite::Connection::open_with_flags(
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("st-control")
            .join("control.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) {
        let rules: i64 = conn
            .query_row("SELECT COUNT(*) FROM automation_rules", [], |r| r.get(0))
            .unwrap_or(0);
        let tasks: i64 = conn
            .query_row("SELECT COUNT(*) FROM automation_tasks", [], |r| r.get(0))
            .unwrap_or(0);
        report.pass("群监控-自动化", format!("{rules} 条规则 / {tasks} 个任务"));
    } else {
        report.warn("群监控-自动化", "control.db 打开失败");
    }

    // ── 通讯录 ──
    for (label, cat) in [
        ("通讯录-好友", "friend"),
        ("通讯录-群聊", "group"),
        ("通讯录-公众号", "official"),
        ("通讯录-服务号", "service"),
    ] {
        match contacts::get_contacts_page(dir, cat, 0, 1000, None) {
            Ok(page) => {
                if page.total > 0 {
                    report.pass(label, format!("共 {} 个", page.total));
                } else {
                    report.warn(label, "0 个（数据为空）");
                }
            }
            Err(e) => report.fail(label, e),
        }
    }
    if let Some(u) = first_friend_username(&cfg) {
        match contacts::get_contact_profile(dir, &u) {
            Some(c) => report.pass(
                "通讯录-资料卡",
                format!(
                    "{u} → {}",
                    if c.remark.is_empty() {
                        c.nick_name
                    } else {
                        c.remark
                    }
                ),
            ),
            None => report.warn("通讯录-资料卡", format!("{u} 未找到资料")),
        }
    }

    // ── 朋友圈 ──
    match moments::get_moments_page(dir, &self_username, 0, 10, None) {
        Ok(page) => {
            if page.total > 0 {
                report.pass(
                    "朋友圈",
                    format!("共 {} 条，本页 {} 条", page.total, page.items.len()),
                );
            } else {
                report.warn("朋友圈", "0 条");
            }
        }
        Err(e) => report.fail("朋友圈", e),
    }

    // ── 收藏 ──
    match favorites::get_favorites(dir, 20) {
        Ok(v) => {
            let total = val_len(&v, &["total", "count", "items"]).unwrap_or(0);
            if total > 0 {
                report.pass("收藏", format!("共 {total} 条"));
            } else {
                report.warn("收藏", "0 条");
            }
            if let Some(first_id) = v
                .get("items")
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
                .and_then(|it| it.get("local_id").and_then(|x| x.as_i64()))
            {
                match favorites::get_favorite_detail(dir, first_id) {
                    Ok(_) => report.pass("收藏-详情", format!("local_id={first_id} 详情可读")),
                    Err(e) => report.fail("收藏-详情", format!("local_id={first_id}: {e}")),
                }
            }
        }
        Err(e) => report.fail("收藏", e),
    }

    // ── 表情 ──
    match emoticons::get_emoticons(dir) {
        Ok(o) => {
            let total = o.packages.len() + o.custom.len() + o.store_files.len();
            if total > 0 {
                report.pass(
                    "表情",
                    format!(
                        "共 {total} 个（包 {} / 自定义 {} / 商店 {}）",
                        o.packages.len(),
                        o.custom.len(),
                        o.store_files.len()
                    ),
                );
            } else {
                report.warn("表情", "0 个");
            }
        }
        Err(e) => report.fail("表情", e),
    }
    match emoticons::get_static_emoticons() {
        Ok(cats) => report.pass("表情-内置", format!("{} 个分类", cats.len())),
        Err(e) => report.fail("表情-内置", e),
    }

    // ── 文件 ──
    match files::get_resource_files(dir, base, 20) {
        Ok(o) => {
            let img = o.images_total;
            let vid = o.videos_total;
            let fil = o.files_total;
            if img + vid + fil > 0 {
                report.pass("文件", format!("图片 {img} / 视频 {vid} / 文件 {fil}"));
            } else {
                report.warn("文件", "0 个资源");
            }
        }
        Err(e) => report.fail("文件", e),
    }

    // ── 记录（转账/红包/视频号等）──
    match settings::get_general_settings(dir, 20) {
        Ok(cats) => {
            let lines: Vec<String> = cats
                .iter()
                .map(|c| format!("{}={}", c.label, c.total))
                .collect();
            report.pass("记录", lines.join(" "));
        }
        Err(e) => report.fail("记录", e),
    }

    // ── 公众号 / 服务号（界面实际调 get_official_accounts）──
    match official::get_official_accounts(dir) {
        Ok(list) => {
            if list.is_empty() {
                report.warn("公众号/服务号", "0 个");
            } else {
                let v = serde_json::to_value(&list).unwrap_or_default();
                let service = v
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter(|b| {
                                b.get("official_kind").and_then(|k| k.as_str()) == Some("service")
                            })
                            .count()
                    })
                    .unwrap_or(0);
                report.pass(
                    "公众号/服务号",
                    format!("共 {} 个（其中服务号 {service}）", list.len()),
                );
            }
        }
        Err(e) => report.fail("公众号/服务号", e),
    }

    // ── 商家客服库（bizchat.db，信息项，非公众号界面数据源）──
    match official::get_bizchats(dir) {
        Ok(o) => {
            if o.groups.is_empty() && o.users.is_empty() {
                report.warn(
                    "商家客服库",
                    "bizchat.db 无数据（不影响公众号界面；公众号/服务号走 get_official_accounts）",
                );
            } else {
                report.pass(
                    "商家客服库",
                    format!("{} 组 / {} 用户", o.groups.len(), o.users.len()),
                );
            }
        }
        Err(e) => report.warn("商家客服库", e),
    }

    // ── 年度总结 ──
    let years = annual::available_years(dir);
    if years.is_empty() {
        report.warn("年度总结", "无消息年份数据");
    } else {
        let latest = *years.iter().max().unwrap();
        report.pass("年度总结-年份", format!("可用年份: {:?}", years));
        match annual::annual_summary(dir, latest) {
            Ok(_) => report.pass("年度总结-明细", format!("{latest} 年总结生成成功")),
            Err(e) => report.fail("年度总结-明细", format!("{latest}: {e}")),
        }
    }

    // ── 每日总结 ──
    match daily_summary::list_tasks() {
        Ok(tasks) => report.pass("每日总结", format!("{} 个任务", tasks.len())),
        Err(e) => report.fail("每日总结", e),
    }

    // ── 隐私体检 ──
    match privacy::scan_privacy_risks(dir, 30_000) {
        Ok(v) => {
            let n = val_len(&v, &["total", "count", "items", "findings"]).unwrap_or(0);
            report.pass("隐私体检", format!("扫描完成，命中 {n} 项"));
        }
        Err(e) => report.fail("隐私体检", e),
    }

    // ── 备份管家 ──
    let exports = default_st_result_dir().join("exports");
    match backup::list_encrypted_backups(exports.to_str().unwrap_or("")) {
        Ok(v) => {
            let n = val_len(&v, &["total", "count", "items", "backups"]).unwrap_or(0);
            if n > 0 {
                report.pass("备份管家", format!("共 {n} 个备份"));
            } else {
                report.warn("备份管家", "暂无备份文件（未创建过备份）");
            }
        }
        Err(e) => report.warn("备份管家", e),
    }

    // ── 原图 Hook ──
    let hook_cfg = default_st_result_dir().join("hook_config.json");
    let hook_dll = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("hook")
        .join("win32")
        .join("x64")
        .join("img_helper.dll");
    let enabled = std::fs::read_to_string(&hook_cfg)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("enabled").and_then(|x| x.as_bool()))
        .unwrap_or(false);
    if hook_dll.exists() {
        report.pass(
            "原图Hook",
            format!("img_helper.dll 存在，配置 enabled={enabled}"),
        );
    } else {
        report.fail(
            "原图Hook",
            format!("img_helper.dll 缺失: {}", hook_dll.display()),
        );
    }

    report.print();
    assert_eq!(
        report.failures, 0,
        "存在 {} 项 FAIL，请查看上方报告",
        report.failures
    );
}

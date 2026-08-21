//! 微信实时消息监听 — 主更新检测循环
//! 自 monitor.rs 拆分：mtime 门控、解密刷新、状态对比、
//! 消息提取与去重、水位线更新。

use std::sync::Arc;
use std::time::SystemTime;

use super::util::{format_msg_type, media_type};
use super::{SessionMonitor, WeChatMessage};

impl SessionMonitor {
    /// 主更新检测循环（单次执行）
    ///
    /// 逻辑：
    /// 0. mtime 门控：session.db / WAL 无变化则直接返回（每周期仅 2 次 stat）
    /// 1. 按变化类型解密：主库变→全量+WAL；仅 WAL 变→增量 WAL patch（毫秒级）
    /// 2. 读取当前 SessionTable 状态
    /// 3. 与 prev_state 对比，找出有新消息的会话
    /// 4. 对每个有变化的会话，从 message DB 提取具体消息
    ///    - 以 session 表时间窗口为 cutoff，水位线为下界
    /// 5. 更新 prev_state 与水线供下次对比
    pub async fn check_updates(self: &Arc<Self>) -> Vec<WeChatMessage> {
        // 0. mtime 门控：session.db/WAL 或 message 分库均无变化则直接跳过。
        //    只做少量 stat，避免空转时的全量解密开销。
        let (db_changed, wal_changed) = self.session_file_changed();
        let msg_changed = self.message_dbs_changed();
        if !db_changed && !wal_changed && !msg_changed {
            return vec![];
        }
        log::debug!(
            "[monitor] 检测到变化: db_changed={}, wal_changed={}, msg_changed={}",
            db_changed,
            wal_changed,
            msg_changed
        );

        // 1. 解密（CPU/IO 密集：全量解密数百 MB 库可能耗时数秒）：
        //    - 主库变化（checkpoint 等）→ 全量解密 + WAL patch
        //    - 仅 WAL 变化（常见，新消息写入）→ 仅 WAL 增量 patch（毫秒级）
        //    - 仅消息分库变化 → 无需刷新 session 副本（消息分库由
        //      query_messages_since_watermark 内的 db_cache 按需解密）
        //    解密移出 tokio worker（spawn_blocking）；外层 30s 超时只能放弃等待，
        //    后台解密继续执行，靠 mtime 门控避免下一轮重复工作。
        //    失败时不推进 mtime 快照，下一轮会自动重试，保证不丢消息。
        let refresh_result = if db_changed || wal_changed {
            let this = Arc::clone(self);
            tauri::async_runtime::spawn_blocking(move || {
                if db_changed {
                    this.do_full_refresh()
                } else {
                    this.do_wal_refresh()
                }
            })
            .await
            .unwrap_or_else(|e| {
                Err(std::io::Error::other(format!(
                    "[monitor] 解密任务异常: {}",
                    e
                )))
            })
        } else {
            Ok(0)
        };
        if let Err(e) = refresh_result {
            log::error!("check_updates 解密失败: {}", e);
            // 解密失败时继续读取已有的解密文件
        }

        self.check_updates_inner(60).await
    }

    /// 强制刷新：不经过 mtime 门控，用于 30s 水位线兜底 tick
    ///
    /// `cutoff_secs` 控制最多回溯多少秒前的消息。
    pub async fn check_updates_forced(self: &Arc<Self>) -> Vec<WeChatMessage> {
        log::debug!("[monitor] 执行强制刷新（水位线兜底）");
        // 解密失败时仍尝试读取已有解密文件
        let this = Arc::clone(self);
        let _ = tauri::async_runtime::spawn_blocking(move || this.do_wal_refresh())
            .await
            .unwrap_or_else(|e| {
                Err(std::io::Error::other(format!(
                    "[monitor] 解密任务异常: {}",
                    e
                )))
            });
        self.check_updates_inner(300).await
    }

    async fn check_updates_inner(self: &Arc<Self>, cutoff_secs: i64) -> Vec<WeChatMessage> {
        // 2. 查询当前状态（直接从解密文件读取）
        let this = Arc::clone(self);
        let query_res = tauri::async_runtime::spawn_blocking(move || this.query_state()).await;
        let curr_state = match query_res {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                log::error!("query_state 失败: {}", e);
                // 文件损坏时删除，下次循环自动重新解密
                if e.to_string().contains("malformed") && self.decrypted_session.exists() {
                    let _ = std::fs::remove_file(&self.decrypted_session);
                    log::warn!(
                        "[monitor] 已删除损坏的解密数据库: {}",
                        self.decrypted_session.display()
                    );
                }
                return vec![];
            }
            Err(e) => {
                log::error!("query_state 任务异常: {}", e);
                return vec![];
            }
        };

        // 3. 比较变化
        let prev_state = self.prev_state.read().await;
        let mut new_msgs = Vec::new();
        let mut watermark_updates: Vec<(String, crate::wechat::watermark::SessionWatermark)> =
            Vec::new();
        let contact_names = self.contact_names.read().await;
        let us_per_sec: i64 = 1_000_000;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let cutoff = now - cutoff_secs; // 最多推送 cutoff_secs 秒前的消息

        for (username, curr) in &curr_state {
            let prev = prev_state.get(username);

            // 判断是否有新消息：
            // - 时间戳更新（最常见情况）
            // - 时间戳相同但内容字段变化：同一秒内可能收到多条消息，
            //   此时 session 的摘要/发送者会变化；群聊尤其常见
            // - 首次运行：所有会话都视为有新消息（会走 fallback 推送摘要）
            let is_new = match prev {
                Some(p) => {
                    curr.timestamp > p.timestamp
                        || (curr.timestamp == p.timestamp
                            && (curr.msg_type != p.msg_type
                                || curr.summary != p.summary
                                || curr.sender != p.sender
                                || curr.sender_name != p.sender_name))
                        || curr.unread > p.unread
                }
                None => true,
            };

            if !is_new {
                continue;
            }

            let display = contact_names
                .get(username)
                .cloned()
                .unwrap_or_else(|| username.clone());
            let is_group = username.contains("@chatroom");
            let prev_ts = prev
                .map(|p| p.timestamp)
                .unwrap_or_else(|| curr.timestamp - 5);

            // 限制不早于 60 秒前
            let prev_ts = prev_ts.max(cutoff);

            // 查询 message DB：以 session 表时间窗口为 cutoff，水位线为下界
            let all_rows = self.query_messages_since_watermark(username, prev_ts).await;

            if all_rows.is_empty() {
                // 没有捞到具体消息（message 分库 WAL 尚未 checkpoint、懒加载映射缺失等）。
                // 【关键】此处【不推进水位线】：原实现把水位线直接推进到 curr.timestamp，
                // 导致晚到的真实消息 create_time <= 水位线而被永久过滤丢失。
                // 保留旧水位线后，下次该会话再有变化时会按旧下界重查，
                // 把本次漏掉的消息一并捞出（shown_keys 负责去重，不会重复推送）。
                // prev_state 仍会更新，因此 fallback 摘要每条只推一次，不会刷屏。

                // fallback: 从 SessionTable 摘要构造。
                // 注意：单聊 SessionTable 的 last_msg_sender 经常为空，直接按
                // curr.sender 判断方向，会把「我发的消息」错放到对方一侧且无头像。
                // 因此先按最新 local_id 直查消息库（不套水位线）拿真实发送者；
                // 查不到才退回摘要（不推进水位线，下轮查库成功后会再推正确消息，
                // 前端按 local_id 原位替换，不会出现两个气泡）。
                let latest = self.query_latest_message(username).await;
                let (sender_username, is_send, content_text, local_id, sort_seq, ts) =
                    if let Some((lid, ts2, lt, _mc, _svr, ss, sender_u)) = latest {
                        let real_sender = if sender_u.is_empty() && !is_group {
                            username.clone()
                        } else {
                            sender_u
                        };
                        let is_send =
                            !self.self_username.is_empty() && real_sender == self.self_username;
                        let content = if curr.msg_type == 1 {
                            curr.summary.clone()
                        } else {
                            format!("[{}]", format_msg_type(lt))
                        };
                        (real_sender, is_send, content, Some(lid), Some(ss), ts2)
                    } else {
                        let content_text = if curr.msg_type == 1 {
                            curr.summary.clone()
                        } else {
                            format!("[{}]", format_msg_type(curr.msg_type))
                        };
                        (
                            curr.sender.clone(),
                            !self.self_username.is_empty() && curr.sender == self.self_username,
                            content_text,
                            None,
                            None,
                            curr.timestamp,
                        )
                    };

                let sender = if is_group {
                    contact_names
                        .get(&sender_username)
                        .cloned()
                        .unwrap_or_else(|| sender_username.clone())
                } else {
                    String::new()
                };

                let msg = WeChatMessage {
                    time: Self::format_time(ts),
                    timestamp: ts * us_per_sec,
                    local_id,
                    sort_seq,
                    session_type: if is_group { "group" } else { "private" }.to_string(),
                    chat: display,
                    username: username.clone(),
                    is_group,
                    sender,
                    sender_username,
                    is_send,
                    msg_type: curr.msg_type,
                    content: content_text,
                    media_type: media_type(curr.msg_type).map(|s| s.to_string()),
                    decrypt_ms: self.decrypt_ms.load(std::sync::atomic::Ordering::Relaxed) as f64,
                    pages: self
                        .patched_pages
                        .load(std::sync::atomic::Ordering::Relaxed),
                    image_url: None,
                    rich: None,
                };
                new_msgs.push(msg);
            } else {
                let mut shown = self.shown_keys.write().await;
                let mut max_local_id = 0i64;
                let mut max_sort_seq = 0i64;
                let mut max_create_time = 0i64;
                for (local_id, ts, lt, mc, _svr_id, sort_seq, sender_username) in &all_rows {
                    max_local_id = max_local_id.max(*local_id);
                    max_sort_seq = max_sort_seq.max(*sort_seq);
                    max_create_time = max_create_time.max(*ts);
                    let base = *lt as i64;
                    let base = if base > (1i64 << 32) {
                        base % (1i64 << 32)
                    } else {
                        base
                    };

                    if !shown.insert((username.clone(), *local_id, *sort_seq)) {
                        continue; // 已推送
                    }

                    // 真实发送者：
                    // - 群消息优先按 "wxid_xxx:\n" 前缀剥离（逐条还原，避免批量推送时
                    //   所有消息都错用会话最后一条的发送人 curr.sender）
                    // - 私聊直接使用 Name2Id 解析出的真实发送者（自己或对方），
                    //   修复"自己发消息显示为对方"的方向错误
                    let (real_sender, content_text) = if is_group && mc.contains(":\n") {
                        let parts: Vec<&str> = mc.splitn(2, ":\n").collect();
                        (
                            parts.first().copied().unwrap_or("").to_string(),
                            parts.get(1).copied().unwrap_or(mc.as_str()).to_string(),
                        )
                    } else {
                        (
                            if sender_username.is_empty() {
                                username.clone()
                            } else {
                                sender_username.clone()
                            },
                            mc.clone(),
                        )
                    };

                    let sender = if is_group {
                        contact_names
                            .get(&real_sender)
                            .cloned()
                            .unwrap_or_else(|| real_sender.clone())
                    } else {
                        String::new()
                    };
                    let is_send =
                        !self.self_username.is_empty() && real_sender == self.self_username;

                    // 图片消息不再内联解码：HEVC→JPEG 转码等耗时操作（50~300ms+）
                    // 若内联到推送热路径会阻塞整个监控任务，拖垮所有消息延迟。
                    // 热路径只下发轻量元数据；前端收到后按需经 get_message_image
                    // IPC 懒加载解密（该调用在独立阻塞线程池执行，不阻塞监控）。
                    let image_url: Option<String> = None;

                    // 富媒体消息: 解析 XML
                    // mmreader 图文推送（腾讯新闻等）local_type=1，需单独识别；
                    // 其余富媒体仅在 base > 1 时解析（与浏览路径行为一致）
                    let rich = if !mc.is_empty() && mc.contains('<') {
                        if mc.contains("<mmreader>") {
                            crate::wechat::media::parse_mmreader(mc)
                                .and_then(|r| serde_json::to_value(r).ok())
                        } else if base > 1 {
                            crate::wechat::media::parse_rich_content(mc, base as i32)
                                .and_then(|r| serde_json::to_value(r).ok())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let msg = WeChatMessage {
                        time: Self::format_time(*ts),
                        timestamp: *ts * us_per_sec,
                        local_id: Some(*local_id),
                        sort_seq: Some(*sort_seq),
                        session_type: if is_group { "group" } else { "private" }.to_string(),
                        chat: display.clone(),
                        username: username.clone(),
                        is_group,
                        sender,
                        sender_username: real_sender.clone(),
                        is_send,
                        msg_type: base as i32,
                        content: if base == 1 {
                            // mmreader 已解析为 rich 卡片时，content 用头条标题做摘要，
                            // 避免把 13KB 原始 XML 推给前端
                            if rich.is_some() && content_text.contains("<mmreader>") {
                                rich.as_ref()
                                    .and_then(|r| {
                                        r.get("items")?
                                            .get(0)?
                                            .get("title")?
                                            .as_str()
                                            .map(|s| s.to_string())
                                    })
                                    .unwrap_or_else(|| "[图文消息]".to_string())
                            } else {
                                content_text
                            }
                        } else {
                            format!("[{}]", format_msg_type(base as i32))
                        },
                        media_type: media_type(base as i32).map(|s| s.to_string()),
                        decrypt_ms: self.decrypt_ms.load(std::sync::atomic::Ordering::Relaxed)
                            as f64,
                        pages: self
                            .patched_pages
                            .load(std::sync::atomic::Ordering::Relaxed),
                        image_url,
                        rich,
                    };
                    new_msgs.push(msg);
                }

                watermark_updates.push((
                    username.clone(),
                    crate::wechat::watermark::SessionWatermark {
                        local_id: max_local_id,
                        sort_seq: max_sort_seq,
                        create_time: max_create_time,
                    },
                ));

                // 清理 shown_keys 上限（按 (username, local_id) 排序保留下边界，
                // 避免原 HashSet 迭代顺序不确定导致随机裁剪、误删去重信息）
                if shown.len() > 10000 {
                    let mut keys: Vec<_> = shown.iter().cloned().collect();
                    keys.sort();
                    let keep_len = keys.len().saturating_sub(5000);
                    for k in keys.drain(..keep_len) {
                        shown.remove(&k);
                    }
                }
            }
        }

        // 按时间排序
        new_msgs.sort_by_key(|a| a.timestamp);

        // 更新 prev_state 与水线
        drop(prev_state);
        *self.prev_state.write().await = curr_state;
        if !watermark_updates.is_empty() {
            self.watermark_store.batch_update(watermark_updates).await;
        }

        new_msgs
    }

    fn format_time(ts: i64) -> String {
        let secs = ts as u64;
        let dur = std::time::Duration::from_secs(secs);
        if let Some(dt) = chrono::DateTime::from_timestamp(dur.as_secs() as i64, 0) {
            dt.format("%H:%M:%S").to_string()
        } else {
            "??:??:??".to_string()
        }
    }
}

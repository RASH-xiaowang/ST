// ============================================================
// 消息通道 — 账号管理
// 自 manager.rs 拆分：账号列表/重命名/解绑、状态汇总、
// 账号读取（通用通道分发）。
// ============================================================

use super::{BotManager, BotStatusSummary};
use crate::bot::db::{self, BotAccount};

impl BotManager {
    // ─── 账号管理 ───

    pub fn list_accounts(&self) -> Vec<BotAccount> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        db::list_accounts(&conn).unwrap_or_default()
    }

    pub fn rename_account(&self, id: i64, name: String) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE bot_accounts SET name=?1, updated_at=datetime('now','localtime') WHERE id=?2",
            rusqlite::params![name, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn unbind_account(&self, id: i64) -> Result<(), String> {
        if let Some(runtime) = self
            .accounts
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
        {
            runtime.cancel.cancel();
        }
        self.accounts
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id);
        let conn = self.conn()?;
        db::delete_account(&conn, id).map_err(|e| e.to_string())?;
        self.emit(
            "bot://status",
            &serde_json::json!({ "accountId": id, "status": "disabled" }),
        );
        log::info!("[bot] 账号 {id} 已解绑");
        Ok(())
    }

    pub fn status_summary(&self) -> BotStatusSummary {
        let mut s = BotStatusSummary {
            total: 0,
            online: 0,
            expired: 0,
            error: 0,
        };
        for acc in self.list_accounts() {
            s.total += 1;
            match acc.status.as_str() {
                "online" | "expiring" | "connecting" => s.online += 1,
                "expired" => s.expired += 1,
                "error" => s.error += 1,
                _ => {}
            }
        }
        s
    }

    // ─── 发送 ───

    /// 读取账号（供通用通道分发使用）
    pub(crate) async fn require_account(&self, account_id: i64) -> Result<BotAccount, String> {
        let conn = self.conn()?;
        db::get_account(&conn, account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "账号不存在".to_string())
    }
}

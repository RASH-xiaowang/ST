//! 通用数据模块 - 对应 PC 微信各类功能记录
//!
//! 数据来源：`general/general.db`
//! - `FMessageTable`           好友验证消息（「新的朋友」）
//! - `transferTable`           转账记录
//! - `redEnvelopeTable`        红包记录
//! - `groupPayTable`           群收款
//! - `revokemessage`           撤回消息
//! - `reddot`                  红点通知
//! - `SearchRecent`            最近搜索
//! - `ForwardRecent`           最近转发
//! - `autoDownloadFileTable`   自动下载文件
//! - `VoiceToTextTable`        语音转文字
//! - `AuthInfo` / `LoginDeviceInfo` 登录设备
//!
//! 结构未知的表以原始行列形式返回，保证信息完整不丢失。

use super::common;
use serde::Serialize;
use std::path::Path;

/// 一个数据分类
#[derive(Debug, Serialize)]
pub struct GeneralCategory {
    /// 分类键
    pub key: String,
    /// 中文名
    pub label: String,
    /// 表名
    pub table: String,
    /// 列名
    pub columns: Vec<String>,
    /// 列中文名（与 columns 对齐，未知则同列名）
    pub column_labels: Vec<String>,
    /// 行数据
    pub rows: Vec<Vec<serde_json::Value>>,
    /// 行数
    pub count: usize,
    /// 表中真实总行数（count 可能受 limit 截断）
    pub total: usize,
}

/// 已知表元数据（表名 / 中文名 / 列名映射）
struct KnownTable {
    table: &'static str,
    label: &'static str,
    columns: &'static [(&'static str, &'static str)],
}

const fn known(
    table: &'static str,
    label: &'static str,
    columns: &'static [(&'static str, &'static str)],
) -> KnownTable {
    KnownTable {
        table,
        label,
        columns,
    }
}

const KNOWN_TABLES: &[KnownTable] = &[
    known(
        "FMessageTable",
        "好友验证",
        &[
            ("username", "用户名"),
            ("nickname", "昵称"),
            ("aliasname", "微信号"),
            ("conremark", "备注"),
            ("verifycontent", "验证消息"),
            ("type", "类型"),
            ("createtime", "时间"),
            ("status", "状态"),
            ("scene", "来源"),
            ("encryptusername", "加密用户名"),
        ],
    ),
    known(
        "transferTable",
        "转账记录",
        &[
            ("transfer_id", "转账ID"),
            ("receiver_username", "收款人"),
            ("amount", "金额(分)"),
            ("fee", "手续费"),
            ("state", "状态"),
            ("pay_memo", "备注"),
            ("create_time", "时间"),
        ],
    ),
    known(
        "redEnvelopeTable",
        "红包记录",
        &[
            ("msg_svr_id", "消息ID"),
            ("sender_username", "发送人"),
            ("receiver_username", "接收人"),
            ("amount", "金额(分)"),
            ("status", "状态"),
            ("send_time", "发送时间"),
            ("receive_time", "领取时间"),
            ("wish_title", "祝福语"),
            ("red_envelope_id", "红包ID"),
        ],
    ),
    known("groupPayTable", "群收款", &[]),
    known(
        "revokemessage",
        "撤回消息",
        &[("create_time", "时间"), ("newxml", "撤回内容")],
    ),
    known("reddot", "红点通知", &[]),
    known(
        "SearchRecent",
        "最近搜索",
        &[("key", "搜索词"), ("timestamp", "时间")],
    ),
    known(
        "ForwardRecent",
        "最近转发",
        &[("key", "对象"), ("timestamp", "时间")],
    ),
    known(
        "autoDownloadFileTable",
        "自动下载文件",
        &[
            ("filename", "文件名"),
            ("filesize", "大小"),
            ("createtime", "时间"),
        ],
    ),
    known("VoiceToTextTable", "语音转文字", &[]),
    known("AuthInfo", "授权信息", &[]),
    known("LoginDeviceInfo", "登录设备", &[]),
    known("NewWcdbGameMsg", "游戏消息", &[]),
    known("GetContactSession", "联系人会话", &[]),
    known("FinderMuteList", "视频号免打扰", &[]),
    known("getcontactinfo", "联系人信息", &[]),
];

/// 时间列候选（用于排序）
const TIME_COLUMNS: &[&str] = &[
    "create_time",
    "createtime",
    "timestamp",
    "send_time",
    "receive_time",
    "modify_time",
];

/// 读取 general.db 中的所有已知分类
pub fn get_general_settings(
    decrypted_dir: &Path,
    limit_per_table: usize,
) -> Result<Vec<GeneralCategory>, String> {
    let db_path = decrypted_dir.join("general").join("general.db");
    if !db_path.exists() {
        return Err(format!("通用数据库未解密: {}", db_path.display()));
    }
    let conn = common::open_readonly_db(&db_path).map_err(|e| format!("打开失败: {}", e))?;

    let mut categories = Vec::new();

    for kt in KNOWN_TABLES {
        if !common::table_exists(&conn, kt.table) {
            continue;
        }
        let cols = common::table_columns(&conn, kt.table);
        if cols.is_empty() {
            continue;
        }
        // 排序列：第一个存在的时间列
        let order_col = TIME_COLUMNS.iter().find(|c| cols.iter().any(|x| x == **c));
        let (cols, rows) =
            match common::dump_table(&conn, kt.table, order_col.copied(), limit_per_table) {
                Some(v) => v,
                None => continue,
            };
        let column_labels: Vec<String> = cols
            .iter()
            .map(|c| {
                kt.columns
                    .iter()
                    .find(|(k, _)| k == c)
                    .map(|(_, v)| v.to_string())
                    .unwrap_or_else(|| c.clone())
            })
            .collect();
        let total = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM \"{}\"", kt.table.replace('"', "")),
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(rows.len() as i64) as usize;
        let count = rows.len();
        categories.push(GeneralCategory {
            key: kt.table.to_string(),
            label: kt.label.to_string(),
            table: kt.table.to_string(),
            columns: cols,
            column_labels,
            rows,
            count,
            total,
        });
    }

    Ok(categories)
}

/// 导出指定分类为 CSV（表头用中文列名，最多 5000 行）
pub fn export_category_csv(decrypted_dir: &Path, table: &str) -> Result<String, String> {
    let db_path = decrypted_dir.join("general").join("general.db");
    if !db_path.exists() {
        return Err(format!("通用数据库未解密: {}", db_path.display()));
    }
    let conn = common::open_readonly_db(&db_path).map_err(|e| format!("打开失败: {}", e))?;
    if !common::table_exists(&conn, table) {
        return Err(format!("分类不存在: {}", table));
    }
    let col_map = KNOWN_TABLES
        .iter()
        .find(|kt| kt.table == table)
        .map(|kt| kt.columns)
        .unwrap_or(&[]);
    let order_col = {
        let cols = common::table_columns(&conn, table);
        TIME_COLUMNS
            .iter()
            .find(|c| cols.iter().any(|x| x == **c))
            .copied()
    };
    let (cols, rows) = common::dump_table(&conn, table, order_col, 5000)
        .ok_or_else(|| "读取分类失败".to_string())?;

    let escape = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let header = cols
        .iter()
        .map(|c| {
            col_map
                .iter()
                .find(|(k, _)| k == c)
                .map(|(_, v)| escape(v))
                .unwrap_or_else(|| escape(c))
        })
        .collect::<Vec<_>>()
        .join(",");
    let mut lines = vec![header];
    for row in &rows {
        let line = row
            .iter()
            .map(|v| match v {
                serde_json::Value::Null => String::new(),
                other => escape(&other.to_string()),
            })
            .collect::<Vec<_>>()
            .join(",");
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    #[ignore = "需要真实 general.db"]
    fn smoke_general_settings_real() {
        let cfg = crate::wechat::config::WeChatConfig::load().expect("加载微信配置");
        let cats = get_general_settings(&cfg.decrypted_dir, 50).expect("读取通用数据");
        assert!(!cats.is_empty());
        for c in &cats {
            assert!(c.total >= c.count, "{} total<count", c.table);
            eprintln!(
                "{}: total={} loaded={} cols={}",
                c.label,
                c.total,
                c.count,
                c.columns.len()
            );
        }
        let csv = export_category_csv(&cfg.decrypted_dir, "transferTable").expect("导出 CSV");
        assert!(
            csv.contains("转账"),
            "CSV 表头异常: {}",
            &csv[..csv.len().min(120)]
        );
        assert!(csv.lines().count() > 1, "CSV 应包含表头与数据行");
    }
}

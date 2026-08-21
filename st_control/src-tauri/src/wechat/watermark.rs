// ============================================================
// 会话水位线管理 —— WatermarkStore
// ============================================================
// 架构文档章节：4.2 高并发优化 / Phase 2 会话级 watermark
// 职责：
//   1. 为每个会话维护已消费消息的三元组水位线：
//      - local_id：消息表自增 ID（表内唯一）
//      - sort_seq：后端排序游标（毫秒级时间戳相关）
//      - create_time：消息创建时间戳
//   2. 增量同步时只查询 > 水位线的记录，避免全量扫描
//   3. 提供批量更新接口，供消息爆发时一次性推进
// 边界条件：
//   - 首次监控某会话时水位线为空，走全量查询后建立 baseline
//   - 同一 ack_id 重复到达不会导致水位线回退（取 max）
//   - 持久化失败时仅降级为内存水位线，不阻塞主流程
// ============================================================

use std::collections::HashMap;
use std::path::PathBuf;

/// 单个会话的水位线
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionWatermark {
    /// 消息表自增 ID
    pub local_id: i64,
    /// 排序游标
    pub sort_seq: i64,
    /// 消息创建时间戳（毫秒或秒，按后端约定）
    pub create_time: i64,
}

impl SessionWatermark {
    /// 取较大的水位线，防止重复/乱序消息导致回退
    pub fn max(self, other: Self) -> Self {
        Self {
            local_id: self.local_id.max(other.local_id),
            sort_seq: self.sort_seq.max(other.sort_seq),
            create_time: self.create_time.max(other.create_time),
        }
    }
}

/// 会话水位线存储
pub struct WatermarkStore {
    map: tokio::sync::Mutex<HashMap<String, SessionWatermark>>,
    /// 持久化路径；None 表示纯内存模式
    persist_path: Option<PathBuf>,
}

impl WatermarkStore {
    /// 创建水位线存储
    ///
    /// `persist_path` 可传入应用数据目录下的 json 文件路径；
    /// 为 None 时完全内存化，进程重启后需重新建立 baseline。
    pub fn new(persist_path: Option<PathBuf>) -> Self {
        let mut store = Self {
            map: tokio::sync::Mutex::new(HashMap::new()),
            persist_path,
        };
        if let Err(e) = store.load() {
            log::warn!("[watermark] 加载持久化水位线失败: {}", e);
        }
        store
    }

    /// 获取某会话当前水位线
    pub async fn get(&self, username: &str) -> Option<SessionWatermark> {
        self.map.lock().await.get(username).copied()
    }

    /// 推进单会话水位线（取较大值）
    pub async fn update(&self, username: &str, watermark: SessionWatermark) {
        let mut map = self.map.lock().await;
        map.entry(username.to_string())
            .and_modify(|e| *e = e.max(watermark))
            .or_insert(watermark);
    }

    /// 批量推进水位线
    pub async fn batch_update(&self, updates: Vec<(String, SessionWatermark)>) {
        let mut map = self.map.lock().await;
        for (username, watermark) in updates {
            map.entry(username)
                .and_modify(|e| *e = e.max(watermark))
                .or_insert(watermark);
        }
    }

    /// 返回所有会话水位线快照（调试用）
    pub async fn snapshot(&self) -> HashMap<String, SessionWatermark> {
        self.map.lock().await.clone()
    }

    /// 保存到磁盘（如路径可用）
    pub async fn save(&self) -> Result<(), String> {
        let path = match &self.persist_path {
            Some(p) => p,
            None => return Ok(()),
        };
        let data = self.map.lock().await.clone();
        let json = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
        tokio::fs::write(path, json)
            .await
            .map_err(|e| e.to_string())
    }

    fn load(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let path = match &self.persist_path {
            Some(p) => p,
            None => return Ok(()),
        };
        if !path.exists() {
            return Ok(());
        }
        let json = std::fs::read_to_string(path)?;
        let data: HashMap<String, SessionWatermark> = serde_json::from_str(&json)?;
        // new() 中 self 是 mut 且尚未共享，可直接替换内部 HashMap，避免 blocking_lock()
        *self.map.get_mut() = data;
        Ok(())
    }
}

/** 微信数据洞察：三步本地流程 + AI 提问 + 年度总结预览 + 隐私引擎 + 真实查询示例 */

export const wechatInsights = {
  title: {
    zh: "你的微信档案，值得被读懂",
    en: "Your WeChat archive, finally readable",
  },
  subtitle: {
    zh: "解密 · 浏览 · 搜索 · 导出 · 年度总结 —— 全程在这台电脑上离线完成，0 字节出网。",
    en: "Decrypt · browse · search · export · annual summary — all offline on this machine, 0 bytes egress.",
  },
  steps: {
    title: {
      zh: "三步，把数据放回你手里",
      en: "Three steps to put the data back in your hands",
    },
    items: [
      {
        no: "01",
        name: { zh: "解锁密钥", en: "Unlock the key" },
        desc: {
          zh: "通过 wx_key.dll 钩子在本机微信进程内取回 64 位数据库口令，PBKDF2 逐库校验后写入 all_keys.json。",
          en: "A wx_key.dll hook retrieves the 64-bit database passphrase inside the local WeChat process; each DB is PBKDF2-verified into all_keys.json.",
        },
      },
      {
        no: "02",
        name: { zh: "本地解密", en: "Decrypt locally" },
        desc: {
          zh: "将 session / message / contact / sns 等数据库解密为 data/wechat/ 下的只读副本，原文件不改动。",
          en: "Decrypt session, message, contact and SNS databases into a read-only copy under data/wechat/, leaving originals untouched.",
        },
      },
      {
        no: "03",
        name: { zh: "分析与导出", en: "Analyze & export" },
        desc: {
          zh: "全文检索、年度总结、朋友圈洞察、隐私扫描与 Markdown / HTML 导出，全部由本地引擎完成。",
          en: "Full-text search, annual summary, moments insights, privacy scan and Markdown / HTML export — all powered by local engines.",
        },
      },
    ],
  },
  ask: {
    title: { zh: "问你的聊天记录", en: "Ask your archive" },
    subtitle: {
      zh: "自然语言问题先被解析成检索计划（关键词 / 会话 / 时间范围 / 数据源），在本地证据上检索，再由 LLM 组织带引用的回答。",
      en: "A natural-language question is parsed into a retrieval plan (keywords / session / time range / data source), evidence is searched locally, then the LLM composes a cited answer.",
    },
    bullets: [
      {
        zh: "未配置模型时仍返回证据列表，检索与回答解耦",
        en: "Retrieval and answering are decoupled — you still get an evidence list without a model configured",
      },
      {
        zh: "检索只读本地解密副本，不触碰微信源库",
        en: "Search reads the local decrypted copy only; the WeChat source databases are never touched",
      },
      {
        zh: "LLM 只负责规划与组织，所有事实必须来自检索到的证据",
        en: "The LLM only plans and organizes; every fact must come from retrieved evidence",
      },
      {
        zh: "数据源覆盖消息 / 转账 / 红包 / 联系人 / 朋友圈 / 收藏",
        en: "Data sources cover messages, transfers, red packets, contacts, moments and favorites",
      },
    ],
    sample: {
      title: { zh: "问题 → 检索计划 → 证据 → 引用回答", en: "Question → plan → evidence → cited answer" },
      code: {
        zh: `Q: 去年冬天我们约在老地方见，是哪天？谁先提的？

// 检索计划（AskPlan，LLM 生成或启发式推导）
{
  "keywords": ["老地方", "见面"],
  "time_from": "2025-11-01",
  "time_to": "2026-02-28",
  "data_sources": ["messages"],
  "limit": 24,
  "rationale": "关键词命中两条会话，时间范围限定在冬季"
}

// 证据（本地解密副本检索，只读）
[{"ts": 1766134800, "name": "老友", "text": "周六老地方见？"},
 {"ts": 1766156400, "name": "我", "text": "好，还是那家店"}]

// 回答（LLM 组织，带引用标注）
12 月 19 日由「老友」先提出见面（证据 1），你随后确认。`,
        en: `Q: Last winter we agreed to meet "at the usual place" — what day, and who brought it up?

// Retrieval plan (AskPlan, LLM-generated or heuristic)
{
  "keywords": ["usual place", "meet"],
  "time_from": "2025-11-01",
  "time_to": "2026-02-28",
  "data_sources": ["messages"],
  "limit": 24,
  "rationale": "keywords hit two sessions, time window narrowed to winter"
}

// Evidence (read-only search of the local decrypted copy)
[{"ts": 1766134800, "name": "Friend", "text": "Same place on Saturday?"},
 {"ts": 1766156400, "name": "Me", "text": "Sure, that spot again"}]

// Answer (LLM-organized, with citations)
December 19 — "Friend" proposed it first (evidence 1), you confirmed.`,
      },
    },
    note: {
      zh: "以上为交互形态示例；实际回答以你本地数据的检索证据为准。",
      en: "Sample interaction; real answers are grounded in evidence retrieved from your local data.",
    },
  },
  annual: {
    title: {
      zh: "年度总结：这一年值得被数据温柔复述",
      en: "Annual Wrapped: a year, retold in data",
    },
    subtitle: {
      zh: "所有数字都来自你本地解密后的真实消息——不编造故事，只呈现痕迹。",
      en: "Every number comes from your locally decrypted messages — no invented stories, only the traces you left.",
    },
    disclaimer: {
      zh: "以下为界面形态示意（数据为占位），接入你的本地数据后由真实聚合自动计算。",
      en: "Illustrative UI preview (placeholder values); with your local data, every figure is computed by real aggregation.",
    },
    frames: [
      {
        key: "overview",
        icon: "📊",
        name: { zh: "年度总览", en: "Year in numbers" },
        field: "total_messages · active_days",
        hint: { zh: "消息总量与活跃天数", en: "message total and active days" },
        visual: { kind: "stat", primary: "128,456", secondary: "312 天活跃", primaryEn: "128,456", secondaryEn: "312 active days" },
      },
      {
        key: "rhythm",
        icon: "🕐",
        name: { zh: "聊天作息", en: "Chat rhythm" },
        field: "heatmap",
        hint: {
          zh: "星期 × 小时热力矩阵，找出最活跃的时段",
          en: "weekday × hour heatmap, revealing your peak hours",
        },
        visual: { kind: "heatmap" },
      },
      {
        key: "chars",
        icon: "✍️",
        name: { zh: "字数统计", en: "Words written" },
        field: "total_chars · avg_chars",
        hint: { zh: "总字数与平均单条字数", en: "total and average characters per message" },
        visual: { kind: "chars", primary: "2,143,090", secondary: "平均 41 字", primaryEn: "2,143,090", secondaryEn: "41 chars avg" },
      },
      {
        key: "latency",
        icon: "🌙",
        name: { zh: "最早与最晚", en: "Earliest & latest" },
        field: "earliest · latest",
        hint: { zh: "第一句与最后一句，最晚的一次夜聊", en: "the first and last messages, your latest late-night chat" },
        visual: { kind: "clock", latest: "00:42", earliest: "2025-01-02" },
      },
      {
        key: "emoji",
        icon: "😄",
        name: { zh: "表情宇宙", en: "Emoji universe" },
        field: "top_emojis",
        hint: { zh: "这一年用得最多的表情排行", en: "the emojis you used most this year" },
        visual: { kind: "bars", items: [{ label: "😂", value: 1287 }, { label: "👍", value: 964 }, { label: "❤️", value: 712 }, { label: "🤝", value: 588 }] },
      },
      {
        key: "friends",
        icon: "💬",
        name: { zh: "挚友墙", en: "Top contacts & groups" },
        field: "top_contacts · top_groups",
        hint: { zh: "聊天最多的联系人与群聊", en: "your most-chatted contacts and groups" },
        visual: {
          kind: "bars",
          items: [
            { label: { zh: "挚友 A", en: "Friend A" }, value: 2431 },
            { label: { zh: "挚友 B", en: "Friend B" }, value: 1876 },
            { label: { zh: "群聊 C", en: "Group C" }, value: 1490 },
            { label: { zh: "挚友 D", en: "Friend D" }, value: 1102 },
          ],
        },
      },
      {
        key: "phrases",
        icon: "🔑",
        name: { zh: "年度关键词", en: "Signature phrases" },
        field: "top_phrases",
        hint: { zh: "这一年你说得最多的话", en: "the phrases you said the most" },
        visual: {
          kind: "phrases",
          items: [
            { zh: "到了跟我说一声", en: "Text me when you land" },
            { zh: "老地方见", en: "Same place as always" },
            { zh: "恭喜发财", en: "Gong xi fa cai" },
            { zh: "改天约", en: "Let's plan it soon" },
            { zh: "收到", en: "Got it" },
          ],
        },
      },
      {
        key: "months",
        icon: "📅",
        name: { zh: "月度热力", en: "Monthly heat" },
        field: "monthly_counts",
        hint: { zh: "12 个月的消息分布曲线", en: "message distribution across twelve months" },
        visual: { kind: "months", values: [38, 42, 31, 45, 52, 48, 39, 44, 57, 63, 71, 66] },
      },
    ] as const,
  },
  privacy: {
    title: {
      zh: "解密、浏览、导出 —— 全程不出这台电脑",
      en: "Decrypt, browse, export — none of it leaves this machine",
    },
    subtitle: {
      zh: "密钥在进程内读取、解密副本只读、扫描结果仅存内存：每一环都按「零出境」设计。",
      en: "The key is read in-process, decrypted copies are read-only and scan results live only in memory — every link is built for zero egress.",
    },
    stat: {
      value: "0",
      unit: "B",
      label: { zh: "数据出境 · 本地处理", en: "bytes egress · processed locally" },
    },
    facts: [
      {
        icon: "🔑",
        title: { zh: "密钥不出进程", en: "Key stays in-process" },
        desc: {
          zh: "wx_key.dll 钩子在本机微信进程内取回口令，PBKDF2 逐库校验，不经过网络。",
          en: "The wx_key.dll hook reads the passphrase inside the local WeChat process; each DB is PBKDF2-verified — no network hop.",
        },
      },
      {
        icon: "📂",
        title: { zh: "解密副本只读", en: "Read-only decrypted copies" },
        desc: {
          zh: "session / message / contact / sns 解密到 data/wechat/ 只读副本，微信源库零改动。",
          en: "session, message, contact and SNS databases decrypt into read-only copies under data/wechat/; the WeChat originals are untouched.",
        },
      },
      {
        icon: "🛡️",
        title: { zh: "扫描结果仅内存", en: "Scan results stay in memory" },
        desc: {
          zh: "隐私体检覆盖 6 类敏感信息，60 万行预算、每类最多 200 条样本，结果不落盘。",
          en: "The privacy scan covers 6 sensitive categories with a 600k-row budget and 200 samples per category; results are never written to disk.",
        },
      },
      {
        icon: "💬",
        title: { zh: "AI 回答受证据约束", en: "AI answers are evidence-bound" },
        desc: {
          zh: "「问我的微信」中 LLM 只做规划与组织，所有事实必须来自本地检索到的证据。",
          en: "In “Ask my WeChat”, the LLM only plans and organizes; every fact must come from locally retrieved evidence.",
        },
      },
    ],
    engine: {
      title: { zh: "引擎与平台", en: "Engine & platform" },
      rows: [
        { k: { zh: "引擎", en: "Engine" }, v: { zh: "Rust (Tauri 2) · Svelte 5 · SQLite (WCDB)", en: "Rust (Tauri 2) · Svelte 5 · SQLite (WCDB)" } },
        { k: { zh: "平台", en: "Platform" }, v: { zh: "Windows 桌面原生", en: "Windows desktop native" } },
        { k: { zh: "数据", en: "Data" }, v: { zh: "data/wechat/ 只读解密副本，本地 JSON-RPC", en: "read-only decrypted copies under data/wechat/, local JSON-RPC" } },
      ],
    },
    legal: {
      zh: "仅限解密你本人的微信数据，请遵守当地法律法规。",
      en: "Only for decrypting your own WeChat data — please comply with local laws.",
    },
  },
  insights: {
    title: { zh: "不止聊天记录", en: "More than chat logs" },
    items: [
      {
        key: "recall",
        icon: "↩️",
        name: { zh: "撤回记录", en: "Recalled messages" },
        desc: {
          zh: "谁撤回了什么、类型构成与撤回最多的发送者，可查可导出。",
          en: "Who recalled what, type mix and top recallers — searchable and exportable.",
        },
      },
      {
        key: "transfer",
        icon: "💸",
        name: { zh: "转账记录", en: "Transfers" },
        desc: {
          zh: "单笔状态映射（发出 / 待收 / 已收 / 退回），跨会话分库稳定聚合。",
          en: "Per-transfer status mapping (sent / pending / received / refunded), aggregated across session shards.",
        },
      },
      {
        key: "redpacket",
        icon: "🧧",
        name: { zh: "红包记录", en: "Red packets" },
        desc: {
          zh: "按服务器 ID 关联明细，类型与金额构成可一键聚合。",
          en: "Server-ID linked details, aggregated by type and amount with one click.",
        },
      },
      {
        key: "moments",
        icon: "📸",
        name: { zh: "朋友圈洞察", en: "Moments insights" },
        desc: {
          zh: "作者活跃榜、月度热力与媒体构成，增量重解密让数据保持新鲜。",
          en: "Author leaderboard, monthly heat and media mix — incremental re-decrypt keeps it fresh.",
        },
      },
      {
        key: "storage",
        icon: "🗂️",
        name: { zh: "存储空间", en: "Storage analysis" },
        desc: {
          zh: "媒体分类与会话占用排行，找出悄悄吃满磁盘的大户。",
          en: "Media breakdown and top sessions — find what quietly fills your disk.",
        },
      },
      {
        key: "privacy",
        icon: "🛡️",
        name: { zh: "隐私扫描", en: "Privacy scan" },
        desc: {
          zh: "手机号 / 身份证 / 银行卡 / 邮箱 / 口令 / 地址六类命中，结果仅存内存、不落盘。",
          en: "Regex hits for phone, ID, bank card, email, password and address — results stay in memory only.",
        },
      },
      {
        key: "graph",
        icon: "🕸️",
        name: { zh: "社交图谱", en: "Social graph" },
        desc: {
          zh: "力导向绘制你的人际网络：群友圈子以「我」为中心、节点为联系人、连线 = 共同群数；群聊网络以群为节点、连线 = 共同成员数；社区检测自动着色，亲密度榜 / 共同群榜 / 圈子概览一键展开，可导出 SVG 矢量图与分享海报。",
          en: "A force-directed map of your network: “Circle of friends” centers on you with contacts as nodes and edges = shared groups; “Group network” maps groups with edges = shared members. Communities auto-colored, with intimacy / shared-group leaderboards and circle overviews — exportable as SVG or a shareable poster.",
        },
      },
    ],
  },
  sample: {
    title: { zh: "真实查询长什么样", en: "What a real query looks like" },
    caption: {
      zh: "示意：真实管线跨 message_0.db / message_1.db 等分库游标合并，每个会话一张 Msg_<MD5> 表。",
      en: "Illustrative: the real pipeline merges shards across message_0.db / message_1.db with cursor pagination, one Msg_<MD5> table per session.",
    },
    code: {
      zh: `-- 年度总结核心聚合（本地解密副本，只读）
-- 真实形态：跨 message_0.db / message_1.db … 分库游标合并
SELECT
  COUNT(*)                                          AS total_messages,
  COUNT(DISTINCT date(create_time, 'unixepoch', 'localtime')) AS active_days,
  SUM(length(COALESCE(message_content, '')))        AS total_chars,
  MIN(create_time)                                  AS earliest,
  MAX(create_time)                                  AS latest
FROM "Msg_<会话MD5>"      -- 每个会话一张表
WHERE local_type = 1;     -- 1 = 文本消息`,
      en: `-- Core annual-summary aggregation (local decrypted copy, read-only)
-- Real shape: shards merged with cursor pagination across message_0.db / message_1.db …
SELECT
  COUNT(*)                                          AS total_messages,
  COUNT(DISTINCT date(create_time, 'unixepoch', 'localtime')) AS active_days,
  SUM(length(COALESCE(message_content, '')))        AS total_chars,
  MIN(create_time)                                  AS earliest,
  MAX(create_time)                                  AS latest
FROM "Msg_<session MD5>"  -- one table per session
WHERE local_type = 1;     -- 1 = text message`,
    },
  },
};

<!-- API 接口说明弹窗（自包含组件，从 App.svelte 抽出） -->
<script lang="ts">
  import { getApiSettings, getConversationMessages, getSessionList } from '../wechat/services/ipc';
  import { apiDebugUrl as buildApiDebugUrl } from './apiUrl';
  import { copyText } from '../clipboard';
  import Modal from './Modal.svelte';

  let { open, onClose }: { open: boolean; onClose: () => void } = $props();

  let apiSettings = $state<{ enabled: boolean; port: number; token: string | null } | null>(null);
  let debugTalker = $state('');
  let debugGroup = $state('');
  let debugMediaLocalId = $state<number | null>(null);

  // 打开 API 文档时加载当前 API 设置与调试样本（第一个会话/群/图片消息）
  $effect(() => {
    if (!open) return;
    (async () => {
      try {
        apiSettings = await getApiSettings();
      } catch {
        apiSettings = null;
      }
      try {
        const sessions = await getSessionList();
        if (Array.isArray(sessions) && sessions.length > 0) {
          debugTalker = sessions[0].username || '';
          const group = sessions.find((s) => String(s.username || '').endsWith('@chatroom'));
          debugGroup = group?.username || '';
          // 找一条图片消息作为 media 调试样本
          const probe = debugGroup || debugTalker;
          if (probe) {
            try {
              const page = await getConversationMessages({
                username: probe,
                page: 0,
                pageSize: 30,
                beforeSortSeq: null,
              });
              const img = (page?.messages || []).find((m) => m.type === 3);
              debugMediaLocalId = img ? img.local_id : null;
            } catch {
              debugMediaLocalId = null;
            }
          }
        }
      } catch {
        /* 数据未就绪时调试按钮自动禁用 */
      }
    })();
  });

  /** 构造浏览器调试 URL（自动附带 access_token） */
  function apiDebugUrl(path: string): string {
    return buildApiDebugUrl(path, apiSettings?.port ?? 5032, apiSettings?.token);
  }

  async function openApiDebug(path: string) {
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open(apiDebugUrl(path));
    } catch (e) {
      console.error('[api-debug] 打开失败:', e);
    }
  }
</script>

{#if open}
  <Modal open={open} onClose={onClose} frameStyle="width:880px;max-width:90vw">
      <div class="modal-hd">
        <h2 class="modal-title">API 接口说明</h2>
        <button class="modal-close" onclick={onClose} aria-label="关闭" title="关闭">
          <svg viewBox="0 0 16 16" width="14" height="14" fill="none" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/></svg>
        </button>
      </div>
      <div class="modal-body">
        <!-- ═══ HTTP API ═══ -->
        <div class="api-section">
          <h3 class="api-method">微信数据 HTTP API</h3>
          <div class="api-desc">
            应用启动后自动开启本机 HTTP 服务（仅监听 127.0.0.1，外部无法访问），
            提供微信会话/消息/联系人/群成员只读查询、图片按需解密与 SSE 实时推送。
          </div>
          <h4 class="api-sub">服务地址</h4>
          <div class="api-box">
            <span class="api-cmd">Base URL</span>
            <code class="api-code">http://127.0.0.1:{apiSettings?.port ?? 5032}</code>
            {#if apiSettings}
              <span class="api-status" class:api-status-on={apiSettings.enabled} class:api-status-off={!apiSettings.enabled}>
                {apiSettings.enabled ? '运行中' : '已停用'}
              </span>
            {/if}
          </div>
          <h4 class="api-sub">当前访问令牌</h4>
          <div class="api-box">
            {#if apiSettings?.token}
              <code class="api-code api-token-value">{apiSettings.token}</code>
              <button class="api-debug-btn" onclick={() => { void copyText(apiSettings?.token ?? ''); }}>复制</button>
            {:else}
              <code class="api-code" style="color:var(--app-color-muted)">未配置（免鉴权模式，可在 设置 → 微信数据配置 → HTTP API 服务 中生成）</code>
            {/if}
          </div>
          <h4 class="api-sub">鉴权（三种方式任选）</h4>
          <table class="table api-table">
            <thead><tr><th style="width:200px">方式</th><th>说明</th></tr></thead>
            <tbody>
              <tr><td><code>Authorization: Bearer &lt;token&gt;</code></td><td>请求头（推荐）</td></tr>
              <tr><td><code>?access_token=&lt;token&gt;</code></td><td>Query 参数（SSE 场景推荐）</td></tr>
              <tr><td><code>&#123;"access_token": "&lt;token&gt;"&#125;</code></td><td>POST JSON body 字段</td></tr>
            </tbody>
          </table>
          <ul class="api-notes">
            <li>令牌在 <code>config.json</code> 的 <code>api_token</code> 配置；未配置时免鉴权（仅限本机使用）</li>
            <li>监听端口：<code>api_port</code>（默认 5032）；开关：<code>api_enabled</code>（默认 true）</li>
          </ul>
          <h4 class="api-sub">通用约定</h4>
          <ul class="api-notes">
            <li>时间参数：秒级/毫秒时间戳，或 <code>YYYYMMDD</code>（end 自动扩展到当天 23:59:59）</li>
            <li>分页：<code>limit</code> + <code>offset</code> 或游标 <code>cursor</code>（消息接口返回 <code>nextCursor</code>）</li>
            <li>统一错误：<code>&#123;"success":false,"error":&#123;"code":"...","message":"..."&#125;&#125;</code></li>
          </ul>
        </div>

        <div class="api-section">
          <div class="api-endpoint-head">
            <h3 class="api-method">GET /health</h3>
            <button class="api-debug-btn" onclick={() => openApiDebug('/health')}>调试 ↗</button>
          </div>
          <div class="api-desc">健康检查（免鉴权），返回服务、监控、数据库状态。</div>
          <pre class="api-example">{`{
  "status": "ok",
  "version": "1.0.0",
  "uptimeSeconds": 3600,
  "port": 5032,
  "auth": true,
  "monitor": { "running": true },
  "database": { "ready": true }
}`}</pre>
        </div>

        <div class="api-section">
          <div class="api-endpoint-head">
            <h3 class="api-method">GET /api/v1/sessions</h3>
            <button class="api-debug-btn" onclick={() => openApiDebug('/api/v1/sessions?limit=5')}>调试 ↗</button>
          </div>
          <div class="api-desc">微信会话列表（按最新消息时间倒序），支持关键词过滤。同时支持 POST。</div>
          <h4 class="api-sub">请求参数</h4>
          <table class="table api-table">
            <thead><tr><th style="width:110px">参数</th><th style="width:70px">类型</th><th style="width:70px">必填</th><th>说明</th></tr></thead>
            <tbody>
              <tr><td><code>keyword</code></td><td>string</td><td>否</td><td>用户名/显示名模糊匹配</td></tr>
              <tr><td><code>limit</code></td><td>int</td><td>否</td><td>返回数量，默认 100，最大 1000</td></tr>
              <tr><td><code>offset</code></td><td>int</td><td>否</td><td>分页偏移，默认 0</td></tr>
            </tbody>
          </table>
          <h4 class="api-sub">响应示例</h4>
          <pre class="api-example">{`{
  "success": true,
  "total": 88, "count": 2, "offset": 0, "hasMore": true,
  "sessions": [{
    "username": "45225247671@chatroom",
    "displayName": "智能回复机器人·ai-3群",
    "type": "group",
    "lastTimestamp": 1785167323,
    "summary": "a憨: [撇嘴]",
    "unreadCount": 1,
    "draft": ""
  }]
}`}</pre>
          <h4 class="api-sub">调用示例</h4>
          <pre class="api-example">{`curl -H "Authorization: Bearer <token>" \\
  "http://127.0.0.1:5032/api/v1/sessions?limit=20&keyword=机器人"`}</pre>
        </div>

        <div class="api-section">
          <div class="api-endpoint-head">
            <h3 class="api-method">GET /api/v1/messages</h3>
            <button class="api-debug-btn" disabled={!debugTalker}
              onclick={() => openApiDebug(`/api/v1/messages?talker=${encodeURIComponent(debugTalker)}&limit=5`)}>调试 ↗</button>
          </div>
          <div class="api-desc">指定会话的历史消息（倒序游标分页）。图片消息自动携带即时解密的 mediaUrl。同时支持 POST。</div>
          <h4 class="api-sub">请求参数</h4>
          <table class="table api-table">
            <thead><tr><th style="width:110px">参数</th><th style="width:70px">类型</th><th style="width:70px">必填</th><th>说明</th></tr></thead>
            <tbody>
              <tr><td><code>talker</code></td><td>string</td><td>是</td><td>会话 username（如 wxid_xxx / xxx@chatroom）</td></tr>
              <tr><td><code>limit</code></td><td>int</td><td>否</td><td>每页数量，默认 100，最大 1000</td></tr>
              <tr><td><code>cursor</code></td><td>int</td><td>否</td><td>上一页返回的 nextCursor（sortSeq 游标）</td></tr>
              <tr><td><code>keyword</code></td><td>string</td><td>否</td><td>消息内容关键词（在返回窗口内过滤）</td></tr>
              <tr><td><code>start / end</code></td><td>int</td><td>否</td><td>时间范围（时间戳或 YYYYMMDD）</td></tr>
            </tbody>
          </table>
          <h4 class="api-sub">响应示例</h4>
          <pre class="api-example">{`{
  "success": true, "talker": "48710321370@chatroom", "chatName": "暴富群",
  "total": 12580, "count": 100, "hasMore": true, "nextCursor": 17779900,
  "messages": [{
    "localId": 771, "serverId": 0, "sortSeq": 17780001,
    "createTime": 1785166899, "time": "2026-07-28 09:41:39",
    "isSend": 0, "type": 1, "typeLabel": "文本",
    "senderUsername": "wxid_abc", "senderName": "张三",
    "content": "早上好",
    "rich": null, "mediaUrl": null
  }, {
    "localId": 768, "type": 3, "typeLabel": "图片",
    "mediaUrl": "/api/v1/media/48710321370@chatroom/768"
  }]
}`}</pre>
          <h4 class="api-sub">调用示例</h4>
          <pre class="api-example">{`curl -H "Authorization: Bearer <token>" \\
  "http://127.0.0.1:5032/api/v1/messages?talker=48710321370@chatroom&limit=50&start=20260727"`}</pre>
        </div>

        <div class="api-section">
          <div class="api-endpoint-head">
            <h3 class="api-method">GET /api/v1/sessions/{"{id}"}/messages</h3>
            <button class="api-debug-btn" disabled={!debugTalker}
              onclick={() => openApiDebug(`/api/v1/sessions/${encodeURIComponent(debugTalker)}/messages?limit=5`)}>调试 ↗</button>
          </div>
          <div class="api-desc">增量拉取会话消息（since 之后的最新消息，正序），返回 sync 分页块，适合定时同步。</div>
          <h4 class="api-sub">请求参数</h4>
          <table class="table api-table">
            <thead><tr><th style="width:110px">参数</th><th style="width:70px">类型</th><th style="width:70px">必填</th><th>说明</th></tr></thead>
            <tbody>
              <tr><td><code>id</code></td><td>string</td><td>是</td><td>路径参数，会话 username</td></tr>
              <tr><td><code>since</code></td><td>int</td><td>否</td><td>起始时间（时间戳/YYYYMMDD），默认 0</td></tr>
              <tr><td><code>end</code></td><td>int</td><td>否</td><td>截止时间</td></tr>
              <tr><td><code>limit</code></td><td>int</td><td>否</td><td>最大返回数，默认 500，最大 5000</td></tr>
            </tbody>
          </table>
          <h4 class="api-sub">响应示例（含 sync 块）</h4>
          <pre class="api-example">{`{
  "success": true,
  "chatlab": { "version": "0.0.2", "generator": "st_control" },
  "meta": { "id": "xxx@chatroom", "name": "群名", "platform": "wechat", "type": "group" },
  "count": 12, "messages": [ ... ],
  "sync": { "hasMore": false, "nextCursor": 17780010, "watermark": 1785167400 }
}`}</pre>
        </div>

        <div class="api-section">
          <div class="api-endpoint-head">
            <h3 class="api-method">GET /api/v1/contacts</h3>
            <button class="api-debug-btn" onclick={() => openApiDebug('/api/v1/contacts?limit=5')}>调试 ↗</button>
          </div>
          <div class="api-desc">通讯录联系人列表，支持分类与关键词过滤。同时支持 POST。</div>
          <h4 class="api-sub">请求参数</h4>
          <table class="table api-table">
            <thead><tr><th style="width:110px">参数</th><th style="width:70px">类型</th><th style="width:70px">必填</th><th>说明</th></tr></thead>
            <tbody>
              <tr><td><code>category</code></td><td>string</td><td>否</td><td>friends / chatrooms / openim / specials</td></tr>
              <tr><td><code>keyword</code></td><td>string</td><td>否</td><td>wxid/昵称/备注/微信号模糊匹配</td></tr>
              <tr><td><code>limit / offset</code></td><td>int</td><td>否</td><td>分页，默认 100，最大 5000</td></tr>
            </tbody>
          </table>
          <h4 class="api-sub">调用示例</h4>
          <pre class="api-example">{`curl -H "Authorization: Bearer <token>" \\
  "http://127.0.0.1:5032/api/v1/contacts?category=friends&keyword=张"`}</pre>
        </div>

        <div class="api-section">
          <div class="api-endpoint-head">
            <h3 class="api-method">GET /api/v1/group-members</h3>
            <button class="api-debug-btn" disabled={!debugGroup}
              onclick={() => openApiDebug(`/api/v1/group-members?chatroomId=${encodeURIComponent(debugGroup)}`)}>调试 ↗</button>
          </div>
          <div class="api-desc">群聊成员列表（含群昵称、群主标记）。同时支持 POST。</div>
          <h4 class="api-sub">请求参数</h4>
          <table class="table api-table">
            <thead><tr><th style="width:110px">参数</th><th style="width:70px">类型</th><th style="width:70px">必填</th><th>说明</th></tr></thead>
            <tbody>
              <tr><td><code>chatroomId</code></td><td>string</td><td>是</td><td>群聊 username（xxx@chatroom）</td></tr>
            </tbody>
          </table>
          <h4 class="api-sub">响应示例</h4>
          <pre class="api-example">{`{
  "success": true, "chatroomId": "45225247671@chatroom", "count": 42,
  "members": [{
    "wxid": "wxid_abc123", "displayName": "张三", "nickname": "三哥",
    "remark": "张三", "alias": "", "groupNickname": "三哥-广州",
    "avatarUrl": "https://wx.qlogo.cn/...", "isOwner": false
  }]
}`}</pre>
        </div>

        <div class="api-section">
          <div class="api-endpoint-head">
            <h3 class="api-method">GET /api/v1/media/{"{username}"}/{"{local_id}"}</h3>
            <button class="api-debug-btn" disabled={!debugTalker || debugMediaLocalId == null}
              onclick={() => openApiDebug(`/api/v1/media/${encodeURIComponent(debugGroup || debugTalker)}/${debugMediaLocalId}`)}>调试 ↗</button>
          </div>
          <div class="api-desc">
            按消息 ID 即时解密图片，直接返回图片二进制（image/jpeg 等）。
            wxgf/HEVC 格式自动转码为 JPEG；结果有磁盘缓存，二次请求毫秒级返回。
          </div>
          <h4 class="api-sub">调用示例</h4>
          <pre class="api-example">{`curl -H "Authorization: Bearer <token>" \\
  "http://127.0.0.1:5032/api/v1/media/48710321370@chatroom/768" -o image.jpg

# HTML 中直接引用
<img src="http://127.0.0.1:5032/api/v1/media/48710321370@chatroom/768?access_token=<token>">`}</pre>
        </div>

        <div class="api-section">
          <div class="api-endpoint-head">
            <h3 class="api-method">GET /api/v1/monitor/status</h3>
            <button class="api-debug-btn" onclick={() => openApiDebug('/api/v1/monitor/status')}>调试 ↗</button>
          </div>
          <div class="api-desc">实时消息监控运行状态与推送指标（ACK 积压、推送计数、延迟分桶）。</div>
          <pre class="api-example">{`{
  "success": true, "running": true, "uptimeSeconds": 7200,
  "wsPort": 9787,
  "metrics": { "pendingAcks": 0, "sentTotal": 1523, "sentBatchCount": 1480,
               "sentWsCount": 43, "latency": { "buckets": [1400, 120, 3], "sumMs": 42000, "count": 1523 } }
}`}</pre>
        </div>

        <div class="api-section">
          <div class="api-endpoint-head">
            <h3 class="api-method">GET /api/v1/push/messages（SSE）</h3>
            <button class="api-debug-btn" onclick={() => openApiDebug('/api/v1/push/messages')}>调试 ↗</button>
          </div>
          <div class="api-desc">
            Server-Sent Events 实时消息推送。连接后持续接收新消息；
            支持 <code>Last-Event-ID</code> 请求头或 <code>?since_ack=</code> 参数从断点补推遗漏消息。
          </div>
          <h4 class="api-sub">事件类型</h4>
          <table class="table api-table">
            <thead><tr><th style="width:140px">事件</th><th>说明</th></tr></thead>
            <tbody>
              <tr><td><code>message.new</code></td><td>新消息（data 为消息 JSON，含 ack_id）</td></tr>
              <tr><td><code>message.batch</code></td><td>批量消息</td></tr>
              <tr><td><code>message.revoke</code></td><td>撤回消息</td></tr>
            </tbody>
          </table>
          <h4 class="api-sub">调用示例</h4>
          <pre class="api-example">{`# curl 持续监听
curl -N "http://127.0.0.1:5032/api/v1/push/messages?access_token=<token>"

# Python 断点续传
import requests
last_id = 0
while True:
    try:
        with requests.get(
            "http://127.0.0.1:5032/api/v1/push/messages",
            headers={"Authorization": "Bearer <token>", "Last-Event-ID": str(last_id)},
            stream=True, timeout=None,
        ) as r:
            for line in r.iter_lines(decode_unicode=True):
                if line and line.startswith("id:"):
                    last_id = int(line[3:].strip())
    except requests.RequestException:
        continue  # 自动重连并从断点补推`}</pre>
        </div>

        <!-- ═══ 自动化管理中心 HTTP API（任务派发与执行回传） ═══ -->
        <div class="api-section">
          <h3 class="api-method">自动化管理中心 HTTP API</h3>
          <div class="api-desc">
            供智能体 / st_agent 领取任务、更新执行状态并回传结果的接口。
            消息命中规则后进入 <code>task_wechat_info</code>，状态机：
            <code>待处理 pending → 已派发 claimed → 处理中 processing → 待回复 to_reply → 已回复 replied</code>。
            鉴权方式与微信数据 API 一致（未配置 token 时免鉴权）。
          </div>

          <h4 class="api-sub">GET /api/v1/automation/tasks — 查询任务</h4>
          <div class="api-box">
            <span class="api-cmd">查询参数</span>
            <code class="api-code">agent_id（必填，目标智能体/Agent ID） · status（可选，按状态过滤）</code>
          </div>
          <pre class="api-example">{`# 查询派发给 agent_id=1 的全部待处理任务
curl "http://127.0.0.1:5032/api/v1/automation/tasks?agent_id=1&status=pending"

# 返回
{ "success": true, "count": 1, "items": [ { "id": 1, "content": "...", "status": "pending", ... } ] }`}</pre>

          <h4 class="api-sub">POST /api/v1/automation/tasks/claim — 领取任务</h4>
          <div class="api-desc">领取派发给自己的任务，状态 <code>pending → claimed</code>。任务不属于该 Agent 时返回 403。</div>
          <pre class="api-example">{`curl -X POST "http://127.0.0.1:5032/api/v1/automation/tasks/claim" -H "Content-Type: application/json" -d '{"task_id": 1, "agent_id": "1"}'

# 返回
{ "success": true, "id": 1, "status": "claimed" }`}</pre>

          <h4 class="api-sub">POST /api/v1/automation/tasks/start — 开始执行</h4>
          <div class="api-desc">标记任务开始执行，状态 <code>claimed → processing</code>。</div>
          <pre class="api-example">{`curl -X POST "http://127.0.0.1:5032/api/v1/automation/tasks/start" -H "Content-Type: application/json" -d '{"task_id": 1}'

# 返回
{ "success": true, "id": 1, "status": "processing" }`}</pre>

          <h4 class="api-sub">POST /api/v1/automation/tasks/complete — 提交结果</h4>
          <div class="api-desc">
            处理完成后按 <code>sender_username + timestamp + username</code> 唯一约束写回回复文本并更新状态
            （默认 <code>to_reply</code>，供回复机器人读取发送；也可传 <code>status</code> 直接标记）。
          </div>
          <pre class="api-example">{`curl -X POST "http://127.0.0.1:5032/api/v1/automation/tasks/complete" -H "Content-Type: application/json" -d '{"sender_username": "wxid_a1z2r51mzqlf22", "timestamp": 1785941000000000, "username": "45225247671@chatroom", "reply_text": "您好，您的新丰田预审已受理", "status": "to_reply"}'

# 返回
{ "success": true, "status": "to_reply" }`}</pre>

          <h4 class="api-sub">任务状态说明</h4>
          <table class="table api-table">
            <thead><tr><th style="width:120px">状态</th><th>含义</th><th style="width:220px">由谁变更</th></tr></thead>
            <tbody>
              <tr><td><code>pending</code></td><td>待处理（规则命中入库 / AI 决策失败待人工）</td><td>系统入库 / 人工重置</td></tr>
              <tr><td><code>claimed</code></td><td>已派发（智能体已领取）</td><td>智能体调用 claim</td></tr>
              <tr><td><code>processing</code></td><td>处理中（智能体正在执行）</td><td>智能体调用 start</td></tr>
              <tr><td><code>to_reply</code></td><td>待回复（已产出回复文本）</td><td>智能体调用 complete</td></tr>
              <tr><td><code>replied</code></td><td>已回复（回复机器人发送成功）</td><td>回复机器人 / complete 指定</td></tr>
              <tr><td><code>done</code> / <code>ignored</code></td><td>已完成 / 已忽略（人工）</td><td>界面人工操作</td></tr>
            </tbody>
          </table>
        </div>

        <div class="api-section">
          <h3 class="api-method">错误码</h3>
          <table class="table api-table">
            <thead><tr><th style="width:80px">HTTP</th><th style="width:190px">code</th><th>说明</th></tr></thead>
            <tbody>
              <tr><td>400</td><td><code>BAD_REQUEST</code></td><td>缺少必填参数或参数非法</td></tr>
              <tr><td>401</td><td><code>UNAUTHORIZED</code></td><td>缺少或无效的 access_token</td></tr>
              <tr><td>404</td><td><code>NOT_FOUND</code></td><td>会话/消息/图片不存在或解密失败</td></tr>
              <tr><td>500</td><td><code>CONFIG_NOT_FOUND</code></td><td>微信数据目录/密钥未配置</td></tr>
              <tr><td>500</td><td><code>INTERNAL_ERROR</code></td><td>查询或解密内部错误</td></tr>
              <tr><td>503</td><td><code>MONITOR_NOT_RUNNING</code></td><td>消息监控未运行，SSE 不可用</td></tr>
            </tbody>
          </table>
        </div>

        <!-- ═══ WebSocket 协议（Agent 控制） ═══ -->
        <div class="api-section">
          <h3 class="api-method">Agent 控制 WebSocket 协议</h3>
          <div class="api-desc">向 Agent 下发任务命令的控制通道（与上述数据 API 相互独立）。</div>
          <h4 class="api-sub">请求地址</h4>
          <div class="api-box">
            <span class="api-cmd">WebSocket</span>
            <code class="api-code">ws://127.0.0.1:9786</code>
          </div>
          <h4 class="api-sub">调用方式</h4>
          <div class="api-box">
            <span class="api-cmd">IPC 指令</span>
            <code class="api-code">{`invoke('send_command_to_agent', { args: { agentId, method, payload } })`}</code>
          </div>
          <h4 class="api-sub">请求示例</h4>
          <pre class="api-example">{`{
  "type": "command",
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": 1721589600000,
  "source": "st_control",
  "target": "st_agent",
  "method": "task.execute",
  "payload": { "targetAgentId": "d621f115-...", "task": { "command": "dir", "timeout": 30 } }
}`}</pre>
          <h4 class="api-sub">注意事项</h4>
          <ul class="api-notes">
            <li>Agent 必须在在线状态，否则下发会返回错误</li>
            <li>Agent 会立即回复 <code>received</code> 确认，不等待任务执行完成</li>
          </ul>
        </div>
      </div>
  </Modal>
{/if}

<style>
  .modal-hd {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 18px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in oklab, var(--popover) 88%, black 12%);
  }
  .modal-title { font-size: 16px; font-weight: 700; color: var(--foreground); flex: 1; }
  .modal-close {
    width: 28px;
    height: 28px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: none;
    border-radius: 7px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 14px;
    cursor: pointer;
  }
  .modal-close:hover { background: var(--muted); color: var(--foreground); }
  .modal-body { padding: 18px; overflow-y: auto; }

  .api-section { margin-bottom: 26px; }
  .api-method { font-size: 14px; font-weight: 700; color: var(--foreground); margin: 0 0 6px; }
  .api-endpoint-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .api-desc { font-size: 12px; color: var(--muted-foreground); line-height: 1.7; margin-bottom: 10px; }
  .api-sub { font-size: 12px; font-weight: 600; color: var(--muted-foreground); margin: 14px 0 6px; }
  .api-box {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: color-mix(in oklab, var(--card) 55%, black 45%);
    margin-bottom: 8px;
    flex-wrap: wrap;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .api-box:hover {
    border-color: color-mix(in oklab, var(--primary) 36%, var(--border));
    box-shadow: 0 0 0 1px color-mix(in oklab, var(--primary) 10%, transparent), 0 6px 20px -14px color-mix(in oklab, var(--primary) 55%, transparent);
  }
  .api-cmd { font-size: 11.5px; font-weight: 600; color: var(--primary); }
  .api-code {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--foreground);
    word-break: break-all;
  }
  .api-token-value { color: var(--primary); }
  .api-status { font-size: 11.5px; font-weight: 600; padding: 2px 8px; border-radius: 999px; }
  .api-status-on { background: color-mix(in oklab, #22c55e 16%, transparent); color: #4ade80; }
  .api-status-off { background: color-mix(in oklab, #ef4444 16%, transparent); color: #f87171; }
  .api-debug-btn {
    display: inline-flex; align-items: center; gap: 4px; white-space: nowrap;
    height: 26px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--muted);
    color: var(--muted-foreground);
    font-size: 11.5px;
    cursor: pointer;
  }
  .api-debug-btn:hover:not(:disabled) { color: var(--foreground); }
  .api-debug-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .api-example {
    margin: 8px 0;
    padding: 12px 14px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    background: color-mix(in oklab, black 45%, var(--card));
    color: color-mix(in oklab, var(--foreground) 82%, var(--primary));
    font-family: var(--font-mono);
    font-size: 12px;
    line-height: 1.65;
    overflow-x: auto;
    white-space: pre;
  }
  .api-notes { margin: 8px 0; padding-left: 18px; font-size: 12px; color: var(--muted-foreground); line-height: 1.8; }
  .api-notes code { font-family: var(--font-mono); color: var(--primary); }

  .table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .table th {
    text-align: left;
    padding: 8px 10px;
    color: var(--muted-foreground);
    font-weight: 600;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  .table td {
    padding: 8px 10px;
    border-bottom: 1px solid color-mix(in oklab, var(--border) 55%, transparent);
    color: var(--foreground);
    vertical-align: top;
  }
  .table td code { font-family: var(--font-mono); color: var(--primary); font-size: 11.5px; }
</style>

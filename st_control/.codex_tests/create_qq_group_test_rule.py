import sqlite3, json

DB = r"E:\ST\st_control\data\control.db"
GROUP_OPENID = "14EBF72484FCACE8C3CF280FC6EF7A29"
PROVIDER_ID = "b8357f93-3cbf-4020-8739-99e18d32bb78"

conn = sqlite3.connect(DB)
conn.execute("PRAGMA busy_timeout=3000")

conditions = [
    {"field": "sender", "op": "equals", "value": GROUP_OPENID},
    {"field": "content", "op": "contains", "value": "测试"},
]
prompt = (
    '你是 QQ 群测试应答机器人。忽略消息内容与格式，只输出下面这个固定 JSON，'
    '不要任何多余文字：{"task":"测试应答","fields":{"reply":"✅ ST 系统自动回复测试成功'
    '（QQ 群被动回复通道）"}}'
)

existing = conn.execute(
    "SELECT id FROM automation_rules WHERE name=?", ("QQ群测试应答",)
).fetchall()
if existing:
    conn.execute("DELETE FROM automation_rules WHERE name=?", ("QQ群测试应答",))
    print("已删除旧测试规则:", existing)

conn.execute(
    """INSERT INTO automation_rules
       (name, enabled, priority, conditions_json, analyze_fields_json,
        prompt_override, provider_id, model, dispatch_mode, target_type, target_id, role_id)
       VALUES (?, 1, 100, ?, '[]', ?, ?, '', 'fixed', 'agent', '', '')""",
    ("QQ群测试应答", json.dumps(conditions, ensure_ascii=False), prompt, PROVIDER_ID),
)
conn.commit()

rows = conn.execute(
    "SELECT id, name, enabled, priority, conditions_json, provider_id FROM automation_rules"
).fetchall()
for r in rows:
    print(r)

conn.close()
print("规则创建完成")

import sqlite3
c = sqlite3.connect(r"E:\ST\st_control\data\control.db")
c.execute("PRAGMA busy_timeout=3000")
rows = c.execute("SELECT id,kind,openid,display,last_content,last_seen_at FROM qqbot_contacts ORDER BY id DESC LIMIT 10").fetchall()
print("contacts:", rows)

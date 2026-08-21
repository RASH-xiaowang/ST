# 找最新（create_time 最大）的含原图字段图片消息
import sqlite3, sys, io
sys.stdout.reconfigure(encoding='utf-8')
import zstandard as zstd

DB = r"E:\ST\st_control\data\wechat\decrypted\message\message_0.db"
conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
conn.execute("PRAGMA busy_timeout=3000")
tables = [r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'Msg_%'").fetchall()]

dctx = zstd.ZstdDecompressor()
best = []
for t in tables:
    try:
        cols = [r[1] for r in conn.execute(f'PRAGMA table_info("{t}")').fetchall()]
        has_ct = 'create_time' in cols
        order = 'ORDER BY create_time DESC' if has_ct else ''
        rows = conn.execute(
            f'SELECT local_id, create_time, message_content, compress_content FROM "{t}" WHERE local_type = 3 OR local_type % 4294967296 = 3 {order} LIMIT 300'
        ).fetchall()
    except Exception:
        continue
    for local_id, ct, content, comp in rows:
        raw = content or comp
        if not isinstance(raw, (bytes, bytearray)):
            continue
        try:
            xml = dctx.decompress(bytes(raw), max_output_size=2_000_000).decode('utf-8', 'replace')
        except Exception:
            continue
        if 'cdnbigimgurl' not in xml:
            continue
        def attr(name):
            try:
                i = xml.index(f'{name}="')
                rest = xml[i + len(name) + 2:]
                return rest[:rest.index('"')]
            except Exception:
                return ''
        file_id = attr('cdnbigimgurl'); aes = attr('aeskey'); hd = attr('hdlength'); md5 = attr('md5')
        if not (file_id and aes and hd):
            continue
        best.append((ct or 0, t, local_id, int(hd), md5))
best.sort(reverse=True)
conn.close()
for b in best[:8]:
    print(b)

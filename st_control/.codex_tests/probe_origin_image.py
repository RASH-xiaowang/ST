# 扫描解密消息库：找一张含 cdnbigimgurl + aeskey + hdlength 的真实图片消息
# （模拟 origin_ilink/extract.rs 的查找逻辑，用于端到端原图下载验证）
import sqlite3, sys, io
sys.stdout.reconfigure(encoding='utf-8')

try:
    import zstandard as zstd
except ImportError:
    print('NO_ZSTANDARD')
    sys.exit(0)

DB = r"E:\ST\st_control\data\wechat\decrypted\message\message_0.db"
conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
conn.execute("PRAGMA busy_timeout=3000")
tables = [r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'Msg_%'").fetchall()]
print(f"tables: {len(tables)}")

dctx = zstd.ZstdDecompressor()
found = []
for t in tables:
    try:
        rows = conn.execute(
            f'SELECT local_id, message_content, compress_content FROM "{t}" WHERE local_type = 3 OR local_type % 4294967296 = 3 LIMIT 2000'
        ).fetchall()
    except Exception:
        continue
    for local_id, content, comp in rows:
        raw = content or comp
        if not isinstance(raw, (bytes, bytearray)):
            continue
        try:
            decoded = dctx.decompress(bytes(raw), max_output_size=2_000_000)
        except Exception:
            continue
        xml = decoded.decode('utf-8', 'replace')
        if 'cdnbigimgurl' not in xml:
            continue
        # 提取关键字段
        def attr(name):
            try:
                i = xml.index(f'{name}="')
                rest = xml[i + len(name) + 2:]
                j = rest.index('"')
                return rest[:j]
            except Exception:
                return ''
        file_id = attr('cdnbigimgurl')
        aes = attr('aeskey')
        hd = attr('hdlength')
        md5 = attr('md5')
        if not (file_id and aes and hd):
            continue
        found.append({
            'table': t, 'local_id': local_id,
            'file_id': file_id[:48], 'hdlength': hd, 'md5': md5,
        })
        if len(found) >= 5:
            break
    if len(found) >= 5:
        break

conn.close()
for f in found:
    print(f)
print(f"TOTAL_FOUND={len(found)}")

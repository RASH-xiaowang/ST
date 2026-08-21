import sqlite3, sys, json, io
sys.stdout.reconfigure(encoding='utf-8')
import zstandard as zstd

TABLE = "Msg_5a8f5ec9ef550505c625c39c3e6d4c9b"
LOCAL_ID = 2966
SB = r"E:\ST\st_control\data\st_result\origin_ilink"
DB = r"E:\ST\st_control\data\wechat\decrypted\message\message_0.db"

conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
row = conn.execute(
    f'SELECT message_content, compress_content FROM "{TABLE}" WHERE local_id = {LOCAL_ID} AND (local_type = 3 OR local_type % 4294967296 = 3) LIMIT 1'
).fetchone()
conn.close()
raw = row[0] or row[1]
xml = zstd.ZstdDecompressor().decompress(bytes(raw), max_output_size=2_000_000).decode("utf-8", "replace")
start = xml.find("<msg")
end = xml.find("</msg>") + len("</msg>")
msg_xml = xml[start:end]
assert "cdnbigimgurl" in msg_xml and "aeskey" in msg_xml, "缺少原图字段"
doc = {"data": [{"source_native_id": f"{TABLE}:{LOCAL_ID}", "text": msg_xml}]}
json.dump(doc, open(SB + r"\message.json", "w", encoding="utf-8"), ensure_ascii=False)
print("已写入 message.json，XML 长度", len(msg_xml))

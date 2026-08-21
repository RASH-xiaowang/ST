# 为端到端原图验证准备隔离沙箱 + 消息 JSON（复刻 origin_ilink/sandbox.rs 与 extract.rs 的逻辑）
import os, shutil, sys, sqlite3, json, io
sys.stdout.reconfigure(encoding='utf-8')
import zstandard as zstd

SB = r"E:\ST\st_control\data\st_result\origin_ilink"
APPDATA = os.environ["APPDATA"]
ILINK = os.path.join(APPDATA, "Tencent", "xwechat", "ilink")

# 1) 会话复制
for sub in ["wechat", "kvcomm", "netbridge/cdn"]:
    os.makedirs(os.path.join(SB, sub), exist_ok=True)
shutil.copyfile(os.path.join(ILINK, "wechat", "cloud_account.txt"), os.path.join(SB, "wechat", "cloud_account.txt"))
for f in ["config.ini", "new_strategy_file_kv"]:
    src = os.path.join(ILINK, "kvcomm", f)
    if os.path.isfile(src):
        shutil.copyfile(src, os.path.join(SB, "kvcomm", f))
for f in ["cdninfo_new.cache", "cdnmisc.cfg"]:
    src = os.path.join(APPDATA, "Tencent", "xwechat", "net", "cdncomm", f)
    if os.path.isfile(src):
        shutil.copyfile(src, os.path.join(SB, "netbridge", "cdn", f))

# 2) ilink 启动配置（与 sandbox.rs 相同编码：字段1=root, 字段2=0, 字段6=client_version）
def varint(v):
    out = bytearray()
    while v >= 0x80:
        out.append((v & 0x7F) | 0x80)
        v >>= 7
    out.append(v)
    return bytes(out)

client_version = 4065598490
ini = os.path.join(ILINK, "kvcomm", "config.ini")
if os.path.isfile(ini):
    for line in open(ini, encoding="utf-8", errors="replace"):
        s = line.strip()
        if s.startswith("kv_clientversion="):
            try:
                client_version = int(s.split("=", 1)[1])
            except ValueError:
                pass
root = SB.replace("/", "\\")
data = bytearray()
data += b"\x0a" + varint(len(root.encode("utf-8"))) + root.encode("utf-8")
data += b"\x10\x00"
data += b"\x30" + varint(client_version)
open(os.path.join(SB, "ilink-start-config.bin"), "wb").write(bytes(data))
print("client_version =", client_version)

# 3) 提取测试消息（Msg_989f61b2a549ee48e1eb49156dd4ff66 local_id=9）
DB = r"E:\ST\st_control\data\wechat\decrypted\message\message_0.db"
TABLE = "Msg_989f61b2a549ee48e1eb49156dd4ff66"
conn = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
row = conn.execute(
    f'SELECT message_content, compress_content FROM "{TABLE}" WHERE local_id = 9 AND (local_type = 3 OR local_type % 4294967296 = 3) LIMIT 1'
).fetchone()
conn.close()
raw = row[0] or row[1]
decoded = zstd.ZstdDecompressor().decompress(bytes(raw), max_output_size=2_000_000)
xml = decoded.decode("utf-8", "replace")
start = xml.find("<msg")
end = xml.find("</msg>") + len("</msg>")
msg_xml = xml[start:end]
doc = {"data": [{"source_native_id": f"Msg_{TABLE}:9", "text": msg_xml}]}
open(os.path.join(SB, "message.json"), "w", encoding="utf-8").write(json.dumps(doc, ensure_ascii=False))
print("XML 长度:", len(msg_xml))
print("含 cdnbigimgurl:", "cdnbigimgurl" in msg_xml, "| 含 aeskey:", "aeskey" in msg_xml)
print("准备完成")

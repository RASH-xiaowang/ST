# ============================================================
# 知识库删除后恢复工具（一次性运维脚本）
# 背景：知识库被删除后，documents 等元数据表已清空，但
#   - file_objects（去重后的原始上传文件）在主库中仍然完好
#   - 被删文档的标题字符串残留在 SQLite 空闲页中（部分字节被覆盖）
# 本脚本：
#   1. 从主库把全部 file_objects 原文件导出到 export 目录（字节级一致）
#   2. 从空闲页尽力还原「标题 → hash」映射，用于给导出文件命名
#   3. 输出 manifest.csv（hash/ext/size）与 recovered_titles.txt
# 用法：python scripts/recover_kb_export.py <主库路径> <输出目录>
# 注意：请先复制一份主库再执行（本脚本只读源库，immutable 打开）。
# ============================================================
import sqlite3, os, re, sys, csv

def main():
    if len(sys.argv) < 3:
        print('用法: python recover_kb_export.py <knowledge_base.db> <输出目录>')
        return
    db_path = sys.argv[1]
    out_dir = sys.argv[2]
    os.makedirs(out_dir, exist_ok=True)
    files_dir = os.path.join(out_dir, 'exported_files')
    os.makedirs(files_dir, exist_ok=True)

    # 1) 导出 file_objects（只读打开，避免写源库）
    conn = sqlite3.connect('file:%s?immutable=1' % db_path.replace('\\', '/'), uri=True)
    conn.row_factory = sqlite3.Row
    rows = conn.execute('SELECT id, hash, ext, size, blob_data FROM file_objects ORDER BY id').fetchall()
    print('file_objects 数量:', len(rows))
    manifest = []
    for r in rows:
        ext = r['ext'] or 'bin'
        fname = '%04d-%s.%s' % (r['id'], r['hash'], ext)
        with open(os.path.join(files_dir, fname), 'wb') as f:
            f.write(r['blob_data'])
        manifest.append({'id': r['id'], 'hash': r['hash'], 'ext': ext, 'size': r['size']})
    with open(os.path.join(out_dir, 'manifest.csv'), 'w', newline='', encoding='utf-8') as f:
        w = csv.DictWriter(f, fieldnames=['id', 'hash', 'ext', 'size'])
        w.writeheader()
        w.writerows(manifest)
    print('已导出 %d 个原始文件 → %s' % (len(rows), files_dir))

    # 2) 尽力还原「标题 → hash」（标题与 hash 同处一条被删记录，邻近出现）
    data = open(db_path, 'rb').read()
    hashes = set(r['hash'] for r in rows)
    title_re = re.compile(rb'[\x20-\x7e\x80-\xff]{3,120}?\.md')
    hash_re = re.compile(rb'[0-9a-f]{32}')
    pairs = {}
    for m in title_re.finditer(data):
        t = m.group(0).decode('utf-8', 'replace')
        if '\x00' in t:
            continue
        seg = data[m.start(): m.start() + 320]
        for hm in hash_re.finditer(seg):
            h = hm.group(0).decode()
            if h in hashes:
                pairs.setdefault(t, h)
                break
    print('还原「标题 → hash」:', len(pairs))
    with open(os.path.join(out_dir, 'recovered_titles.txt'), 'w', encoding='utf-8') as f:
        for t, h in sorted(pairs.items()):
            f.write('%s\t%s\n' % (h, t))

    # 3) 未配对标题参考列表（可能含少量噪声）
    all_titles = set()
    for m in title_re.finditer(data):
        t = m.group(0).decode('utf-8', 'replace')
        if '\x00' in t:
            continue
        all_titles.add(t)
    with open(os.path.join(out_dir, 'title_candidates.txt'), 'w', encoding='utf-8') as f:
        for t in sorted(all_titles):
            f.write(t + '\n')
    print('标题候选（参考）:', len(all_titles), '→', os.path.join(out_dir, 'title_candidates.txt'))

if __name__ == '__main__':
    main()

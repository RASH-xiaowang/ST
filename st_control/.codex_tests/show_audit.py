import json, io, sys
sys.stdout.reconfigure(encoding='utf-8')
d = json.load(io.open(r"E:\ST\st_control\data\ui-audit\audit.json", encoding='utf-8'))
for r in d:
    print('==', r['tab'], '==')
    print('   ', ' | '.join(r['buttons'][:26]))

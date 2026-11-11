"""fx's benchmark inventory (benchmarks/file_index_bench.zig:24-125@481104fd),
made a tree: candidate i is a file when i is even, a directory when odd.
usage: python3 -I gen_tree.py <out dir> [count]"""
import os, sys
sub = ["apps","packages","libs","services","tools","plugins","extensions","modules","infra","vendor","web","mobile","workers","functions"]
areas = ["core","utils","components","hooks","api","db","auth","ui","server","client","cli","tests","docs","internal","shared","config"]
subareas = ["common","helpers","types","schemas","models","views","routes","handlers","services","stores","context","providers","layouts","pages","forms","fields"]
names = ["index","main","app","Button","Input","Modal","helper","util","types","schema","model","view","route","handler","service","store","hook","context","provider","layout","page","widget","icon","form","field","label","table","list","card","dialog","menu","nav"]
exts = [".ts",".tsx",".js",".jsx",".zig",".py",".rs",".go",".md",".json",".yaml"]
def path(i):
    fixed = {0: "docs/Ärger-file.txt", 1: "docs/Kelvin", 2: "src/core/workspace/file_index.zig", 3: "apps/web/api-route-handler"}
    if i in fixed: return fixed[i]
    s = sub[i % len(sub)] + "/" + areas[(i // len(sub)) % len(areas)] + "/"
    roll = (i // 7) % 3
    if roll >= 1: s += subareas[(i // 17) % len(subareas)] + "/"
    if roll >= 2: s += subareas[(i // 23) % len(subareas)] + "/"
    s += names[(i // 31) % len(names)] + str(i)
    if i % 2 == 0: s += exts[(i // 13) % len(exts)]
    return s
out = sys.argv[1]; n = int(sys.argv[2]) if len(sys.argv) > 2 else 100000
for i in range(n):
    p = os.path.join(out, path(i))
    if i % 2 == 0:
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as f: f.write("x")
    else:
        os.makedirs(p, exist_ok=True)

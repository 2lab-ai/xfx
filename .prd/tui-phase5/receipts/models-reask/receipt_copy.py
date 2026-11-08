#!/usr/bin/env python3
"""Copies files into the receipt, redacted by the repo's own redact.py.
usage: receipt_copy.py <repo> <scratch> <dest-dir> <src>... ; each src lands as <dest-dir>/<basename>"""
import os, sys
repo, scratch, dest = sys.argv[1], sys.argv[2], sys.argv[3]
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
import redact
# The run named the short TMPDIR and the pinned fx binary as literal local
# paths here; this copy reads them from XFX_TMP and FX_BIN instead.
known = {scratch: "<scratch>", repo: "<repo>", os.environ["XFX_TMP"]: "<tmp>",
         os.environ["FX_BIN"]: "<fx-bin>"}
r = redact.Redactor(known)
os.makedirs(dest, exist_ok=True)
for src in sys.argv[4:]:
    text = open(src, encoding="utf-8", errors="replace").read()
    out = os.path.join(dest, os.path.basename(src))
    open(out, "w").write(r.public(text))
    print(out)

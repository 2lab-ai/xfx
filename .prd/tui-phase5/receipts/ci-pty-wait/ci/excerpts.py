#!/usr/bin/env python3
"""Each failing test's own message from one job log, timestamps dropped, runner
paths scrubbed, escapes shown as visible marks, a long terminal dump cut to its
head and tail.  usage: excerpts.py <raw-log> <out-file>"""
import re, sys

text = open(sys.argv[1], "rb").read().decode("utf-8", "replace")
text = re.sub(r"(?m)^\d{4}-\d\d-\d\dT[\d:.]+Z ", "", text)
out = []
for m in re.finditer(r"(?m)^---- (\S+) stdout ----\n", text):
    end = text.find("stack backtrace:", m.end())
    body = text[m.start(): end if end >= 0 else m.end() + 4000]
    # the runner's home and a macOS per-user temporary directory, each spelled
    # in two parts so that this script holds neither
    body = re.sub(r"/Users/" + r"runner\b", "<runner>", body)
    body = re.sub(r"(/private)?/var/" + "folders/[^\\s\"']*", "<tmp>", body)
    body = body.replace("\x1b", "␛").replace("\x07", "␇").replace("\\u{1b}", "␛").replace("\\u{7}", "␇")
    if len(body) > 6000:
        body = body[:1500] + f"\n[... {len(body) - 5500} characters cut ...]\n" + body[-4000:]
    out.append(body.rstrip() + "\n")
open(sys.argv[2], "w").write("\n".join(out))

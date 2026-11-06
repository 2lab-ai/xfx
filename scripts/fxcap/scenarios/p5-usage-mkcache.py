# Writes the usage-cache fixtures the p5-usage scenarios seed, in fx v0.0.16's
# schema 2 (vercel-labs/fx@481104fd src/usage/cache.zig:172-195), which xfx's
# cache reads as it is: for the capture harness's fake key, on the UTC day of
# the pinned clock.
#
# usage: python3 -I p5-usage-mkcache.py <out-dir> [<clock ms>]
#
# The clock is the one each scenario pins with `clock` (default 1791819000000,
# 2026-10-12 15:30 UTC, fx's own e2e time, tests/e2e/usage-fakes.ts:10). Every
# stamp is that clock minus five minutes, so the files do not depend on the day
# they are written and a capture reads `5m ago` on any day. The identity is
# sha256("fx-usage-cache-v1\0api_key\0fake-key-not-real\0") (cache.zig:22-35):
# the scope, the key itself and no team -- the hash is the file's name, and the
# key is never in it.
import hashlib
import json
import sys

out = sys.argv[1]
now = int(sys.argv[2]) if len(sys.argv) > 2 else 1791819000000
day = now // 86400000
at = now - 5 * 60000
ident = hashlib.sha256(b"fx-usage-cache-v1\x00api_key\x00fake-key-not-real\x00").hexdigest()


def t(req, inp, outp, cr, cw, rs, cost, hit=0.0, fail=0.0, ttft=None, tps=None):
    return {"request_count": req, "input_tokens": inp, "output_tokens": outp, "cache_read_tokens": cr,
            "cache_write_tokens": cw, "reasoning_tokens": rs, "total_cost": cost,
            "cache_hit_requests": hit / 100 * req, "failed_requests": fail / 100 * req,
            "ttft_weighted": (ttft or 0) * req, "ttft_requests": req if ttft else 0,
            "tps_weighted": (tps or 0) * req, "tps_requests": req if tps else 0}


today = [
    {"model": "anthropic/claude-opus-5", "totals": t(12, 52000, 3100, 30000, 2000, 900, 0.4210, 75, 0, 1840, 61)},
    {"model": "fake/model-a", "totals": t(3, 12000, 800, 4000, 0, 0, 0.0125, 33.3, 0, 920, 88)},
]
week = [
    {"model": "anthropic/claude-opus-5", "totals": t(140, 2_400_000, 91_000, 1_800_000, 120_000, 22_000, 18.75, 81, 1.4, 2100, 58)},
    {"model": "openai/gpt-5.6", "totals": t(64, 880_000, 40_000, 300_000, 0, 31_000, 4.10, 40, 0, 1300, 95)},
    {"model": "fake/model-a", "totals": t(20, 90_000, 6_000, 10_000, 0, 0, 0.09, 10, 5, 800, 120)},
]
month = week + [
    {"model": "xai/grok-5", "totals": t(9, 40_000, 3_000, 0, 0, 1_000, 0.31, 0, 11.1, 600, 140)},
]
entry = {"schema_version": 2, "identity": ident,
         "today": {"day": day, "at_ms": at, "models": today},
         "finished": {"day": day, "at_ms": at, "days_7": week, "month": month}}
# A budget, which fx reads only for an fx login with a team (usage.zig:867-875):
# here only because the file holds one.
budget = dict(entry, budget={"at_ms": at, "value": {"limit": 316.0, "spent": 65.97, "period": "monthly"}})
# Twelve models this month: more than the ten-row cap, for the "N more" row and
# the model window (usage_presentation.zig:19, :718-748).
many = [{"model": "vendor-%02d/model-with-a-long-name-%02d" % (i, i),
         "totals": t(10 + i, 100_000 * (12 - i), 5_000, 0, 0, 0, 1.0 * (12 - i))} for i in range(12)]
manyentry = dict(entry, finished={"day": day, "at_ms": at, "days_7": many[:4], "month": many})
for name, value in (("p5-usage-cache.json", entry), ("p5-usage-cache-budget.json", budget),
                    ("p5-usage-cache-many.json", manyentry)):
    with open("%s/%s" % (out, name), "w") as f:
        json.dump(value, f)
print(ident, day)

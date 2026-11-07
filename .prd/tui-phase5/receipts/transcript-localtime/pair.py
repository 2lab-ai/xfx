"""One fx/xfx snap pair, compared cell by cell (text and attributes) after the
parity ledger's normalizations -- row 1's run clock (`network-ms`), each
header's clock (`transcript-timestamp`, which keeps the zone).

usage: python3 -I -B pair.py <repo> <fx.ansi> <xfx.ansi> [--clock <zone>]

With `--clock`, the masked clocks are checked too, against the moment each
snap was written -- its file time, so only on the run's own copies, right after
the capture (`clocks.txt`): each header `YYYY-MM-DD HH:MM:SS.mmm <abbr> · <Kind>`
must be <zone>'s (`Asia/Seoul`, `Asia/Kolkata`) reading of a moment no more
than 120 s before the snap, under the abbreviation <zone> gives it. Prints the
differing rows; exit 0 when there are none (and every clock checks).
"""

import datetime
import os
import re
import sys
import zoneinfo

repo = os.path.realpath(sys.argv[1])
sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))

import cells  # noqa: E402
import ledger as ledgermod  # noqa: E402

paths = sys.argv[2:4]
zone = zoneinfo.ZoneInfo(sys.argv[5]) if sys.argv[4:5] == ["--clock"] else None
led = ledgermod.load(os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"), repo)
HEADER = re.compile(r"^  (\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}) ([A-Za-z0-9+-]{1,10}) · \w+$")

bad = 0
sides = []
for side, path in zip(("fx", "xfx"), paths):
    which = "fx" if os.path.basename(path).startswith("fx-") else "xfx"
    rows = cells.parse(open(path, encoding="utf-8").read())
    sides.append([cells.normalize_row(r, ledgermod.rules_for(led, which)) for r in rows])
    if zone is None:
        continue
    written = datetime.datetime.fromtimestamp(os.stat(path).st_mtime, datetime.timezone.utc)
    headers = 0
    for r in rows:
        m = HEADER.match(cells.text_of(r))
        if not m:
            continue
        headers += 1
        naive = datetime.datetime.strptime(m.group(1), "%Y-%m-%d %H:%M:%S.%f")
        # The reading names its zone; fold picks the reading that zone gives
        # that abbreviation (the hour a change repeats).
        moment = None
        for fold in (0, 1):
            local = naive.replace(tzinfo=zone, fold=fold)
            if local.tzname() == m.group(2) or local.strftime("%z") == m.group(2):
                moment = local
        lag = (written - moment).total_seconds() if moment else None
        ok = lag is not None and 0 <= lag <= 120
        bad += not ok
        print("  %-3s clock %s %s -> %s" % (which, m.group(1), m.group(2), ("%.1fs before the snap" % lag) if ok else "WRONG"))
    if headers == 0:
        bad += 1
        print("  %-3s no entry header" % which)

a, b = sides
differ = [i + 1 for i in range(max(len(a), len(b))) if (a[i] if i < len(a) else []) != (b[i] if i < len(b) else [])]
for row in differ:
    for side, rows in (("a", a), ("b", b)):
        print("  row %2d %s: %s" % (row, side, cells.text_of(rows[row - 1]) if row <= len(rows) else "(none)"))
print("positional: %d of %d rows differ%s" % (len(differ), max(len(a), len(b)), (" " + str(differ)) if differ else ""))
sys.exit(1 if differ or bad else 0)

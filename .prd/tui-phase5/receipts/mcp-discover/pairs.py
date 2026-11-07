"""The receipt's pairs, compared as the sweep compares them: cell by cell,
row 1 (the banner) left out, after the ledger's normalizations
(scripts/fxcap/parity-ledger.toml -- each product's name, the run's own
directory and clock, the HTTP fixture's port).

usage: python3 -I pairs.py <fx.ansi> <xfx.ansi>...   (fx, then xfx, repeated)

Prints, per pair, the rows that differ raw and after normalization.
"""
import os
import sys

here = os.path.dirname(os.path.realpath(__file__))
repo = os.path.realpath(os.path.join(here, "..", "..", "..", ".."))
sys.path.insert(0, os.path.join(repo, "scripts", "fxcap"))
import cells  # noqa: E402
import ledger  # noqa: E402

led = ledger.load(os.path.join(repo, "scripts", "fxcap", "parity-ledger.toml"), repo)
args = sys.argv[1:]
for fx_path, xfx_path in zip(args[0::2], args[1::2]):
    fx, xfx = cells.rows(fx_path, True)[1:], cells.rows(xfx_path, False)[1:]
    raw = cells.differing(fx, xfx)
    norm = cells.differing(
        cells.normalize(fx, ledger.rules_for(led, "fx")),
        cells.normalize(xfx, ledger.rules_for(led, "xfx")),
    )
    name = os.path.basename(xfx_path)[len("xfx-"):-len(".ansi")]
    print(f"{name}: raw {len(raw)} rows, normalized {len(norm)} rows")
    for i, a, b, _, _ in norm:
        print(f"    row {i + 2}: fx  {a!r}")
        print(f"    row {i + 2}: xfx {b!r}")

"""The parity sweep in CI, per pull request: the scenarios the change touches
plus a fixed core set, on fx and on two xfx builds -- the merge base's and the
head's -- and a failure for every snap pair the base kept identical to fx and
the head no longer does.

usage: ci-sweep.sh --base <rev> --fx <bin> --pin <name> --xfx <head bin>
                   --base-xfx <base bin> --out <dir>
                   [--jobs N] [--tmp <dir>] [--core <file>] [--retries 3]
       ci-sweep.sh --select --base <rev>     the selection alone, nothing runs
       ci-sweep.sh --full --fx <bin> --pin <name> --xfx <bin> --out <dir>
                                             every scenario, one sweep.sh run,
                                             reported and never failed

--pin names a [[pin]] of the ledger; its sha256 is what sweep.sh checks the
fx binary against, before and after every run.

What runs is one list of scenarios:

  core       every name in ci-core.txt (a missing one stops the run: the list
             is edited in the change that removes a scenario, where review
             sees it)
  changed    every scenario whose .cap the change adds or edits between
             --base and HEAD, and every .cap that names a file under
             scenarios/ the change adds or edits (a turns file, a seeded
             profile, a workspace directory)
  receipts   every scenario a receipt the change adds or edits names, in a
             receipt's own spelling of one: `scenarios/<name>.cap`,
             `<name>.cap`, or a capture's file name `fx-<name>-<rows>x<cols>-`
             / `xfx-<name>-<rows>x<cols>-` (a bare word is not a name: `copy`
             and `alias` are scenarios too)

Each runs through sweep.sh as it is, with HEAD's harness, scenarios and
ledger, so the one thing that differs between the two comparisons is the xfx
binary. The pass verdicts are the ledger's: identical and
normalized-identical.

  1. The head: one sweep of the list with --xfx.
  2. A pair the head does not pass, that the base also snaps (its scenario
     and label exist in --base's .cap), and that a `nondeterminism` cause of
     the ledger covers -- a row of its diff, either side's text -- is known to
     vary: its scenario runs again, up to --retries times, and the pair passes
     if any run passes it. No other pair is run again.
  3. The base: one sweep, with --base-xfx, of the scenarios that still hold
     a pair the base snaps and the head does not pass. None, and nothing
     runs. Such a pair the base fails and a `nondeterminism` cause covers
     gets the head's re-runs, up to --retries, so a base run that happened to
     vary cannot turn a regression into a pair neither build passes.
  4. A regression is a pair the base passes and the head does not. A pair the
     base does not snap is new: reported, never failed. A pair neither passes
     is reported.

Exit 0 with no regression, 1 with one, 2 when the run itself could not be
made (a refused sweep, a missing core scenario, a pin the ledger does not
name). --full exits 0 whatever the scoreboard says, and 2 only when the sweep
could not be made: it is the measured number at an epic's preview, not a gate
(`.prd/07-fx-parity-program.md` puts the gate on the ship).

--out gets report.md (redacted; also appended to $GITHUB_STEP_SUMMARY) and
result.json, and each sweep's own output under head/, head-retry-<n>/ and
base/, base-retry-<n>/ (--full: full/).
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.realpath(__file__))
# No __pycache__ beside the scripts: the checkout holds only what is committed.
sys.dont_write_bytecode = True
sys.path.insert(0, HERE)

import capfile  # noqa: E402
import ledger as ledgermod  # noqa: E402
from redact import Redactor  # noqa: E402

REPO = os.path.realpath(os.path.join(HERE, "..", ".."))
SWEEP = os.path.join(HERE, "sweep.sh")
PASS = ("identical", "normalized-identical")
RECEIPT_PATH = re.compile(r"^\.prd/.*receipts/")
RECEIPT_NAME = re.compile(
    r"(?:^|[\s/`'\"(\[])([a-z0-9][a-z0-9-]*)\.cap\b"  # scenarios/<name>.cap, <name>.cap
    r"|(?<![\w-])x?fx-([a-z0-9][a-z0-9-]*?)-\d+x\d+-"  # a capture's file name
)
RECEIPT_MAX_BYTES = 1 << 20


class Refusal(Exception):
    pass


def git(repo, *args):
    r = subprocess.run(["git", "-C", repo] + list(args), capture_output=True)
    if r.returncode != 0:
        raise Refusal("git %s: %s" % (" ".join(args), r.stderr.decode(errors="replace").strip()))
    return r.stdout


class Tree:
    """The checkout the scenarios directory is in, and where in it they are."""

    def __init__(self, scenarios):
        self.scenarios = os.path.realpath(scenarios)
        self.repo = os.path.realpath(git(self.scenarios, "rev-parse", "--show-toplevel").decode().strip())
        self.rel = os.path.relpath(self.scenarios, self.repo)

    def git(self, *args):
        return git(self.repo, *args)


# --- the selection ----------------------------------------------------------


def read_core(path):
    """ci-core.txt: one scenario per line, `# ...` comments."""
    out = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            name = line.split("#", 1)[0].strip()
            if name:
                out.append(name)
    return out


def reading_index(units):
    """Every name a scenario goes by -- its unit's, and each binary's file
    (`compact-inturn-fx` is the unit `compact-inturn`) -- to its unit."""
    index = {}
    for u in units:
        for name in (u.name, u.fx.name, u.xfx.name):
            index[name] = u.name
    return index


def tokens(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return set(f.read().split())


def changed_paths(tree, base):
    """The files HEAD adds, edits or deletes against --base (a rename is a
    delete and an add)."""
    raw = tree.git("diff", "--name-only", "--no-renames", "-z", base, "HEAD")
    return [p for p in raw.decode("utf-8", errors="replace").split("\0") if p]


def select(tree, base, core):
    """{unit: [why]}, in the order core, changed, receipts; and the scenario
    names the change removed."""
    scenarios = tree.scenarios
    units = capfile.units(scenarios)
    index = reading_index(units)
    chosen, removed = {}, []

    def add(unit, why):
        chosen.setdefault(unit, [])
        if why not in chosen[unit]:
            chosen[unit].append(why)

    missing = [n for n in core if n not in index]
    if missing:
        raise Refusal("ci-core.txt names %s, which scenarios/ does not hold" % ", ".join(missing))
    for name in core:
        add(index[name], "core")

    paths = changed_paths(tree, base)
    caps = {f[: -len(".cap")]: tokens(os.path.join(scenarios, f)) for f in os.listdir(scenarios) if f.endswith(".cap")}
    for p in paths:
        if not p.startswith(tree.rel + "/"):
            continue
        rel = p[len(tree.rel) + 1 :]
        if rel.endswith(".cap") and "/" not in rel:
            name = rel[: -len(".cap")]
            if name in index:
                add(index[name], "changed")
            else:
                removed.append(name)
            continue
        for name, words in sorted(caps.items()):
            if any(rel == t or rel.startswith(t.rstrip("/") + "/") for t in words):
                add(index[name], "uses " + rel)

    for p in paths:
        if not RECEIPT_PATH.match(p):
            continue
        found = names_in(p, os.path.join(tree.repo, p))
        for name in sorted(found):
            if name in index:
                add(index[name], "receipt")
    return chosen, sorted(set(removed))


def names_in(rel, path):
    """The scenario names a receipt spells: in its path and, for a text file
    that is still there, its contents."""
    texts = [rel]
    if os.path.isfile(path) and os.path.getsize(path) <= RECEIPT_MAX_BYTES:
        with open(path, encoding="utf-8", errors="replace") as f:
            texts.append(f.read())
    found = set()
    for text in texts:
        for m in RECEIPT_NAME.finditer(text):
            found.add(m.group(1) or m.group(2))
    return found


def base_labels(tree, base, unit):
    """The labels --base's .cap file(s) for this unit snap, or None when the
    base has no such scenario: each binary's file, then the unit's own name
    (a scenario split into an -fx/-xfx pair since)."""
    with tempfile.TemporaryDirectory(prefix="fxci.") as d:
        labels = []
        found = False
        for side, name in (("fx", unit.fx.name), ("xfx", unit.xfx.name)):
            text = None
            for candidate in (name, unit.name):
                try:
                    text = tree.git("show", "%s:%s/%s.cap" % (base, tree.rel, candidate))
                    break
                except Refusal:
                    continue
            if text is None:
                continue
            found = True
            path = os.path.join(d, side, name + ".cap")
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "wb") as f:
                f.write(text)
            for label in capfile.Reading(path, side).snaps:
                if label not in labels:
                    labels.append(label)
        return set(labels) if found else None


# --- running ----------------------------------------------------------------


def sweep(args, xfx, units, out, say):
    """One sweep.sh run over these units; {(unit, label): pair}."""
    cmd = ["bash", SWEEP, "--fx", args.fx, "--pin-sha", args.pin_sha, "--xfx", xfx, "--out", out]
    cmd += ["--jobs", str(args.jobs), "--tmp", args.tmp, "--scenarios", args.scenarios, "--ledger", args.ledger]
    for u in units:
        cmd += ["--only", u]
    say("sweep: %d scenario(s) with %s -> %s" % (len(units), xfx, out))
    started = time.time()
    r = subprocess.run(cmd, stdin=subprocess.DEVNULL)
    if r.returncode != 0:
        raise Refusal("sweep.sh exited %d for %s" % (r.returncode, out))
    say("sweep: %.0f s" % (time.time() - started))
    with open(os.path.join(out, "pairs.json"), encoding="utf-8") as f:
        return {(p["scenario"], p["label"]): p for p in json.load(f)}


def varies(pair, causes):
    """Whether a nondeterminism cause covers a row of this pair's diff."""
    for r in pair.get("rows", []):
        for side in ("fx", "xfx"):
            text = r.get(side) or ""
            if text.strip() and ledgermod.covering(causes, pair["scenario"], pair["label"], r["row"], side, text):
                return True
    return False


def pin_of(args):
    led = ledgermod.load(args.ledger, Tree(args.scenarios).repo)
    pin = next((p for p in led["pins"] if p["name"] == args.pin), None)
    if pin is None:
        raise Refusal("the ledger has no [[pin]] named %s" % args.pin)
    args.pin_sha = pin["sha256"]
    return led, pin


def full(args, red, say):
    """Every scenario, one sweep; its summary into the job's summary."""
    pin_of(args)
    out = os.path.join(args.out, "full")
    cmd = ["bash", SWEEP, "--fx", args.fx, "--pin-sha", args.pin_sha, "--xfx", args.xfx, "--out", out]
    cmd += ["--jobs", str(args.jobs), "--tmp", args.tmp, "--scenarios", args.scenarios, "--ledger", args.ledger]
    say("full sweep with %s -> %s" % (args.xfx, out))
    r = subprocess.run(cmd, stdin=subprocess.DEVNULL)
    if r.returncode != 0:
        raise Refusal("sweep.sh exited %d" % r.returncode)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(os.path.join(out, "summary.md"), encoding="utf-8") as f:
            text = f.read()
        with open(summary, "a", encoding="utf-8") as f:
            f.write(red.public(text))
    return 0


def run(args, red, say):
    led, pin = pin_of(args)
    causes = [c for c in led["causes"] if "nondeterminism" in c["kind"]]
    began = time.time()
    tree = Tree(args.scenarios)
    chosen, removed = select(tree, args.base, read_core(args.core))
    by_name = {u.name: u for u in capfile.units(tree.scenarios)}
    units = list(chosen)
    old = {name: base_labels(tree, args.base, by_name[name]) for name in units}
    say("selection: %d scenario(s): %s" % (len(units), " ".join(units)))

    timing = {}
    t = time.time()
    head = sweep(args, args.xfx, units, os.path.join(args.out, "head"), say)
    timing["head"] = time.time() - t
    final = {k: {"verdict": p["verdict"], "runs": 1, "pair": p} for k, p in head.items()}

    def is_old(key):
        return old.get(key[0]) is not None and key[1] in old[key[0]]

    def failing_old():
        return [k for k, v in final.items() if v["verdict"] not in PASS and is_old(k)]

    t = time.time()
    for n in range(1, args.retries + 1):
        again = [k for k in failing_old() if varies(final[k]["pair"], causes)]
        if not again:
            break
        res = sweep(args, args.xfx, sorted({k[0] for k in again}), os.path.join(args.out, "head-retry-%d" % n), say)
        for k in again:
            final[k]["runs"] += 1
            p = res.get(k)
            if p is not None:
                final[k]["pair"] = p
                final[k]["verdict"] = p["verdict"]
    timing["retries"] = time.time() - t

    t = time.time()
    suspects = failing_old()
    need = sorted({k[0] for k in suspects})
    base = sweep(args, args.base_xfx, need, os.path.join(args.out, "base"), say) if need else {}
    base_runs = {k: 1 for k in suspects if k in base}
    # The base gets the head's re-runs: a known-varying pair the base fails
    # once could still be one it passes, and then the head's failure is a
    # regression, not a pair neither build passes.
    for n in range(1, args.retries + 1):
        again = [k for k in base_runs if base[k]["verdict"] not in PASS and varies(base[k], causes)]
        if not again:
            break
        res = sweep(args, args.base_xfx, sorted({k[0] for k in again}), os.path.join(args.out, "base-retry-%d" % n), say)
        for k in again:
            base_runs[k] += 1
            p = res.get(k)
            if p is not None:
                base[k] = p
    timing["base"] = time.time() - t
    timing["total"] = time.time() - began

    gone = []
    for name in units:
        snapped = set(by_name[name].labels())
        gone += ["%s/%s" % (name, label) for label in sorted((old[name] or set()) - snapped)]

    rows = []
    for k in sorted(final, key=lambda k: (units.index(k[0]), k[1])):
        v = final[k]
        b = base.get(k)
        rows.append(
            {
                "scenario": k[0],
                "label": k[1],
                "head": v["verdict"],
                "runs": v["runs"],
                "new": not is_old(k),
                "base": b["verdict"] if b else None,
                "base_runs": base_runs.get(k, 0),
                "regression": is_old(k) and v["verdict"] not in PASS and bool(b) and b["verdict"] in PASS,
                "first": first_row(v["pair"]),
            }
        )
    result = {
        "pin": pin["name"],
        "fx_sha256": args.pin_sha,
        "base": tree.git("rev-parse", args.base).decode().strip(),
        "head": tree.git("rev-parse", "HEAD").decode().strip(),
        "selection": chosen,
        "removed": removed,
        "removed_pairs": gone,
        "pairs": rows,
        "regressions": sum(1 for r in rows if r["regression"]),
        "timing_seconds": {k: round(v, 1) for k, v in timing.items()},
    }
    red.write(os.path.join(args.out, "result.json"), json.dumps(result, indent=1, ensure_ascii=False) + "\n")
    text = report(result)
    red.write(os.path.join(args.out, "report.md"), text)
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(red.public(text))
    say(
        "pairs %d: pass %d, new %d, regressions %d; %.0f s"
        % (
            len(rows),
            sum(1 for r in rows if r["head"] in PASS),
            sum(1 for r in rows if r["new"]),
            result["regressions"],
            timing["total"],
        )
    )
    return 1 if result["regressions"] else 0


def first_row(pair):
    rows = pair.get("rows") or []
    if rows:
        r = rows[0]
        return {"row": r["row"], "fx": r["fx"], "xfx": r["xfx"]}
    if pair.get("reason"):
        return {"reason": pair["reason"]}
    return None


# --- the report -------------------------------------------------------------


def cell(text, limit=80):
    text = text if len(text) <= limit else text[: limit - 1] + "…"
    return "`%s`" % text.replace("`", "'").replace("|", "\\|") if text.strip() else "(blank)"


def where(first):
    if not first:
        return ""
    if "reason" in first:
        return first["reason"]
    return "row %d: %s / %s" % (first["row"], cell(first["fx"], 60), cell(first["xfx"], 60))


def report(res):
    rows = res["pairs"]
    regress = [r for r in rows if r["regression"]]
    passed = sum(1 for r in rows if r["head"] in PASS)
    t = res["timing_seconds"]
    lines = [
        "## fx parity sweep, per pull request (%s)" % res["pin"],
        "",
        "- fx %s, sha256 `%s`" % (res["pin"], res["fx_sha256"]),
        "- head `%s`, base `%s` (both xfx builds against the same fx)" % (res["head"][:12], res["base"][:12]),
        "- %d scenarios, %d snap pairs: %d pass at the head, %d new, **%d regression(s)**"
        % (len(res["selection"]), len(rows), passed, sum(1 for r in rows if r["new"]), len(regress)),
        "- wall: head %.0f s, retries %.0f s, base %.0f s, total %.0f s" % (t["head"], t["retries"], t["base"], t["total"]),
        "",
    ]
    if regress:
        lines += ["### Regressions: the base passes these, the head does not", ""]
        lines += [
            "| scenario | snap | base | head | runs (base / head) | first differing row (fx / xfx) |",
            "|---|---|---|---|---|---|",
        ]
        for r in regress:
            lines.append(
                "| %s | %s | %s | %s | %d / %d | %s |"
                % (r["scenario"], r["label"], r["base"], r["head"], r["base_runs"], r["runs"], where(r["first"]))
            )
        lines.append("")
    lines += ["### Scenarios", ""]
    lines += ["| scenario | why | pairs | pass at head | new | not passing at head |", "|---|---|---|---|---|---|"]
    for name, why in res["selection"].items():
        mine = [r for r in rows if r["scenario"] == name]
        bad = [r for r in mine if r["head"] not in PASS]
        lines.append(
            "| %s | %s | %d | %d | %d | %s |"
            % (
                name,
                ", ".join(why),
                len(mine),
                len(mine) - len(bad),
                sum(1 for r in mine if r["new"]),
                ", ".join(
                    "%s (%s%s%s%s)"
                    % (
                        r["label"],
                        r["head"],
                        ", new" if r["new"] else (", base %s" % r["base"] if r["base"] else ""),
                        ", %d head runs" % r["runs"] if r["runs"] > 1 else "",
                        ", %d base runs" % r["base_runs"] if r["base_runs"] > 1 else "",
                    )
                    for r in bad
                )
                or "",
            )
        )
    if res["removed"]:
        lines += ["", "Scenarios the change removes: %s." % ", ".join("`%s`" % n for n in res["removed"])]
    if res["removed_pairs"]:
        lines += ["", "Snaps the base takes and the head no longer does: %s." % ", ".join("`%s`" % n for n in res["removed_pairs"])]
    lines += [
        "",
        "A regression is a snap pair identical (or normalized-identical) to fx with the base's xfx and not with the "
        "head's, both run with the head's scenarios and ledger. A pair the base does not snap is new: reported, "
        "not failed. A pair a ledger `nondeterminism` cause covers runs again, up to 3 times on each build, before it "
        "counts. "
        "CI green does not replace the local re-run.",
        "",
    ]
    return "\n".join(lines)


# --- the command ------------------------------------------------------------


def parse_args(argv):
    p = argparse.ArgumentParser(prog="ci-sweep.sh", description="the parity sweep per pull request")
    p.add_argument("--base")
    p.add_argument("--select", action="store_true")
    p.add_argument("--full", action="store_true")
    p.add_argument("--fx")
    p.add_argument("--pin")
    p.add_argument("--xfx")
    p.add_argument("--base-xfx")
    p.add_argument("--out")
    p.add_argument("--jobs", type=int, default=2)
    p.add_argument("--tmp", default=os.environ.get("TMPDIR") or "/tmp")
    p.add_argument("--core", default=os.path.join(HERE, "ci-core.txt"))
    p.add_argument("--scenarios", default=os.path.join(HERE, "scenarios"))
    p.add_argument("--ledger", default=os.path.join(HERE, "parity-ledger.toml"))
    p.add_argument("--retries", type=int, default=3)
    args = p.parse_args(argv)
    if args.select and args.full:
        p.error("--select or --full, not both")
    need = ("base",) if args.select else ("fx", "pin", "xfx", "out") if args.full else ("base", "fx", "pin", "xfx", "base_xfx", "out")
    for k in need:
        if not getattr(args, k):
            p.error("--%s is required" % k.replace("_", "-"))
    for k in ("fx", "xfx", "base_xfx", "out", "tmp", "scenarios", "ledger", "core"):
        if getattr(args, k):
            setattr(args, k, os.path.abspath(getattr(args, k)))
    if args.retries < 0 or args.jobs < 1:
        p.error("--retries is at least 0 and --jobs at least 1")
    return args


def main(argv):
    args = parse_args(argv)
    known = {REPO: "<repo>", args.tmp: "<tmp>"}
    for k, name in (("out", "<out>"), ("fx", "<fx-bin>"), ("xfx", "<xfx-bin>"), ("base_xfx", "<base-xfx-bin>")):
        if getattr(args, k):
            known[getattr(args, k)] = name
    red = Redactor(known)

    def say(line):
        print("ci-sweep: " + red(line), flush=True)

    try:
        if args.select:
            chosen, removed = select(Tree(args.scenarios), args.base, read_core(args.core))
            for name, why in chosen.items():
                print("%s\t%s" % (name, ", ".join(why)))
            for name in removed:
                print("%s\tremoved" % name)
            return 0
        if os.path.exists(args.out) and os.listdir(args.out):
            raise Refusal("--out is not empty")
        os.makedirs(args.out, exist_ok=True)
        return full(args, red, say) if args.full else run(args, red, say)
    except (Refusal, ledgermod.LedgerError, ValueError, OSError) as e:
        print("ci-sweep: %s" % red(str(e)), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

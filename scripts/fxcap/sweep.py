"""The parity sweep: every scenario on fx and on xfx, every snap pair compared
cell by cell, and a scoreboard of how many are the same.

usage: sweep.sh --fx <bin> --pin-sha <sha256> [--xfx <bin>] [--size 40x120]
                [--jobs N] [--only <glob>]... --out <dir outside the repo>
       sweep.sh --fx <bin> --pin-sha <sha256> --inventory-only --out <dir>
       sweep.sh --rescore --out <dir of an earlier sweep>

  --fx           the reference fx binary; its sha256 must be --pin-sha, or
                 nothing runs (checked again after the run: fx upgrades itself
                 in place when it can, and capture.sh guards each run too)
  --xfx          default: target/debug/xfx in this checkout
  --only         a glob over scenario names; repeatable. The scoreboard says
                 PARTIAL and its denominator is the scenarios selected.
  --inventory-only
                 reads fx's surface inventory and maps the scenarios to it;
                 no scenario runs
  --rescore      classifies an earlier sweep's captures again with the ledger
                 as it is now (no capture runs): a ledger change is reviewed
                 against the same captures

Each scenario is one capture.sh run per binary, used as it is, `--jobs`
scenarios at a time, longest first. A scenario's two runs share a TMPDIR of
their own under --tmp (default $TMPDIR), one after the other, so a path on
screen is the same path on both sides, and the one socket in it is the run's:
if the run outlives its deadline -- 60 s plus 1.5 times the scenario's waits
for that binary -- its tmux server is stopped through that socket, then the
run and what it started, each by PID.

A scenario is `ran` when both runs exit 0, `timeout` when either outlived its
deadline, `failed` otherwise. The unit counted is the snap pair: every label a
scenario snaps (`snap x`, `snapfull x` as `x.full`), read from the .cap file,
so a scenario that failed or timed out counts every snap it would have taken,
as failed or timeout, in the total.

    pass = (identical + normalized-identical) / total

Classification is `ledger.py`'s, over `parity-ledger.toml`. --out gets
`scoreboard.md`, `scoreboard.json` and `summary.md` (redacted: no local path,
see redact.py), `pairs.json` and `diffs/` (raw, this machine's only), the
captures under `runs/`, fx's inventory under `inventory-run/`, and the .cap
files as they were under `scenarios-used/` (what --rescore reads).

FXCAP_SWEEP_CAPTURE names another capture script, for this sweep's own tests.
"""

import argparse
import concurrent.futures
import datetime
import fnmatch
import glob
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.realpath(__file__))
# No __pycache__ beside the scripts: the checkout holds only what is committed.
sys.dont_write_bytecode = True
sys.path.insert(0, HERE)

import capfile  # noqa: E402
import cells  # noqa: E402
import inventory  # noqa: E402
import ledger as ledgermod  # noqa: E402
from redact import Redactor  # noqa: E402

REPO = os.path.realpath(os.path.join(HERE, "..", ".."))
COLUMNS = (
    "total",
    "ran",
    "identical",
    "normalized-identical",
    "approved-divergence",
    "approved-divergence-cascade",
    "replaced-in-next-pin",
    "unresolved-defect",
    "failed",
    "timeout",
)
CLASSIFIED = ledgermod.VERDICTS


class Refusal(Exception):
    pass


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main_worktree():
    try:
        common = subprocess.run(
            ["git", "-C", REPO, "rev-parse", "--path-format=absolute", "--git-common-dir"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        return os.path.realpath(os.path.dirname(common))
    except (OSError, subprocess.CalledProcessError):
        return None


def inside(path, root):
    return root is not None and (path == root or path.startswith(root.rstrip("/") + "/"))


def check_out(out):
    real = os.path.realpath(out)
    for root in (REPO, main_worktree()):
        if inside(real, root):
            raise Refusal("--out must be outside the repository")
    return real


# --- running ----------------------------------------------------------------


class Runner:
    def __init__(self, args, out, say):
        self.args = args
        self.out = out
        self.say = say
        self.capture = os.environ.get("FXCAP_SWEEP_CAPTURE") or os.path.join(HERE, "capture.sh")
        self.root = tempfile.mkdtemp(prefix="fxsw.", dir=args.tmp)
        self.lock = threading.Lock()
        self.running = {}
        self.done = 0
        self.total = 0
        self.peak_load = os.getloadavg()[0]
        self.start_load = self.peak_load
        self._stop = threading.Event()
        # A socket path past ~104 bytes is refused by the kernel: keep room.
        if len(self.root) + len("/000/fxcap.XXXXXX/s") > 100:
            raise Refusal("--tmp is too long for a tmux socket path under it")

    def sample(self):
        while not self._stop.wait(5):
            self.peak_load = max(self.peak_load, os.getloadavg()[0])

    def env(self, jobtmp, scenarios):
        env = dict(os.environ)
        for k in ("TMUX", "TMUX_PANE", "FXCAP_SCENARIOS"):
            env.pop(k, None)
        env["TMPDIR"] = jobtmp
        env["FX_BIN"] = self.args.fx
        env["XFX_BIN"] = self.args.xfx
        if scenarios:
            env["FXCAP_SCENARIOS"] = scenarios
        return env

    def run_unit(self, jobs):
        """A unit's runs, one after the other in one TMPDIR: each binary sees
        the same path to its run directory, so a path on screen differs only
        in capture.sh's own `fxcap.XXXXXX` (the run-directory normalization),
        and the one run in it is the one whose socket is there."""
        jobtmp = os.path.join(self.root, "%03d" % jobs[0]["n"])
        out = []
        for job in jobs:
            shutil.rmtree(jobtmp, ignore_errors=True)
            os.makedirs(jobtmp)
            out.append(self.run(job, jobtmp))
            self.report(job, out[-1])
        shutil.rmtree(jobtmp, ignore_errors=True)
        return out

    def report(self, job, st):
        with self.lock:
            self.done += 1
            done = self.done
        what = "timeout" if st["timed_out"] else ("ran" if st["exit"] == 0 else "failed exit %d" % st["exit"])
        self.say("[%d/%d] %s %s: %s (%.0fs)" % (done, self.total, job["side"], job["unit"], what, st["seconds"]))

    def run(self, job, jobtmp):
        os.makedirs(job["outdir"], exist_ok=True)
        started = time.time()
        with open(job["log"], "wb") as log:
            p = subprocess.Popen(
                ["bash", self.capture, job["side"], job["scenario"], self.args.size, job["outdir"]],
                cwd=REPO,
                env=self.env(jobtmp, job.get("scenarios")),
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            with self.lock:
                self.running[p.pid] = (p, jobtmp)
            timed_out = False
            try:
                rc = p.wait(timeout=job["timeout"])
            except subprocess.TimeoutExpired:
                timed_out = True
                self.stop(p, jobtmp)
                rc = p.wait()
            with self.lock:
                self.running.pop(p.pid, None)
        leaked = self.stop_servers(jobtmp)
        status = {
            "side": job["side"],
            "scenario": job["scenario"],
            "exit": rc,
            "timed_out": timed_out,
            "deadline": job["timeout"],
            "seconds": round(time.time() - started, 1),
            "leaked_tmux_servers": leaked,
        }
        with open(job["status"], "w") as f:
            json.dump(status, f, indent=1)
        return status

    def stop_servers(self, jobtmp):
        """Stops every tmux server still answering on a socket of this run's,
        by its pid and its panes' pids; returns how many there were."""
        n = 0
        for sock in glob.glob(os.path.join(jobtmp, "fxcap.*", "s")):
            tm = ["tmux", "-f", "/dev/null", "-S", sock]
            try:
                alive = subprocess.run(tm + ["list-sessions"], capture_output=True, timeout=10).returncode == 0
            except subprocess.TimeoutExpired:
                alive = True
            if not alive:
                continue
            n += 1
            pids = []
            for query in (["display-message", "-p", "#{pid}"], ["list-panes", "-a", "-F", "#{pane_pid}"]):
                try:
                    r = subprocess.run(tm + query, capture_output=True, text=True, timeout=10)
                    pids += [int(x) for x in r.stdout.split() if x.isdigit()]
                except subprocess.TimeoutExpired:
                    pass
            try:
                subprocess.run(tm + ["kill-server"], capture_output=True, timeout=10)
            except subprocess.TimeoutExpired:
                pass
            time.sleep(0.5)
            for pid in pids:
                kill(pid, signal.SIGKILL)
        return n

    def stop(self, p, jobtmp):
        """A run past its deadline: its tmux server through its socket, then
        the run and every process it started, by PID."""
        tree = descendants(p.pid)
        self.stop_servers(jobtmp)
        kill(p.pid, signal.SIGTERM)
        try:
            p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            pass
        for pid in [p.pid] + tree:
            kill(pid, signal.SIGKILL)

    def stop_all(self):
        with self.lock:
            running = list(self.running.values())
        for p, jobtmp in running:
            self.stop(p, jobtmp)

    def close(self):
        self._stop.set()
        shutil.rmtree(self.root, ignore_errors=True)


def kill(pid, sig):
    try:
        os.kill(pid, sig)
    except (ProcessLookupError, PermissionError):
        pass


def descendants(root):
    try:
        listing = subprocess.run(["ps", "-A", "-o", "pid=,ppid="], capture_output=True, text=True, timeout=10).stdout
    except (OSError, subprocess.TimeoutExpired):
        return []
    children = {}
    for line in listing.split("\n"):
        parts = line.split()
        if len(parts) == 2 and parts[0].isdigit() and parts[1].isdigit():
            children.setdefault(int(parts[1]), []).append(int(parts[0]))
    out, todo = [], [root]
    while todo:
        for c in children.get(todo.pop(), []):
            if c not in out:
                out.append(c)
                todo.append(c)
    return out


def deadline(reading, args):
    return int(args.timeout_slack + args.timeout_scale * reading.waits)


def unit_jobs(units, out, args):
    jobs = []
    for u in units:
        for side in ("fx", "xfx"):
            r = u.reading(side)
            d = os.path.join(out, "runs", u.name, side)
            jobs.append(
                {
                    "unit": u.name,
                    "side": side,
                    "scenario": r.name,
                    "outdir": d,
                    "log": os.path.join(d, "capture.log"),
                    "status": os.path.join(d, "status.json"),
                    "timeout": deadline(r, args),
                    "scenarios": args.scenarios if args.scenarios != os.path.join(HERE, "scenarios") else None,
                }
            )
    return jobs


def inventory_job(out, args):
    d = os.path.join(out, "inventory-run")
    r = capfile.Reading(os.path.join(HERE, "inventory", "inventory.cap"), "fx")
    return {
        "unit": "(inventory)",
        "side": "fx",
        "scenario": "inventory",
        "outdir": d,
        "log": os.path.join(d, "capture.log"),
        "status": os.path.join(d, "status.json"),
        "timeout": deadline(r, args),
        "scenarios": os.path.join(HERE, "inventory"),
    }


def run_jobs(jobs, runner, args):
    """Every unit's runs, `--jobs` units at a time, the longest first."""
    by_unit = {}
    for n, job in enumerate(jobs):
        job["n"] = n
        by_unit.setdefault(job["unit"], []).append(job)
    order = sorted(by_unit.values(), key=lambda js: -sum(j["timeout"] for j in js))
    runner.total = len(jobs)
    sampler = threading.Thread(target=runner.sample, daemon=True)
    sampler.start()
    try:
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
            for fut in concurrent.futures.as_completed([pool.submit(runner.run_unit, js) for js in order]):
                fut.result()
    except BaseException:
        runner.stop_all()
        raise


# --- scoring ----------------------------------------------------------------


def read_status(path):
    try:
        with open(path) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def unit_status(out, u):
    reasons, timed, failed = [], False, False
    for side in ("fx", "xfx"):
        st = read_status(os.path.join(out, "runs", u.name, side, "status.json"))
        if st is None:
            failed = True
            reasons.append("%s: no run" % side)
        elif st["timed_out"]:
            timed = True
            reasons.append("%s: past its %ds deadline" % (side, st["deadline"]))
        elif st["exit"] != 0:
            failed = True
            reasons.append("%s: exit %d" % (side, st["exit"]))
    if timed:
        return "timeout", reasons
    if failed:
        return "failed", reasons
    return "ran", reasons


def capture_path(out, u, side, label, size, ext):
    r = u.reading(side)
    return os.path.join(out, "runs", u.name, side, "%s-%s-%s-%s.%s" % (side, r.name, size, label, ext))


def read_text(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read()


def counts():
    return {c: 0 for c in COLUMNS}


def add(into, verdict):
    into["total"] += 1
    into[verdict] += 1
    if verdict in CLASSIFIED:
        into["ran"] += 1


def pass_rate(c):
    return (c["identical"] + c["normalized-identical"]) / c["total"] if c["total"] else 0.0


def score(units, out, size, led, pin):  # noqa: C901
    rules = {side: ledgermod.rules_for(led, side) for side in ("fx", "xfx")}
    divs = ledgermod.active(led, pin["name"] if pin else None)
    usage = {d["id"]: {"rows": 0, "snaps": set()} for d in divs}
    diffdir = os.path.join(out, "diffs")
    shutil.rmtree(diffdir, ignore_errors=True)
    scen, pairs = [], []
    for u in units:
        status, reasons = unit_status(out, u)
        c, first, banner = counts(), None, None
        for label in u.labels():
            pair = {"scenario": u.name, "label": label}
            if status != "ran":
                pair["verdict"], pair["reason"] = status, "; ".join(reasons)
            elif label not in u.fx.snaps or label not in u.xfx.snaps:
                pair["verdict"], pair["reason"] = "failed", "snapped by one binary only"
            else:
                fxp = capture_path(out, u, "fx", label, size, "ansi")
                xp = capture_path(out, u, "xfx", label, size, "ansi")
                if not (os.path.isfile(fxp) and os.path.isfile(xp)):
                    pair["verdict"], pair["reason"] = "failed", "a capture is missing"
                else:
                    a, b = cells.parse(read_text(fxp)), cells.parse(read_text(xp))
                    if banner is None and b and b[0]:
                        banner = cells.text_of(b[0])
                    na, nb = cells.normalize(a, rules["fx"]), cells.normalize(b, rules["xfx"])
                    readings = [cells.normalize(a, rules["fx"], absorb=way) for way in ("after", "before")]
                    diff = cells.differing(na, nb, readings)
                    verdict, matched = ledgermod.classify(a == b, diff, u.name, label, divs)
                    left = None
                    if verdict == "unresolved-defect":
                        promoted, left = ledgermod.cascade(
                            aligned_fx(na, readings, nb), nb, u.name, label, divs, cells.text_of
                        )
                        if promoted:
                            verdict = promoted
                            pair["positional"] = "unresolved-defect"
                        pair["aligned"] = [
                            {"block": blk, "side": side, "row": row, "text": text, "matched": ids}
                            for blk, side, row, text, ids in left
                        ]
                        if promoted:
                            for *_, ids in left:
                                for e in ids:
                                    usage[e]["rows"] += 1
                                    usage[e]["snaps"].add("%s/%s" % (u.name, label))
                        else:
                            pair["causes"], pair["unattributed"] = attribute(left, u.name, label, led["causes"])
                    pair["verdict"] = verdict
                    if diff:
                        pair["unaligned"] = cells.unaligned(na, nb)
                    pair["rows"] = [
                        {"row": i + 1, "fx": ft, "xfx": xt, "attrs_only": same, "matched": ids}
                        for (i, ft, xt, _, same), (_, ids) in zip(diff, matched)
                    ]
                    for (_, ids) in matched:
                        for e in ids:
                            usage[e]["rows"] += 1
                            usage[e]["snaps"].add("%s/%s" % (u.name, label))
                    if diff:
                        write_diff(diffdir, u.name, label, verdict, diff, na, nb, matched, pair["unaligned"], left)
                    if verdict == "unresolved-defect" and first is None:
                        row = next((r for r in pair["rows"] if not r["matched"]), pair["rows"][0])
                        above = [r for r in pair["rows"] if r["matched"] and r["row"] < row["row"]]
                        first = dict(row, label=label, unaligned=pair["unaligned"])
                        if above:
                            first["below"] = {"row": above[-1]["row"], "matched": above[-1]["matched"]}
            add(c, pair["verdict"])
            pairs.append(pair)
        scen.append(
            {
                "scenario": u.name,
                "group": u.group(),
                "status": status,
                "reasons": reasons,
                "counts": c,
                "first_unresolved": first,
                "xfx_banner": banner,
            }
        )
    for v in usage.values():
        v["snaps"] = len(v["snaps"])
    return scen, pairs, usage


def attribute(left, unit, label, causes):
    """The causes of an unresolved pair's rows: each row left without a match
    and not accounted for by an approved divergence goes to the first cause
    that covers its side's text; a blank row to its block's cause, or to a
    cause that covers it by row number or whole snap. Returns ([cause ids],
    [(side, row, text)] no cause covers)."""
    todo = [(blk, side, row, text) for blk, side, row, text, ids in left if not ids]
    if not left:
        return [], [("both", 0, "(aligned, the rows are the same: the screen is shifted)")]
    found, block_cause, unattributed = [], {}, []

    def take(cid):
        if cid not in found:
            found.append(cid)

    for blk, side, row, text in todo:
        if text.strip():
            hit = ledgermod.covering(causes, unit, label, row, side, text)
            if hit:
                take(hit[0]["id"])
                block_cause.setdefault(blk, hit[0]["id"])
            else:
                unattributed.append((side, row, text))
    for blk, side, row, text in todo:
        if text.strip() or blk in block_cause:
            continue
        hit = [c for c in ledgermod.covering(causes, unit, label, row, side, text) if c["whole_snap"] or c["rows"]]
        if hit:
            take(hit[0]["id"])
        else:
            unattributed.append((side, row, "(blank)"))
    return found, unattributed


def aligned_fx(na, readings, nb):
    """fx's rows for the aligned reading: each row in whichever of its
    readings (positional, or the padding absorbed after or before) xfx shows
    somewhere, else the positional one."""
    shown = {tuple(r) for r in nb}
    out = []
    for i, row in enumerate(na):
        pick = row
        for r in [row] + [rd[i] for rd in readings if i < len(rd)]:
            if tuple(r) in shown:
                pick = r
                break
        out.append(pick)
    return out


def write_diff(diffdir, unit, label, verdict, diff, na, nb, matched, unaligned, left=None):
    d = os.path.join(diffdir, unit)
    os.makedirs(d, exist_ok=True)
    lines = [
        "# %s %s: %s" % (unit, label, verdict),
        "# %d rows differ by position; aligned as sequences, %d fx rows and %d xfx rows have no match"
        % ((len(diff),) + tuple(unaligned)),
    ]
    if left is not None:
        lines.append("# aligned: every row left without a match, and what accounts for it")
        for blk, side, row, text, ids in left:
            lines.append("#   [%d] %-3s %2d: %s -> %s" % (blk, side, row, text, ", ".join(ids) or "UNCOVERED"))
        lines.append("# positional rows:")
    for (i, ft, xt, at, same), (_, ids) in zip(diff, matched):
        a = na[i] if i < len(na) else []
        b = nb[i] if i < len(nb) else []
        lines.append("row %d%s: %s" % (i + 1, " (attributes only)" if same else "", ", ".join(ids) or "unresolved"))
        lines.append("  fx : %s" % ft)
        lines.append("  xfx: %s" % xt)
        lines.append("  first differing cell %d: fx %r / xfx %r" % (at, a[at : at + 2], b[at : at + 2]))
    with open(os.path.join(d, label + ".diff"), "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")


def aggregate(scen, key):
    groups = {}
    for s in scen:
        g = groups.setdefault(s[key], {"scenarios": 0, "counts": counts()})
        g["scenarios"] += 1
        for col in COLUMNS:
            g["counts"][col] += s["counts"][col]
    return groups


def surfaces(inv, covered, scen):
    by_name = {s["scenario"]: s for s in scen}
    out = []
    for s in inv["commands"] + inv["keys"]:
        units = covered.get(s["id"], [])
        c = counts()
        swept = [u for u in units if u in by_name]
        for u in swept:
            for col in COLUMNS:
                c[col] += by_name[u]["counts"][col]
        out.append({"surface": s["id"], "kind": s["kind"], "scenarios": units, "swept": len(swept), "counts": c})
    return out


# --- writing ----------------------------------------------------------------


def cell(text, limit=70):
    text = text if len(text) <= limit else text[: limit - 1] + "…"
    return "`%s`" % text.replace("`", "'").replace("|", "\\|") if text.strip() else "(blank)"


def table(rows, head):
    lines = ["| " + " | ".join(head) + " |", "|" + "---|" * len(head)]
    lines += ["| " + " | ".join(str(x) for x in r) + " |" for r in rows]
    return "\n".join(lines)


def pct(c):
    return "%.1f%%" % (100 * pass_rate(c))


def count_cells(c):
    return [c[col] for col in COLUMNS]


def coverage_lines(cov):
    lines = [
        "fx's inventory (%s): %d surfaces -- %d commands, %d key hints. Covered by a scenario: %d. **Uncovered: %d**%s."
        % (
            cov["pin"],
            cov["surfaces"],
            cov["commands"],
            cov["keys"],
            cov["covered"],
            len(cov["uncovered"]),
            (" -- " + ", ".join("`%s`" % s for s in cov["uncovered"])) if cov["uncovered"] else "",
        )
    ]
    if cov.get("unmapped"):
        lines.append(
            "Tokens typed as commands that are no command of this inventory (aliases, partial names, refusals): %s."
            % ", ".join("`%s`" % t for t in cov["unmapped"])
        )
    return lines


def header_lines(meta):
    lines = [
        "- fx: pin `%s` (fx %s), sha256 `%s`, checked before and after the run"
        % (meta["pin"], meta["fx_version"], meta["fx_sha256"]),
        "- xfx: `%s`, sha256 `%s`" % (meta.get("xfx_banner") or "?", meta["xfx_sha256"]),
        "- size %s, `--jobs %d`, %d scenarios%s, wall %s, load %.1f at the start and %.1f at the peak (1-minute average, sampled every 5 s)"
        % (
            meta["size"],
            meta["jobs"],
            meta["scenarios"],
            (" -- **PARTIAL**: `--only %s`" % " ".join(meta["only"])) if meta["only"] else "",
            meta["wall"],
            meta["start_load"],
            meta["peak_load"],
        ),
        "- ledger sha256 `%s`: %d normalizations; %d divergences approved for this pin"
        % (meta["ledger_sha256"], meta["normalizations"], meta["divergences"]),
    ]
    if meta.get("leaked_tmux_servers"):
        lines.append(
            "- **%d tmux server(s) outlived their run** and were stopped by the sweep" % meta["leaked_tmux_servers"]
        )
    return lines


UNIT_NOTE = (
    "The unit is the snap pair: one `snap` or `snapfull` label of a scenario, read from its .cap file, so a "
    "scenario that failed or timed out counts every snap it would have taken. "
    "pass = (identical + normalized-identical) / total. approved-divergence-cascade is a pair that differs by "
    "position beyond its approved rows (a row they pushed down) and whose every row left without a match, the two "
    "sides aligned as sequences, is approved; the positional diff stays in the raw output."
)
GROUP_NOTE = (
    "Groups are the leading letters of a scenario's name (`model-anchor`, `model2` and `model` are `model`). "
    "A scenario counts under every surface it exercises, so the surface rows overlap."
)


def write_reports(out, meta, scen, pairs, usage, inv_block, led, red, say):
    total = counts()
    for s in scen:
        for col in COLUMNS:
            total[col] += s["counts"][col]
    groups = aggregate(scen, "group")
    status = {k: sum(1 for s in scen if s["status"] == k) for k in ("ran", "failed", "timeout")}
    unused = sorted(i for i, v in usage.items() if v["rows"] == 0)
    causes, cause_lines = cause_table(pairs, led)

    md = ["# fx parity sweep", ""] + header_lines(meta) + [""]
    md += ["## Coverage", ""]
    md += coverage_lines(inv_block["coverage"]) if inv_block else ["(no inventory)"]
    md += ["", "## Pass", ""]
    md += [
        "Scenarios: %d -- ran %d, failed %d, timeout %d."
        % (len(scen), status["ran"], status["failed"], status["timeout"]),
        "",
        "**pass = %d / %d = %s**" % (total["identical"] + total["normalized-identical"], total["total"], pct(total)),
        "",
        table([count_cells(total)], COLUMNS),
        "",
        UNIT_NOTE,
        "",
        "## By group",
        "",
        GROUP_NOTE,
        "",
    ]
    md.append(
        table(
            [[g, v["scenarios"]] + count_cells(v["counts"]) + [pct(v["counts"])] for g, v in sorted(groups.items())],
            ("group", "scenarios") + COLUMNS + ("pass",),
        )
    )
    if inv_block:
        md += ["", "## By surface", ""]
        md.append(
            table(
                [
                    ["`%s`" % s["surface"], s["swept"] or "**none**"] + count_cells(s["counts"]) + [pct(s["counts"])]
                    for s in inv_block["surfaces"]
                ],
                ("surface", "scenarios") + COLUMNS + ("pass",),
            )
        )
    md += ["", "## By scenario", ""]
    md.append(
        table(
            [
                [s["scenario"], s["status"]]
                + count_cells(s["counts"])
                + [pct(s["counts"]), first_cell(s["first_unresolved"])]
                for s in scen
            ],
            ("scenario", "status") + COLUMNS + ("pass", "first unresolved row (fx / xfx)"),
        )
    )
    md += ["", "## Causes of the unresolved snaps", ""] + cause_lines
    md += ["", "## Ledger", ""]
    md.append(
        table(
            [
                [d["id"], d["class"], cell(d["owner"], 60), cell(d["expires"], 30), usage[d["id"]]["rows"], usage[d["id"]]["snaps"]]
                for d in ledgermod.active(led, meta["pin_name"])
            ],
            ("divergence", "class", "owner", "expires", "rows it covers", "snaps"),
        )
    )
    lapsed = ledgermod.expired(led, meta["pin_name"])
    if lapsed:
        md += ["", "Past their date, applied to nothing: %s." % ", ".join("`%s`" % d["id"] for d in lapsed)]
    if unused:
        md += ["", "Approved for this pin but covering no row here: %s." % ", ".join("`%s`" % i for i in unused)]
    red.write(os.path.join(out, "scoreboard.md"), "\n".join(md) + "\n")

    board = {
        "meta": meta,
        "totals": total,
        "pass": pass_rate(total),
        "scenario_status": status,
        "groups": groups,
        "scenarios": scen,
        "surfaces": inv_block["surfaces"] if inv_block else None,
        "coverage": inv_block["coverage"] if inv_block else None,
        "ledger": {"usage": usage, "unused": unused},
        "causes": causes,
    }
    red.write(os.path.join(out, "scoreboard.json"), json.dumps(board, indent=1, ensure_ascii=False) + "\n")
    with open(os.path.join(out, "pairs.json"), "w", encoding="utf-8") as f:
        json.dump(pairs, f, indent=1, ensure_ascii=False)

    top = sorted(
        (s for s in scen if s["counts"]["unresolved-defect"]),
        key=lambda s: (-s["counts"]["unresolved-defect"], s["scenario"]),
    )[:10]
    sm = ["## Scoreboard (%s)" % meta["pin"], ""] + header_lines(meta) + [""]
    sm += coverage_lines(inv_block["coverage"]) if inv_block else []
    sm += [
        "",
        "Scenarios: %d -- ran %d, failed %d, timeout %d. **pass = %d / %d = %s**"
        % (
            len(scen),
            status["ran"],
            status["failed"],
            status["timeout"],
            total["identical"] + total["normalized-identical"],
            total["total"],
            pct(total),
        ),
        "",
        table([count_cells(total)], COLUMNS),
        "",
        UNIT_NOTE + " " + GROUP_NOTE,
        "",
        table(
            [[g, v["scenarios"]] + count_cells(v["counts"]) + [pct(v["counts"])] for g, v in sorted(groups.items())],
            ("group", "scenarios") + COLUMNS + ("pass",),
        ),
        "",
        "Causes of the unresolved snaps:",
        "",
    ] + cause_lines + [
        "",
        "Most unresolved snaps (scenario, unresolved of total, its first unresolved row):",
        "",
    ]
    for s in top:
        sm.append(
            "- `%s` %d/%d -- %s"
            % (s["scenario"], s["counts"]["unresolved-defect"], s["counts"]["total"], first_cell(s["first_unresolved"]))
        )
    for s in scen:
        if s["status"] != "ran":
            sm.append("- `%s` %s: %s" % (s["scenario"], s["status"], "; ".join(s["reasons"])))
    red.write(os.path.join(out, "summary.md"), "\n".join(sm) + "\n")
    say(
        "pass = %d / %d = %s; unresolved %d, failed %d, timeout %d"
        % (
            total["identical"] + total["normalized-identical"],
            total["total"],
            pct(total),
            total["unresolved-defect"],
            total["failed"],
            total["timeout"],
        )
    )
    say("wrote %s" % os.path.join(out, "scoreboard.md"))


KIND_LABEL = {"decision": "a", "unit": "b", "nondeterminism": "c", "scenario": "c", "defect": "d"}


def cause_table(pairs, led):
    """Per cause: the unresolved snaps it is a cause of, and of the only
    cause of; and the snaps a row of which no cause covers."""
    unresolved = [p for p in pairs if p["verdict"] == "unresolved-defect" and "causes" in p]
    rows, out = [], []
    for c in led["causes"]:
        snaps = [p for p in unresolved if c["id"] in p["causes"]]
        if not snaps:
            continue
        sole = sum(1 for p in snaps if p["causes"] == [c["id"]] and not p["unattributed"])
        where = c["owner"]
        if "defect" in c["kind"]:
            xfx = c["xfx"] if isinstance(c["xfx"], str) else "; ".join(c["xfx"])
            fx = c["fx"] if isinstance(c["fx"], str) else "; ".join(c["fx"])
            where = "%s -- xfx %s / fx %s" % (where, xfx, fx)
        if "nondeterminism" in c["kind"] or "scenario" in c["kind"]:
            where = "%s -- fix: %s" % (where, c["fix"])
        kind = "+".join(KIND_LABEL[k] for k in c["kind"])
        out.append({"cause": c["id"], "kind": c["kind"], "snaps": len(snaps), "sole": sole, "owner": c["owner"]})
        rows.append([c["id"], kind, len(snaps), sole, cell(c["why"], 90), cell(where, 140)])
    loose = [p for p in unresolved if p["unattributed"]]
    rows.sort(key=lambda r: -r[2])
    lines = [
        "%d unresolved snaps; a snap counts under every cause of one of its rows (`only`: that cause alone)." % len(unresolved),
        "Kinds: a = awaiting a user decision, b = a unit in flight, c = the capture (nondeterminism, or the scenario script), d = a defect.",
        "",
        table(rows, ("cause", "kind", "snaps", "only", "what", "owner")),
    ]
    if loose:
        lines += ["", "Rows no cause covers (%d snaps):" % len(loose)]
        for p in loose[:20]:
            side, row, text = p["unattributed"][0]
            lines.append("- `%s/%s` %s row %d: %s" % (p["scenario"], p["label"], side, row, cell(text, 80)))
    return {"by_cause": out, "unattributed": [(p["scenario"], p["label"]) for p in loose]}, lines


def first_cell(first):
    if not first:
        return ""
    text = "%s row %d: %s / %s (aligned: %d/%d rows unmatched" % (
        (first["label"], first["row"], cell(first["fx"], 50), cell(first["xfx"], 50)) + tuple(first["unaligned"])
    )
    if first.get("below"):
        text += "; under approved row %d, %s" % (first["below"]["row"], ", ".join(first["below"]["matched"]))
    return text + ")"


def inventory_block(out, size, units, pin_label):
    inv = inventory.read(os.path.join(out, "inventory-run"), size)
    covered, unmapped = inventory.coverage(inv, units)
    every = inv["commands"] + inv["keys"]
    cov = {
        "pin": pin_label,
        "surfaces": len(every),
        "commands": len(inv["commands"]),
        "keys": len(inv["keys"]),
        "covered": sum(1 for s in every if covered[s["id"]]),
        "uncovered": [s["id"] for s in every if not covered[s["id"]]],
        "unmapped": sorted({t for ts in unmapped.values() for t in ts}),
        "unmapped_by_scenario": unmapped,
    }
    return inv, covered, cov


# --- the commands -----------------------------------------------------------


def fmt_wall(seconds):
    m, s = divmod(int(round(seconds)), 60)
    return "%dm%02ds" % (m, s)


def select(units, only):
    if not only:
        return units
    picked = [u for u in units if any(fnmatch.fnmatchcase(u.name, g) for g in only)]
    if not picked:
        raise Refusal("--only %s selects no scenario" % " ".join(only))
    return picked


def verify_pin(fx, pin_sha):
    if not re.fullmatch(r"[0-9a-f]{64}", pin_sha):
        raise Refusal("--pin-sha is a full sha256, 64 hex digits")
    if not (os.path.isfile(fx) and os.access(fx, os.X_OK)):
        raise Refusal("--fx is not an executable file")
    got = sha256(fx)
    if got != pin_sha:
        raise Refusal("the fx binary's sha256 is %s, not the pin %s: refusing to run" % (got, pin_sha))


def start(args, red, say):
    out = check_out(args.out)
    pin_sha = args.pin_sha.lower()
    verify_pin(args.fx, pin_sha)
    led = ledgermod.load(args.ledger, REPO)
    pin = ledgermod.pin_named(led, pin_sha)
    units = select(capfile.units(args.scenarios), args.only) if not args.inventory_only else capfile.units(args.scenarios)
    if not args.inventory_only and not (os.path.isfile(args.xfx) and os.access(args.xfx, os.X_OK)):
        raise Refusal("--xfx is not an executable file (cargo build first)")
    if os.path.exists(out) and os.listdir(out):
        raise Refusal("--out is not empty")
    os.makedirs(out, exist_ok=True)
    # The .cap files as they are now, for scoring: a later --rescore reads the
    # labels the captures were taken with, whatever the checkout holds by then.
    used = os.path.join(out, "scenarios-used")
    os.makedirs(used)
    for name in os.listdir(args.scenarios):
        if name.endswith(".cap"):
            shutil.copy2(os.path.join(args.scenarios, name), used)

    runner = Runner(args, out, say)
    began = time.time()
    jobs = [inventory_job(out, args)] + ([] if args.inventory_only else unit_jobs(units, out, args))
    try:
        run_jobs(jobs, runner, args)
    finally:
        runner.close()
    wall = time.time() - began
    if sha256(args.fx) != pin_sha:
        raise Refusal("the fx binary changed during the run: no scoreboard is written")
    meta = {
        "pin": pin["name"] if pin else "unlisted pin",
        "pin_name": pin["name"] if pin else None,
        "fx_version": pin["version"] if pin else "?",
        "fx_sha256": pin_sha,
        "fx": args.fx,
        "xfx": args.xfx,
        "xfx_sha256": sha256(args.xfx) if os.path.isfile(args.xfx) else None,
        "repo_head": git_head(),
        "size": args.size,
        "jobs": args.jobs,
        "only": args.only or [],
        "inventory_only": args.inventory_only,
        "units": [u.name for u in units] if not args.inventory_only else [],
        "scenarios_dir": used,
        "scenarios_from": args.scenarios,
        "started": datetime.datetime.fromtimestamp(began).astimezone().isoformat(timespec="seconds"),
        "wall_seconds": round(wall, 1),
        "wall": fmt_wall(wall),
        "start_load": round(runner.start_load, 2),
        "peak_load": round(runner.peak_load, 2),
    }
    with open(os.path.join(out, "run.json"), "w") as f:
        json.dump(meta, f, indent=1)
    return finish(out, meta, led, red, say)


def git_head():
    try:
        return subprocess.run(["git", "-C", REPO, "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def finish(out, meta, led, red, say):
    with open(led["path"], "rb") as f:
        meta["ledger_sha256"] = hashlib.sha256(f.read()).hexdigest()
    meta["normalizations"] = len(led["normalizations"])
    meta["divergences"] = len(ledgermod.active(led, meta["pin_name"]))
    all_units = capfile.units(meta["scenarios_dir"])
    inv, covered, cov = inventory_block(out, meta["size"], all_units, meta["pin"])
    red.write(
        os.path.join(out, "inventory.json"),
        json.dumps(dict(inv, pin=meta["pin"], fx_sha256=meta["fx_sha256"], covered_by=covered), indent=1, ensure_ascii=False)
        + "\n",
    )
    if meta["inventory_only"]:
        lines = ["## Inventory (%s)" % meta["pin"], ""] + coverage_lines(cov) + [""]
        lines.append(
            table(
                [["`%s`" % s["id"], s.get("category", "key"), cell(s.get("description", ", ".join(s.get("keys", []))), 80), len(covered[s["id"]])]
                 for s in inv["commands"] + inv["keys"]],
                ("surface", "category", "help entry / keys", "scenarios"),
            )
        )
        red.write(os.path.join(out, "summary.md"), "\n".join(lines) + "\n")
        red.write(os.path.join(out, "coverage.json"), json.dumps(cov, indent=1, ensure_ascii=False) + "\n")
        say("\n".join(coverage_lines(cov)))
        return 0
    units = [u for u in all_units if u.name in set(meta["units"])]
    pin = next((p for p in led["pins"] if p["name"] == meta["pin_name"]), None)
    scen, pairs, usage = score(units, out, meta["size"], led, pin)
    meta["scenarios"] = len(units)
    meta["xfx_banner"] = next((s["xfx_banner"] for s in scen if s["xfx_banner"]), None)
    meta["leaked_tmux_servers"] = sum(
        (read_status(os.path.join(out, "runs", u.name, side, "status.json")) or {}).get("leaked_tmux_servers", 0)
        for u in units
        for side in ("fx", "xfx")
    )
    block = {"coverage": cov, "surfaces": surfaces(inv, covered, scen)}
    write_reports(out, meta, scen, pairs, usage, block, led, red, say)
    return 0


def rescore(args, red, say):
    out = os.path.realpath(args.out)
    try:
        with open(os.path.join(out, "run.json")) as f:
            meta = json.load(f)
    except (OSError, ValueError):
        raise Refusal("--out holds no earlier sweep (run.json)")
    led = ledgermod.load(args.ledger, REPO)
    return finish(out, meta, led, red, say)


def parse_args(argv):
    p = argparse.ArgumentParser(prog="sweep.sh", description="fx/xfx parity sweep")
    p.add_argument("--fx")
    p.add_argument("--pin-sha")
    p.add_argument("--xfx", default=os.path.join(REPO, "target", "debug", "xfx"))
    p.add_argument("--size", default="40x120")
    p.add_argument("--jobs", type=int, default=8)
    p.add_argument("--only", action="append")
    p.add_argument("--out", required=True)
    p.add_argument("--ledger", default=os.path.join(HERE, "parity-ledger.toml"))
    p.add_argument("--scenarios", default=os.path.join(HERE, "scenarios"))
    p.add_argument("--tmp", default=os.environ.get("TMPDIR") or "/tmp")
    p.add_argument("--timeout-scale", type=float, default=1.5)
    p.add_argument("--timeout-slack", type=float, default=60)
    p.add_argument("--inventory-only", action="store_true")
    p.add_argument("--rescore", action="store_true")
    args = p.parse_args(argv)
    if not args.rescore and (not args.fx or not args.pin_sha):
        p.error("--fx and --pin-sha are required")
    if not re.fullmatch(r"\d+x\d+", args.size):
        p.error("--size is <rows>x<cols>")
    if args.jobs < 1:
        p.error("--jobs is at least 1")
    for k in ("fx", "xfx", "scenarios", "tmp", "ledger"):
        if getattr(args, k):
            setattr(args, k, os.path.abspath(getattr(args, k)))
    return args


def main(argv):
    args = parse_args(argv)
    red = Redactor(
        {
            os.path.realpath(args.out): "<out>",
            REPO: "<repo>",
            args.fx or "": "<fx-bin>",
            args.xfx: "<xfx-bin>",
            args.tmp: "<tmp>",
        }
    )
    lock = threading.Lock()

    def say(line):
        with lock:
            print(red(line), flush=True)

    try:
        return rescore(args, red, say) if args.rescore else start(args, red, say)
    except (Refusal, ledgermod.LedgerError, inventory.InventoryError) as e:
        print("sweep: %s" % red(str(e)), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

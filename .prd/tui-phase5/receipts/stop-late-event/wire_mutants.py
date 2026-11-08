"""The checker's own mutants for STOP-LATE-EVENT: each changes one wire of a
copy of this receipt the way a defect would, and `osc.py` must fail on it.

usage: python3 -I -B wire_mutants.py <receipt-dir> <scratch-dir> <sha7>

Exit 0 when `osc.py` passes the unchanged copy on `stopnext` and fails every
mutant.
"""
import os
import shutil
import subprocess
import sys

receipt, scratch, sha = sys.argv[1:4]
here = os.path.dirname(os.path.realpath(__file__))
SCENARIO = "p5-lifecycle-osc-stopnext"
UNIT = SCENARIO + "-40x120"
SECOND = os.path.join("xfx", UNIT, "xfx-%s-s01-second.wire" % UNIT)
EXIT = os.path.join("xfx", UNIT, "xfx-%s-s03-exit.wire" % UNIT)
FX_RUN2 = os.path.join("fx-run2", UNIT, "fx-%s-s01-second.wire" % UNIT)
IDLE = b"\x1b]7501;state=idle:app=xfx\x1b\\"
WORKING = b"\x1b]7501;state=working:app=xfx\x1b\\"
ERROR = b"\x1b]7501;state=error:app=xfx\x1b\\"
FX_WORKING = b"\x1b]7501;state=working:app=fx\x1b\\"
FRAME_END = b"\x1b[?2026l"


def the_stop(data):
    """Where the stop's `idle` ends: the second `idle` on the wire."""
    first = data.find(IDLE)
    second = data.find(IDLE, first + 1)
    assert second > 0
    return second + len(IDLE)


def working_behind_the_thinking_frame(data, working=WORKING):
    """Round 1's order: the prompt's `working` after the frame that first
    shows its `Thinking` (or, on fx, its activity)."""
    at = data.find(working, the_stop(data) if working == WORKING else data.find(b"Cancelled"))
    rest = data[:at] + data[at + len(working):]
    shown = max(rest.find(b"Thinking", at), rest.find(b"Generating", at))
    end = rest.find(FRAME_END, shown) + len(FRAME_END)
    return rest[:end] + working + rest[end:]


def the_stopped_turn_reported_as_its_failure(data):
    """Item 1's wire: `error` after the stop's `idle`."""
    at = the_stop(data)
    return data[:at] + ERROR + data[at:]


def the_stopped_turn_working_again_then_failed(data):
    """Item 1's wire with a call running: `working`, `error` after the stop's `idle`."""
    at = the_stop(data)
    return data[:at] + WORKING + ERROR + data[at:]


MUTANTS = [
    ("the prompt's working behind the frame that shows its Thinking (round 1's order)", SECOND,
     working_behind_the_thinking_frame),
    ("the same, on the exit's wire", EXIT, working_behind_the_thinking_frame),
    ("the stopped turn's end reported as its failure", EXIT, the_stopped_turn_reported_as_its_failure),
    ("the stopped turn working again, then failed", EXIT, the_stopped_turn_working_again_then_failed),
    ("an fx run whose working follows the prompt's frame", FX_RUN2,
     lambda data: working_behind_the_thinking_frame(data, FX_WORKING)),
]


def run(copy):
    done = subprocess.run(
        [sys.executable, "-I", "-B", os.path.join(here, "osc.py"), copy, "xfx", sha, SCENARIO],
        capture_output=True, text=True,
    )
    fails = [l for l in done.stdout.splitlines() if l.startswith("FAIL")]
    return done.returncode, fails


def fresh():
    copy = os.path.join(scratch, "receipt-copy", "stop-late-event")
    shutil.rmtree(os.path.dirname(copy), ignore_errors=True)
    for side in ("fx", "fx-run2", "fx-run3", "xfx"):
        shutil.copytree(os.path.join(receipt, side), os.path.join(copy, side))
    return copy


code, fails = run(fresh())
print("== unchanged wires: exit %d, %d failing claims" % (code, len(fails)))
ok = code == 0
for title, rel, mutate in MUTANTS:
    copy = fresh()
    path = os.path.join(copy, rel)
    data = open(path, "rb").read()
    changed = mutate(data)
    assert changed != data, title
    open(path, "wb").write(changed)
    code, fails = run(copy)
    print("== mutant: %s\n   exit %d, %d failing claims" % (title, code, len(fails)))
    for line in fails[:2]:
        print("   " + line[:200])
    ok &= code != 0
shutil.rmtree(os.path.join(scratch, "receipt-copy"), ignore_errors=True)
sys.exit(0 if ok else 1)

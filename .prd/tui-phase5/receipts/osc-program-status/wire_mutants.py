"""The checker's own mutants: each changes one xfx wire of a copy of this
receipt the way a defect would, and `osc.py` must fail on it.

usage: python3 -I -B wire_mutants.py <receipt-dir> <scratch-dir> <sha7>

Exit 0 when `osc.py` passes the unchanged copy and fails every mutant.
"""
import os
import shutil
import subprocess
import sys

receipt, scratch, sha = sys.argv[1:4]
here = os.path.dirname(os.path.realpath(__file__))
UNIT = "p5-lifecycle-osc-states-40x120"
EXIT = os.path.join("xfx", UNIT, "xfx-%s-w09-exit.wire" % UNIT)
PERM = os.path.join("xfx", UNIT, "xfx-%s-w05-permission.wire" % UNIT)
SUSPEND = "p5-lifecycle-osc-suspend-40x120"
START = os.path.join("xfx", UNIT, "xfx-%s-w00-start.wire" % UNIT)
RESUMED = os.path.join("xfx", SUSPEND, "xfx-%s-s02-resumed.wire" % SUSPEND)
CLEAR = b"\x1b]7501;state=clear\x1b\\"
DONE = b"\x1b]7501;state=done:app=xfx\x1b\\"
IDLE = b"\x1b]7501;state=idle:app=xfx\x1b\\"
WORKING = b"\x1b]7501;state=working:app=xfx\x1b\\"
WORKING_WIRE = os.path.join("xfx", UNIT, "xfx-%s-w01-working.wire" % UNIT)


def drop_clear(data):
    return data.replace(CLEAR, b"")


def clear_moved_after_the_cursor(data):
    at = data.rfind(CLEAR)
    rest = data[at + len(CLEAR):]
    show = rest.find(b"\x1b[?25h") + len(b"\x1b[?25h")
    return data[:at] + rest[:show] + CLEAR + rest[show:]


def report_after_the_clear(data):
    return data + DONE


def report_inside_a_frame(data):
    at = data.rfind(b"\x1b[?2026h") + len(b"\x1b[?2026h")
    return data[:at] + DONE + data[at:]


def raw_message(data):
    start = data.find(b":msg=")
    end = data.find(b"\x1b\\", start)
    return data[:start] + b":msg=terminal echo hi" + data[end:]


def message_with_an_escape(data):
    # `a<ESC>b`, encoded: base64 on the wire, a control once decoded.
    start = data.find(b":msg=")
    end = data.find(b"\x1b\\", start)
    return data[:start] + b":msg=YRti" + data[end:]


def a_report_lost(data):
    at = data.find(b"\x1b]7501;state=error:app=xfx\x1b\\")
    return data[:at] + data[at + len(b"\x1b]7501;state=error:app=xfx\x1b\\"):]


def another_build(data):
    return data.replace(("v0.1.0-%s" % sha).encode(), b"v0.1.0-0000000")


def no_banner(data):
    # One wire with no banner at all, while every other wire carries the
    # right one: a check over the union of banners would pass it.
    return data.replace(("v0.1.0-%s" % sha).encode(), b"v0.1.0-unknown")


def unterminated_report_after_the_clear(data):
    return data + b"\x1b]7501;state=working:app=xfx"


def bel_terminated_report(data):
    at = data.find(b"\x1b]7501;state=working:app=xfx\x1b\\")
    end = at + len(b"\x1b]7501;state=working:app=xfx")
    return data[:end] + b"\x07" + data[end + 2 :]


def eight_bit_introducer_after_the_clear(data):
    return data + b"\x9d7501;state=done:app=xfx\x9c"


def launch_idle_ahead_of_its_title(data):
    # The order the unit's first two rounds wrote: `idle`, then the title.
    at = data.find(IDLE)
    title = data.rfind(b"\x1b]2;", 0, at)
    return data[:title] + IDLE + data[title:at] + data[at + len(IDLE):]


def keystroke_working_ahead_of_its_frame(data):
    # The order rounds 1 to 3 wrote: the prompt's `working`, then the frame
    # that shows its `Thinking`. Still right after a frame's `?2026l`, so only
    # the claim's which-frame half can tell.
    at = data.find(WORKING)
    frame = data.rfind(b"\x1b[?2026h", 0, at)
    return data[:frame] + WORKING + data[frame:at] + data[at + len(WORKING):]


def resume_report_ahead_of_its_frame(data):
    # The order the unit's first two rounds wrote: the resume's `idle`, then
    # the frame it repaints.
    at = data.rfind(IDLE)
    frame = data.rfind(b"\x1b[?2026h", 0, at)
    return data[:frame] + IDLE + data[frame:at] + data[at + len(IDLE):]


MUTANTS = [
    ("the exit's restore without its clear", EXIT, drop_clear),
    ("the clear moved after ?25h", EXIT, clear_moved_after_the_cursor),
    ("a report after the exit's clear", EXIT, report_after_the_clear),
    ("a report inside a frame", EXIT, report_inside_a_frame),
    ("a permission's label raw on the wire", PERM, raw_message),
    ("a permission's label decoding to a control", PERM, message_with_an_escape),
    ("the error report lost", EXIT, a_report_lost),
    ("a wire from another build", PERM, another_build),
    ("one wire with no banner", PERM, no_banner),
    ("an unterminated report after the exit's clear", EXIT, unterminated_report_after_the_clear),
    ("a report ended by BEL", EXIT, bel_terminated_report),
    ("an 8-bit introducer after the exit's clear", EXIT, eight_bit_introducer_after_the_clear),
    ("the launch's idle ahead of its title", START, launch_idle_ahead_of_its_title),
    ("a resume's report ahead of the frame it repaints", RESUMED, resume_report_ahead_of_its_frame),
    ("a keystroke's working ahead of the frame that shows it", WORKING_WIRE, keystroke_working_ahead_of_its_frame),
]


def run(copy):
    done = subprocess.run(
        [sys.executable, "-I", "-B", os.path.join(here, "osc.py"), copy, "xfx", sha],
        capture_output=True, text=True,
    )
    fails = [l for l in done.stdout.splitlines() if l.startswith("FAIL")]
    return done.returncode, fails


def fresh():
    copy = os.path.join(scratch, "receipt-copy")
    shutil.rmtree(copy, ignore_errors=True)
    for side in ("fx", "xfx"):
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

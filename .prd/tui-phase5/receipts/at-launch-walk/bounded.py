# AT-SCOPE-INTERIM's, unchanged.
"""Runs one command in its own process group, killed with everything it
started after a time limit; prints its output and exit status.

usage: python3 -I bounded.py <seconds> <command>...
"""

import os
import signal
import subprocess
import sys

limit = int(sys.argv[1])
child = subprocess.Popen(sys.argv[2:], start_new_session=True, text=True,
                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
try:
    out, _ = child.communicate(timeout=limit)
    rc = child.returncode
except subprocess.TimeoutExpired:
    os.killpg(child.pid, signal.SIGKILL)
    out, _ = child.communicate()
    rc = f"killed after {limit} s"
sys.stdout.write(out)
print(f"# exit: {rc}")

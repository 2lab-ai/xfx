"""Runs one smoke-tui scenario with the harness's pty reads shaped the way a
loaded runner shapes them -- the RED/GREEN driver for the 3b/3d flake.

    python3 read_shape.py <helpers-dir> <mode> <scenario> --binary B --faulty F --evidence E

`<helpers-dir>` is the `helpers/` directory `scripts/smoke-tui.sh` writes into
its evidence directory. `<mode>`:

  late:<s>   every read of the pty waits <s> seconds first: a harness that is
             descheduled while the child keeps writing (CI's macOS runners).
  short:<n>  every read takes at most <n> bytes: a read that ends wherever the
             kernel's buffer did, which the harness itself documents
             (`received` in scenarios.py).

Neither the scenario code nor the binary is touched: only
`pty_tui.Session.pump` is replaced, in this process. Run it under a wall-clock
limit, in its own process group, like any RED run.
"""
import os
import select
import sys
import time

helpers, mode, *rest = sys.argv[1:]
sys.path.insert(0, helpers)
import pty_tui  # noqa: E402

original = pty_tui.Session.pump
if mode.startswith("late:"):
    delay = float(mode[len("late:") :])

    def pump(self, timeout=0.05):
        time.sleep(delay)
        return original(self, timeout)

elif mode.startswith("short:"):
    most = int(mode[len("short:") :])

    def pump(self, timeout=0.05):
        ready, _, _ = select.select([self.terminal.master], [], [], timeout)
        if not ready:
            return False
        try:
            chunk = os.read(self.terminal.master, most)
        except OSError:
            return False
        if not chunk:
            return False
        self.captured.extend(chunk)
        return True

else:
    raise SystemExit("read_shape: unknown mode %r (late:<s> or short:<n>)" % mode)

pty_tui.Session.pump = pump
sys.argv = [os.path.join(helpers, "scenarios.py")] + rest
import scenarios  # noqa: E402

sys.exit(scenarios.main())

"""The M1 receipt's cell-by-cell comparison, promoted to
`scripts/fxcap/cells.py` for the parity sweep. This copy runs that file, so
the receipt's command line and its output are what they were:

usage: python3 -I cells.py <fx.ansi> <xfx.ansi>

Row 1 (each product's banner) is left out and `/.fx/` in the fx file is read
as `/.xfx/`, as before. `scripts/fxcap/test-sweep.sh` runs this copy over the
receipt's pairs.
"""

import os
import runpy

here = os.path.dirname(os.path.realpath(__file__))
runpy.run_path(
    os.path.join(here, "..", "..", "..", "..", "scripts", "fxcap", "cells.py"),
    run_name="__main__",
)

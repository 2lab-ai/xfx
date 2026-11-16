#!/usr/bin/env bash
# Train 16: train 15's scenarios.sh, its join checks' scenarios this
# train's: join check 1's (PERMISSION-MODE-CASE's three and SLASH-ARG-SKILLS'
# own, which holds `permissions-full-enter`), join check 2's (the amendment
# and the decoder's: `approval*`, `p5-approval*`, `p5-super-arrows*`,
# `p5-modified-*`, `p5-cancel*`, `cancel*`) and join check 3's (the profile
# and slash lines' and the settings': `profile*`, `p5-profile*`,
# `p5-panels*`, `settings*`, `slash*`, `p5-slash*`). Nothing else changed.
# Train 15 (new), join checks 1 and 4 read in the sweeps: scenarios.py over
# the base (rescored) and the head, for J1's globs (`approval*`,
# `p5-approval*`, `p5-super-arrows-approval`) and J4's scenarios (the two
# re-pointed causes' own, and the unit's split scenario) with the two causes
# named. Output: scenarios.txt, through redacted.py.
# usage: scenarios.sh <scratch>   (its ref-base and sw-head)
set -u
T=$1
here=$(cd "$(dirname "$0")" && pwd)
{ echo "# join check 1: /permissions full, PERMISSION-MODE-CASE's scenarios and SLASH-ARG-SKILLS' own"
  python3 -I -B "$here/scenarios.py" "$T/ref-base" "$T/sw-head" 'p5-mode-*' 'p5-slash-arg-skills'
  echo
  echo "# join check 2: the amendment's and the decoder's scenarios"
  python3 -I -B "$here/scenarios.py" "$T/ref-base" "$T/sw-head" 'approval*' 'p5-approval*' 'p5-super-arrows*' 'p5-modified-*' 'p5-cancel*' 'cancel*'
  echo
  echo "# join check 3: the profile line's, the slash line's and the settings' scenarios"
  python3 -I -B "$here/scenarios.py" "$T/ref-base" "$T/sw-head" 'profile*' 'p5-profile*' 'p5-panels*' 'settings*' 'slash*' 'p5-slash*'
} 2>&1 | python3 -I -B "$here/redacted.py" "$here/scenarios.txt" "$T=<scratch>"

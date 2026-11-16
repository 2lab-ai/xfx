#!/usr/bin/env bash
# Train 16, round 3 (the join of main e0aedbf6): ci.yml's gate job at the
# joined head has four steps more, each mirrored where ci.yml runs it on the
# macos-15 row (script_tests and bash32 both, the row this machine is):
# #101's pinned toolchain (here: the cargo and rustc this gate runs must be
# 1.99.1; nothing is installed), and #99's "The gate scripts run /bin/bash"
# (/bin/bash must be 3.2, and a directory holding a `bash` link to it goes
# first on PATH for every step after it, as GITHUB_PATH does), "Which bash
# the gate scripts run" (printed by a script that starts so) and "A gate
# script that stops early fails" (test-early-stop.sh, after the preview
# contract's test). Nothing else changed.
# Train 16: train 15's gates.sh with TEST-ENV-ISOLATION's two CI steps added
# where ci.yml runs them, after the lints: `check-env-reads.py` (every read
# of the environment clippy lets through is listed) and its `--self-test`
# (the script-test rows). ci.yml changed in nothing else that runs between
# train 15's head 866747ff and this train's (`ci-steps.txt`); sweep.yml not
# at all. Nothing else changed.
# Train 15: train 14's gates.sh, unchanged but for this line (ci.yml and
# sweep.yml did not change between main 753b0a72 and this train's head).
# Train 14: train 13's gates.sh with one step added: ci.yml's gate job gained
# `check-at-search-cost.sh` (AT-RANK-PERF), after the output-cost check, and
# it runs there here too. Nothing else changed.
# Train 13's gate (train 12's gates.sh, unchanged but for this line: ci.yml
# and sweep.yml did not change between train 12's head and this train's):
# every CI step at the checkout's head, one after another -- trains 5's to
# 11's gates.sh with LOCAL-PATHS's two CI steps (`check-no-local-paths.sh`
# in the gate job, `test-check-no-local-paths.sh` on the script-test rows),
# each where ci.yml runs it: after check-no-secrets. The whole script runs inside one hold of the machine's
# suite lock (`suite-lock.sh gates.sh ...`), so every step runs under it and
# none overlaps another of this gate's. Each step's output goes to
# <log-dir>/<n>-<name>.log; one line per step to stdout: its exit (taken on the
# line right after the command, from `$?`, never from a substitution), its
# seconds, the load at its start.
#
# usage: suite-lock.sh gates.sh <repo> <log-dir> <smoke-dir>
set -u
repo=$1 logs=$2 smoke=$3
mkdir -p "$logs" "$smoke"
cd "$repo" || exit 2
n=0 failed=0
step() {
  local name=$1; shift
  n=$((n + 1))
  local log
  log=$(printf '%s/%02d-%s.log' "$logs" "$n" "$name")
  local load start rc
  load=$(sysctl -n vm.loadavg | awk '{print $2}')
  start=$SECONDS
  "$@" > "$log" 2>&1
  rc=$?
  printf '%02d %-34s exit %d  %4ds  load %s\n' "$n" "$name" "$rc" "$((SECONDS - start))" "$load"
  [ "$rc" -eq 0 ] || failed=$((failed + 1))
}
echo "# head $(git rev-parse --short HEAD); tree clean: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
pinned() { # ci.yml's "Install the pinned toolchain": 1.99.1
  rustc --version && cargo --version && cargo clippy --version && rustfmt --version || return 1
  rustc --version | grep -q '^rustc 1\.99\.1 ' && cargo --version | grep -q '^cargo 1\.99\.1 '
}
step pinned-toolchain pinned
bash32() { # ci.yml's "The gate scripts run /bin/bash" (matrix.bash32)
  local version
  version="$(/bin/bash -c 'printf %s "$BASH_VERSION"')"
  case "$version" in
  3.2.*) echo "/bin/bash is bash $version" ;;
  *)
    echo "/bin/bash is bash $version, not 3.2" >&2
    return 1
    ;;
  esac
  mkdir -p "$logs/bash32" && ln -sf /bin/bash "$logs/bash32/bash"
}
step bash32 bash32
PATH="$logs/bash32:$PATH"
export PATH
which_bash() { # ci.yml's "Which bash the gate scripts run" (matrix.script_tests)
  # shellcheck disable=SC2016 # the script's own expansions
  printf '%s\n' '#!/usr/bin/env bash' \
    'printf "#!/usr/bin/env bash runs %s, bash %s\n" "$BASH" "$BASH_VERSION"' >"$logs/which-bash"
  chmod 755 "$logs/which-bash"
  "$logs/which-bash"
  printf 'command -v bash: %s\n' "$(command -v bash)"
  bash --version | sed -n 1p
}
step which-bash which_bash
step fmt cargo fmt --check
step clippy cargo clippy --locked --all-targets -- -D warnings
step clippy-fault cargo clippy --locked --all-targets --features fault-injection -- -D warnings
step check-env-reads python3 -I scripts/check-env-reads.py
step check-env-reads-self-test python3 -I scripts/check-env-reads.py --self-test
step test cargo test --locked --no-fail-fast
step test-fault cargo test --locked --features fault-injection --no-fail-fast
step check-xfx-identity ./scripts/check-xfx-identity.sh
step check-no-stubs ./scripts/check-no-stubs.sh
step check-no-secrets ./scripts/check-no-secrets.sh
step check-no-local-paths ./scripts/check-no-local-paths.sh
step test-check-no-local-paths ./scripts/test-check-no-local-paths.sh
step check-preview-contract ./scripts/check-preview-contract.sh
step check-tui-preflight-cost ./scripts/check-tui-preflight-cost.sh
step check-at-search-cost ./scripts/check-at-search-cost.sh
step check-usage-clock ./scripts/check-usage-clock.sh
step test-check-no-secrets ./scripts/test-check-no-secrets.sh
step test-gate-env ./scripts/test-gate-env.sh
step test-preview-contract ./scripts/test-preview-contract.sh
step test-early-stop ./scripts/test-early-stop.sh
step fxcap-test-sweep ./scripts/fxcap/test-sweep.sh
step fxcap-test-capture ./scripts/fxcap/test-capture.sh
step build-release cargo build --locked --release
step build-faulty cargo build --locked --release --features fault-injection --target-dir target/faulty
step smoke ./scripts/smoke.sh target/release/xfx "$smoke/smoke"
step smoke-tui ./scripts/smoke-tui.sh target/release/xfx --faulty target/faulty/release/xfx "$smoke/smoke-tui"
echo "# $n steps, $failed failed; tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"
exit "$failed"

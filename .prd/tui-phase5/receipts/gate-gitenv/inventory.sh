#!/usr/bin/env bash
#
# GATE-GITENV's inventory: every script CI runs (one-line and block `run:`
# steps), which of them run git, which drop GIT_*, how the two smoke scripts
# build the environment of the binary they drive, and whether anything in the
# repository installs a git hook, and whether it tracks a .gitattributes.
#
#   inventory.sh <copy>
set -uo pipefail
cd "$1" || exit 1
unset GREP_OPTIONS
for name in "${!GIT_@}"; do
	case "$name" in
	*[!A-Za-z0-9_]*) ;;
	*) unset "$name" ;;
	esac
done
printf '== head %s\n' "$(git rev-parse --short HEAD)"

printf '\n== the scripts the workflows run (one-line and block run: steps)\n'
grep -n -h -o -E '\./scripts/[A-Za-z0-9_./-]+\.sh' .github/workflows/*.yml | sed 's/^[0-9]*://' | sort -u

printf '\n== per script: lines that run git (comments and ruby/text excluded), and the GIT_* drop\n'
for f in scripts/check-*.sh scripts/test-*.sh scripts/smoke.sh scripts/smoke-tui.sh; do
	runs=$(grep -c -E '^[[:space:]]*(if !? ?|elif !? ?|[a-z_]+=\"\$\()?(GIT_[A-Z_]+=[^ ]+ )*git( |$)|[|(] *git |&& git |\$\(git ' "$f" || true)
	drop=$(grep -n -m1 -F -e 'for name in "${!GIT_@}"' "$f" | cut -d: -f1)
	printf '%-36s git lines %-3s drops GIT_* at :%s\n' "$f" "$runs" "${drop:--}"
done

printf '\n== smoke.sh: the environment of the binary it drives\n'
grep -n -E '^xfx_env\(\)|^	env -i' scripts/smoke.sh
printf '\n== smoke-tui.sh: the environment of the binary it drives\n'
grep -n -E 'Built from nothing|^        env = \{|"PATH": os.environ.get' scripts/smoke-tui.sh | head -4

printf '\n== build.rs (built by the cargo gates): its git calls\n'
grep -n -E 'fn git\(\)|env_remove|git_path\(&\[|Command::new\("git"\)|"rev-parse"|git\(\)$|git\(\)\.' build.rs

printf '\n== hooks: anything that installs or names one\n'
grep -rn -E 'core\.hooksPath|\.githooks|git/hooks' scripts .github Cargo.toml build.rs 2>/dev/null | grep -v -E 'scripts/test-gate-env\.sh' || echo "none"

printf '\n== .gitattributes: tracked anywhere in the tree (git ls-files, every depth)\n'
attrs="$(git -c core.fsmonitor=false ls-files -- .gitattributes ':(glob)**/.gitattributes')"
printf '%s tracked\n' "$(printf '%s' "$attrs" | grep -c . || true)"
[ -z "$attrs" ] || printf '%s\n' "$attrs"

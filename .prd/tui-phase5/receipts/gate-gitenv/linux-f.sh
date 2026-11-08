#!/usr/bin/env bash
#
# GATE-GITENV-F on Linux: runs-f.sh, then mutants-f.py, each in a fresh
# rust:1.99-bookworm container (root installs ruby, then su to uid 1000), on
# the clones given. Run it under suite-lock.sh; the first line it prints is
# the slot it holds.
#
#   linux-f.sh <tools dir> <runs clone> <mutants clone> <runs log> <mutants log>
set -uo pipefail
tools=$1 runs_clone=$2 mutants_clone=$3 runs_log=$4 mutants_log=$5
lock="$(bash "$tools/lock-line.sh" "$PPID")"
container() {
	local clone=$1 inner=$2
	timeout 3600 docker run --rm -e LANG=C.UTF-8 -v "$clone":/src:ro -v "$tools":/tools:ro rust:1.99-bookworm bash -c '
		set -e
		apt-get update -qq >/dev/null 2>&1 && apt-get install -y -qq ruby >/dev/null 2>&1
		useradd -m -u 1000 runner
		cp -R /src <copy> && chown -R runner <copy>
		printf "docker run rust:1.99-bookworm; root installs ruby, then su to uid 1000\n"
		su runner -c "export PATH=/usr/local/cargo/bin:\$PATH RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo LANG=C.UTF-8 TMPDIR=/tmp; '"$inner"'"
	'
}
{
	printf '%s\n' "$lock"
	container "$runs_clone" "bash /tools/runs-f.sh <copy> /tools 4151a1d9 a41695bf"
	echo "docker exit=$?"
} >"$runs_log" 2>&1
{
	printf '%s\n' "$lock"
	container "$mutants_clone" "bash /tools/mutants-f-inner.sh"
	echo "docker exit=$?"
} >"$mutants_log" 2>&1

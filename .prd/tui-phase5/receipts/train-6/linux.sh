#!/usr/bin/env bash
# Join check 5, the Linux row of CI's per-OS split: the gate scripts' own
# tests (CI's `script_tests` steps: test-gate-env.sh, which runs
# test-check-no-secrets.sh, and test-preview-contract.sh) at the train head,
# in a fresh rust:1.99-bookworm container -- root installs ruby, then uid 1000
# runs them on a copy of the mounted (read-only) clone, chowned to it -- as
# GATE-GITENV-F's linux-f.sh did. Under suite-lock.sh.
#   linux.sh <head clone>
set -uo pipefail
clone=$1
timeout 3600 docker run --rm -e LANG=C.UTF-8 -v "$clone":/src:ro rust:1.99-bookworm bash -c '
	set -e
	apt-get update -qq >/dev/null 2>&1 && apt-get install -y -qq ruby >/dev/null 2>&1
	useradd -m -u 1000 runner
	cp -R /src /home/runner/x && chown -R runner /home/runner/x
	su runner -c "export PATH=/usr/local/cargo/bin:\$PATH RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo LANG=C.UTF-8 TMPDIR=/tmp
		set -u
		cd /home/runner/x
		printf \"== platform\n\"; uname -sm; bash --version | head -1; git --version; grep --version | head -1; ruby -v; rustc --version; printf \"uid %s; head %s\n\" \"\$(id -u)\" \"\$(git rev-parse --short HEAD)\"
		for t in test-gate-env test-preview-contract; do
			start=\$(date +%s)
			./scripts/\$t.sh > \$HOME/\$t.log 2>&1 < /dev/null
			rc=\$?
			printf \"\n== scripts/%s.sh: exit %d, %d s\n\" \$t \$rc \$((\$(date +%s) - start))
			tail -4 \$HOME/\$t.log
		done
		printf \"\n== the clone afterwards: %s\n\" \"\$( [ -z \"\$(git status --porcelain)\" ] && echo clean || echo dirty)\"
	"
'
echo "docker exit=$?"

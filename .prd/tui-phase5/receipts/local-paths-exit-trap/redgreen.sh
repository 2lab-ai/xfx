#!/usr/bin/env bash
# LOCAL-PATHS-EXIT-TRAP's RED and GREEN: each early-stop case's host test,
# run with /bin/bash, on a throwaway clone at the head under test.
#
#   redgreen.sh <clone> <base> <log-dir> green
#   redgreen.sh <clone> <base> <log-dir> red [<subject>...]
#
# green runs each host once over the head's scripts. red reverts one subject
# script at a time to <base> (`git show <base>:scripts/<subject>`), runs the
# host that holds its cases, and puts the clone back. Run it on a clone only:
# a reverted subject is the script without its guard.
#
# One summary line per run, then the host's lines about its early-stop cases
# (`stopped`), each with any path in it written `<path>`; the whole log of each
# run stays in <log-dir>, out of the receipt.
set -u
clone=$1 base=$2 logs=$3 mode=$4
shift 4
mkdir -p "$logs"
cd "$clone" || exit 2

host_of() {
	case $1 in
	check-no-secrets.sh) echo test-check-no-secrets.sh ;;
	check-preview-contract.sh) echo test-preview-contract.sh ;;
	check-xfx-identity.sh | test-check-no-secrets.sh | test-preview-contract.sh) echo test-gate-env.sh ;;
	test-gate-env.sh | smoke.sh) echo test-early-stop.sh ;;
	*) return 1 ;;
	esac
}

run_host() {
	local label=$1 host=$2 log status start
	log="$logs/$label.log"
	start=$SECONDS
	/bin/bash "scripts/$host" </dev/null >"$log" 2>&1
	status=$?
	printf '%s: %s exit %d (%ds), %s\n' "$label" "$host" "$status" "$((SECONDS - start))" \
		"$(tail -n 1 "$log" | sed -E 's#/[^ ]*#<path>#g')"
	grep -E '^test-[a-z-]+: .*stopped' "$log" | sed -E 's#/[^ ]*#<path>#g; s/^/    /'
}

echo "# head $(git rev-parse --short HEAD), base $(git rev-parse --short "$base"); /bin/bash $(/bin/bash -c 'echo $BASH_VERSION')"
case $mode in
green)
	for host in test-check-no-secrets.sh test-preview-contract.sh test-gate-env.sh test-early-stop.sh; do
		run_host "green-$host" "$host"
	done
	;;
red)
	subjects=("$@")
	if [ "${#subjects[@]}" -eq 0 ]; then
		subjects=(check-no-secrets.sh check-preview-contract.sh check-xfx-identity.sh test-check-no-secrets.sh
			test-preview-contract.sh test-gate-env.sh smoke.sh)
	fi
	for subject in "${subjects[@]}"; do
		host="$(host_of "$subject")" || exit 2
		git show "$base:scripts/$subject" >"scripts/$subject"
		run_host "red-$subject" "$host"
		git checkout -q -- "scripts/$subject"
	done
	;;
esac
echo "# tree clean after: $([ -z "$(git status --porcelain)" ] && echo yes || echo no)"

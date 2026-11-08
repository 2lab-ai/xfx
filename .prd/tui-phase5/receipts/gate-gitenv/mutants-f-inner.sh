#!/usr/bin/env bash
# GATE-GITENV-F: mutants-f.py in a container on the clone at <copy>,
# then each mutant's failing cases from its log.
uname -sm; git --version; git -C <copy> log --oneline -1
python3 /tools/mutants-f.py <copy> <local-path>
echo "mutants-exit=$?"
cd <local-path> || exit 1
for f in *.log; do
	echo "== $f"
	grep -E '^test-gate-env: [a-zA-Z.-]+: |^test-gate-env: ok \(|^test-gate-env: skip|problem' "$f" | cut -c1-160
done

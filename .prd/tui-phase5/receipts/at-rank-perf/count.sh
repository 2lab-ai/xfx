#!/bin/bash
# usage: count.sh <clone> <label> <sed expression on src/tui/file_index.rs>
# One cut switched off by a sed edit in a throwaway clone, the work-count
# test run alone with its counts printed, and the file put back. r2-counts.txt
# ran it three times in one hold of the suite lock: `folder: true`,
# `crossing: true`, and both `limit = Some(..)` as `limit = None`.
c=$1 label=$2 expr=$3
: "${TMPDIR:?a short scratch directory}"
cd "$c" || exit 2
cp src/tui/file_index.rs "$TMPDIR/fi.orig"
sed -i '' "$expr" src/tui/file_index.rs
if cmp -s src/tui/file_index.rs "$TMPDIR/fi.orig"; then echo "$label: the edit changed nothing"; exit 1; fi
echo "== $label"
cargo test --locked --lib -- --exact --nocapture tui::file_index::tests::a_keystroke_places_no_more_than_its_cuts_allow 2>&1 | grep -E 'searches, ceiling|test result|^error'
cp "$TMPDIR/fi.orig" src/tui/file_index.rs

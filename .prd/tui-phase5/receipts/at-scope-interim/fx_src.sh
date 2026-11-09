#!/bin/bash
# Prints every fx line this receipt cites, at the pin, each range under its
# path:lines@rev. usage: fx_src.sh <fx-upstream checkout>
set -euo pipefail
fx=$1
show() { # rev path start end what
  printf '\n== %s:%s-%s@%s -- %s\n' "$2" "$3" "$4" "$1" "$5"
  git -C "$fx" show "$1:$2" | awk -v a="$3" -v b="$4" 'NR>=a && NR<=b {printf "%5d  %s\n", NR, $0}'
}
P=481104fd
W=src/core/app/app_workspace_runtime.zig
I=src/core/workspace/file_index.zig
C=src/core/app/input_completion_runtime.zig
S=src/core/input/file_completion_state.zig
show $P $W 74 80 "the list's source is the index's readable revision; the app's epoch"
show $P $W 98 110 "invalidateScope: the epoch moves, the folder listing stops, the list is invalidated"
show $P $W 127 152 "install: any change of the access invalidates the scope and refreshes the index with the new epoch"
show $P $I 103 108 "ReadableRevision carries a scope epoch"
show $P $I 276 296 "the index's generations, roots, epoch and pending scope"
show $P $I 333 341 "currentState: .ready once a generation is adopted"
show $P $I 352 406 "ensureScopeEpoch and startLoad: a walk takes the epoch it starts with (386) and clears the failure (389)"
show $P $I 409 461 "joinThreadIfDone: adopted only whole (425), the pending scope started after any end"
show $P $I 462 524 "refresh, refreshScopeEpoch, refreshOwnedRoots (no stop: a running thread gets a pending scope), replacePendingRoots"
show $P $I 572 583 "readableRevision: the searchable generation's epoch"
show $P $I 647 654 "searchableGeneration: the adopted one first, else the first walk's prefix"
show $P src/main.zig 2125 2131 "isFileIndexLoading and isFileIndexFailed"
show $P $C 770 773 "fileScopeEpoch"
show $P $C 848 920 "prepareFilePicker: the refresh, the source, the epoch check (887-888), the status"
show $P $C 946 958 "autocompleteFilePickerSelection: Tab on unavailable retries"
show $P $S 130 139 "invalidate"
show $P $S 147 200 "reconcile (a changed epoch asks for the refresh, 164) and needsLookup (198-200)"
show $P $S 204 210 "stage"
show $P $S 230 233 "retry"
show $P src/core/app/app_render_runtime.zig 645 654 "the status rows: Files unavailable for an indexed list"
show $P src/core/app/app_input_runtime.zig 2724 2729 "Escape dismisses the inline picker"

#!/bin/bash
# Prints every fx v0.0.16 line this unit cites, each range under its
# path:first-last@rev, read with `git show` (the checkout is never touched).
# `fxcites.py` checks every citation in this folder's README, in the
# parity row's text this unit changed and in the source lines it added
# against these `q` ranges, so each names its file in full.
# usage: fx_src.sh <fx-upstream checkout>   (writes to stdout; fx-src.txt is its output)
set -euo pipefail
fx=$1
P=481104fd
q() { # path first last what
  printf '\n== %s:%s-%s@%s -- %s\n' "$1" "$2" "$3" "$P" "$4"
  git -C "$fx" show "$P:$1" | awk -v a="$2" -v b="$3" 'NR>=a && NR<=b {printf "%5d  %s\n", NR, $0}'
}
q src/core/app/app_entry_runtime.zig 143 156 "the native entry: runInteractiveWithDeps with cooperative = false (155)"
q src/core/app/app_entry_runtime.zig 205 217 "runInteractive (false, 209); runInteractiveCooperative (true, 216), a single-threaded host"
q src/core/app/app_entry_runtime.zig 302 343 "the launch: App.init (304), the resumed warmup (330), startFileIndex (334), then app.run() (343)"
q src/core/app/app_entry_runtime.zig 867 869 "the entry tests' app: startFileIndex records an event, walks nothing"
q src/core/app/app_entry_runtime.zig 964 973 "fx's test: a launch's order -- init, auto-upgrade, file-index, worker thread, then run"
q src/core/app/app_entry_runtime.zig 975 984 "fx's test: the cooperative launch has no file-index"
q src/core/app/app_entry_runtime.zig 1316 1325 "fx's test: a resumed launch -- init:session-123, the warmup, then file-index"
q src/main.zig 655 718 "App.init: bootstrap, applyLaunch (682-686), then the requested resume (696-713)"
q src/main.zig 2073 2078 "App.startFileIndex: after init() returns"
q src/main.zig 2125 2131 "isFileIndexLoading: currentState() == .loading"
q src/core/app/app_bootstrap_runtime.zig 438 441 "bootstrap: the workspace root and its access adopted"
q src/core/app/app_workspace_runtime.zig 20 28 "the scope: the access over the workspace root"
q src/core/app/app_workspace_runtime.zig 98 152 "invalidateScope, adopt, applyLaunch, install; startFileIndex (145-148): none for an empty root (146), ensureScopeEpoch with the app's epoch (147)"
q src/core/workspace/file_index.zig 333 341 "currentState: .idle with nothing adopted, loading or failed"
q src/core/workspace/file_index.zig 351 406 "ensureScopeEpoch (356-363): only from idle (357), none for an empty primary (358), the epoch set (361); startLoad: the cache (371-378), the walk takes the epoch (386), clears the failure (389)"
q src/core/workspace/file_index.zig 407 461 "joinThreadIfDone: adopted only whole (425), a failure dropped (432-440), the pending scope started after any end (443-451)"
q src/core/workspace/file_index.zig 462 524 "refresh, refreshScopeEpoch, refreshOwnedRoots: a running loader gets a pending scope (494-497)"
q src/core/workspace/file_index.zig 572 583 "readableRevision: the searchable generation's epoch"
q src/core/workspace/file_index.zig 647 654 "searchableGeneration: the adopted one, else a loading or ready loader; a failed one is none"
q src/core/workspace/file_index.zig 1041 1061 "activeRootsAlloc: the primary, then each active added directory, once"
q src/core/app/input_completion_runtime.zig 848 920 "prepareFilePicker: the first episode's skip (857-861), the refresh (863), the epoch check (887-888), a failure (909)"
q src/core/app/app_session_runtime.zig 2052 2068 "resumeSelectedSession: no file index call"
q src/core/app/app_session_runtime.zig 2098 2126 "forkBeforeSelectedPrompt: the fork switched to in place, no file index call"
q src/ui/event_loop.zig 76 122 "the loop: collect facts (the join) at the top, then the keys, then the frame"
q src/main.zig 174 175 "the loop's key wait: 8 ms"
q src/main.zig 2846 2853 "loopCollectFacts joins a finished loader"
q src/main.zig 2992 2998 "loopCommitFrame prepares the picker before the frame"
q src/core/app/app_bootstrap_runtime.zig 617 620 "fresh sessions only: the welcome and the assembly notice (moved with tui::open, not changed)"
q src/core/app/app_bootstrap_runtime.zig 660 690 "the launch's config and history notices (moved with tui::open, not changed)"

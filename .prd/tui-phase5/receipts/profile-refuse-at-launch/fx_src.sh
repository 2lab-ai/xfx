#!/bin/bash
# Every fx line this receipt cites, at the v0.0.16 pin 481104fd, each range
# checked against the file's length before it is printed.
# usage: fx_src.sh <fx source checkout>
set -u
g=$1 rev=481104fd bad=0
show() { # path first last why
  local total
  total=$(git -C "$g" show "$rev:$1" | wc -l | tr -d ' ')
  if [ "$3" -gt "$total" ] || [ "$2" -gt "$3" ]; then
    echo "!! $1:$2-$3 out of range ($total lines)"; bad=$((bad + 1)); return
  fi
  echo "== $1:$2-$3@$rev ($total lines) -- $4"
  git -C "$g" show "$rev:$1" | sed -n "$2,$3p" | nl -ba -v "$2"
}
L=src/core/app/app_lifecycle.zig
E=src/core/app/app_entry_runtime.zig
A=src/core/cli/cli_ask.zig
S=src/core/cli/cli_surface.zig
C=src/core/config/config_runtime.zig
T=src/core/config/settings_store.zig

echo "#### 1. The rule"
show $L 676 703 "the load, then: a bad FX_ULTRAFAST in every mode; in local mode a user-layer malformed_settings, settings_too_large or invalid_model_id refuses; durable_path_unsafe refuses unless the credential mode is .stored"
show src/core/auth/credentials.zig 21 33 "two auth modes; no FX_AUTH_MODE is local"
show src/main.zig 3277 3290 "FX_AUTH_MODE read once per process; a bad value is its own refusal"
show $C 205 219 "the diagnostic causes"

echo "#### 2. What counts: the user layer only, and how each cause arises"
show $C 463 521 "the profile file: unreadable store (cause by error), invalid JSON or non-object (malformed), oversized (too large)"
show $C 1038 1043 "a store error other than an unsafe path or open permissions is malformed_settings"
show $C 523 561 "the project file's diagnostics are layer .project: never a refusal"
show $C 563 630 "the profile's global object, then its workspaces entry for this root: both diagnostic layer .user; a non-object workspaces or entry is malformed_settings"
show $C 1009 1036 "a profile layer that does not parse: the routing salvage, then the cause (InvalidModelValue -> invalid_model_id, a few named causes, anything else malformed_settings)"
show $C 898 911 "the salvage re-parses only provider, providers and models; when that fails too the parse error itself is returned"
show $C 1611 1636 "the parser: profile-only fields for user layers, project-safe fields for all"
show $C 1645 1656 "skill_match_fuzzy is checked before the model; the model must be a string fx's validateModel takes"
show $C 1658 1662 "provider: a routing key"
show $C 1676 1686 "models: a routing key"
show $C 1733 1737 "one typed key of many: a non-bool fast_mode fails the whole layer"
show $C 1816 1836 "statusLine: a non-object one is tolerated only in a workspace entry"
show $T 17 17 "the size ceiling"
show $T 894 902 "validateModel: 1..1024 bytes, UTF-8, no surrounding whitespace, no ASCII control"
show $T 340 391 "the profile directory, read-only: opened without following a link (a link or not a directory: DurablePathUnsafe; another failure returned, so malformed_settings), then its mode: open to group or other is PrivateStatePermissionsUnsupported, a cause no launch refuses, and no store"
show $T 400 420 "loadPrimary: absent, oversized, invalid, valid"
show $T 715 749 "the settings file: opened for reading without following a link (a failed open returned before the mode is looked at), then, read-only, open to group or other is PrivateStatePermissionsUnsupported before the size and the read"
show $T 763 765 "open to group or other: mode & 0o022"
show src/core/shared/io.zig 170 180 "openExistingRegularFile: no link followed, hard links refused"
show src/core/shared/io.zig 213 286 "lstat, then the kind (anything but a regular file is DurablePathUnsafe) and the link count (more than one: DurablePathUnsafe), then the open"

echo "#### 3. Which loads refuse: the credential mode each entry passes"
show $L 420 438 "loadStartupStateForRun (ask): .refresh_if_needed; loadStartupStateWithoutCredentials (permissions, workspace, upgrade): null"
show $L 501 515 "loadInteractiveStartupState (the TUI): .stored"
show $L 517 587 "loadStartupStatus (status): reads the settings and keeps the diagnostics; no refusal"
show $A 1434 1458 "ask loads the startup state before anything else of the turn"
show $A 3954 3971 "ask's default loader: loadStartupStateForRun, local"
show $S 1080 1103 "--version and help answer before any settings are read"
show $S 1336 1357 "status: loadStartupStatus, then the config diagnostics on stderr"
show $S 1520 1553 "doctor: doctor_runtime.collect"
show src/core/cli/doctor_runtime.zig 92 105 "doctor reads the merged settings itself and reports a failure as checks"
show $S 1554 1600 "session and sessions read the store, not the settings"
show $S 1325 1335 "setup: runPasteSetup, or the host-managed result"
show $S 1927 1971 "runPasteSetup reads a pasted key and stores it: no settings are read"

echo "#### 4. The text and the status"
show $E 158 168 "a command's error: 'fx: <ErrorName>' on stderr, exit 1"
show $E 537 539 "the interactive error's text: 'fx: <ErrorName>'"
show $E 553 557 "written to stderr"
show $E 223 229 "a launch with no terminal says so first: the TTY check precedes the load"
show $E 290 300 "any other init error: reported, then returned"
show $E 302 315 "App.init failing goes to initFailure"
show src/main.zig 3301 3324 "the interactive outcome: an error is exitFast(1)"
show $A 1189 1197 "ask's notices: none for InvalidProfileConfiguration, so the error goes up"
show $A 1306 1327 "ask: in text mode an error with no notice is returned; under --json the JSON result carries the error name"
show $A 3911 3917 "the JSON result for an error"

echo "#### 5. What is done before the refusal"
show $L 851 883 "the TUI: the TTY check and the termios capture are reads; the load comes before raw mode, the resize signal and the layout query"

echo "#### 6. A profile that goes bad after launch"
show src/core/session/session_commands.zig 280 296 "the /status panel reads the settings at open and shows the error name in a row"

echo "== $bad range(s) out of bounds"

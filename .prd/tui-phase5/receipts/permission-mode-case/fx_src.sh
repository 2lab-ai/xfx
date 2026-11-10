#!/bin/bash
# Every fx line this receipt and its code cite, at the v0.0.16 pin 481104fd,
# each range checked against the file's length before it is printed
# (PROFILE-REFUSE-TYPED-KEYS's fx_src.sh, with this unit's ranges).
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
Y=src/core/shared/types.zig
C=src/core/config/config_runtime.zig
T=src/core/config/settings_store.zig
L=src/core/app/app_lifecycle.zig
D=src/core/cli/doctor_runtime.zig
K=src/core/cli/cli_ask.zig
B=src/builtins/commands.zig
R=src/core/slash_commands/command_router.zig
A=src/core/app/app_commands.zig
SC=src/core/session/session_commands.zig
PR=src/core/app/app_permission_runtime.zig
P=src/core/permissions/permissions.zig
O=src/core/output/output_contracts.zig

echo "#### 1. The parse: any case, three spellings of the third, nothing trimmed"
show $Y 2370 2383 "PermissionMode.parse: eqlIgnoreCase against ask, auto, full-access, full access, yolo"
show $C 1057 1059 "parsePermissionMode: the one wrapper every reader calls"
show $C 3149 3162 "fx's own table: ASK, Auto, YOLO, Full Access read; full, padded words, danger do not"
show $C 20 20 "the compiled default: auto"

echo "#### 2. Every reader of a mode a person wrote"
show $C 1688 1692 "the settings key (profile and workspace entry): InvalidPermissionMode when it does not parse"
show $C 1497 1500 "the startup status record reads the profile's key ..."
show $C 1563 1569 "... with the same parse (readStartupStatusPermissionMode)"
show $L 1556 1560 "FX_PERMISSION_MODE: getenv, untrimmed, parsePermissionMode, else the configured mode"
show $L 579 579 "startup (status snapshot) resolves through loadPermissionMode"
show $L 768 768 "the interactive launch resolves through loadPermissionMode"
show $D 366 370 "doctor: the same override, the same parse, the same fallback"
show $D 223 223 "doctor resolves through it"
show $T 1932 1936 "the settings validator before a write: the same parse"

echo "#### 3. /permissions: the same parse, after the router trims spaces and tabs"
show $R 92 94 "command_payload: the rest of the line trimmed of space and tab"
show $R 114 114 "/permissions takes that payload"
show $A 586 596 "commandHandlePermissions: blank opens the panel; remember/revoke; else handlePermissions"
show $SC 464 489 "handlePermissions: parsePermissionMode(rest), then reset, else the usage"
show $A 3394 3394 "the settings menu's mode goes through handlePermissions too"

echo "#### 4. The CLI flag: its own grammar, exact"
show $K 3579 3584 "ask: --auto, --full-access or --yolo (std.mem.eql); a second mode flag is InvalidAskArgs"
show $B 35 37 "ask's help: --auto, --full-access, and --yolo as its alias"
show $K 4994 5012 "fx's test: both spellings are yolo; any two mode flags are refused"
show $K 1482 1488 "the flag wins over the configured mode; full access not yet acknowledged prints the warning ..."
show $K 3675 3699 "... fx's sentence, then yolo_acknowledged saved (once per person, not per run)"
show $P 90 90 "the sentence"

echo "#### 5. No parse of a mode: Shift-Tab and the approval surface"
show $PR 127 142 "selectMode/toggleMode: a mode value, cycled ask -> auto -> yolo; no text read"
echo "== the approval files: lines naming PermissionMode, parsePermissionMode or toggleMode"
for f in src/core/app/input_approval_runtime.zig src/core/permissions/approval_amendment.zig \
  src/core/permissions/approval_decision.zig src/core/permissions/approval_prompt.zig \
  src/ui/approval_screen.zig src/ui/footer/approval_ui.zig; do
  printf '%s@%s: %s\n' "$f" "$rev" \
    "$(git -C "$g" show "$rev:$f" | grep -cE 'PermissionMode|parsePermissionMode|toggleMode')"
done

echo "#### 6. What fx writes back"
show $T 1053 1053 "a saved mode is @tagName: ask, auto, yolo -- never the spelling read"
show $PR 233 243 "the interactive selection saves the mode value"
show $P 100 115 "permissionModeLabel (the wire word) and permissionModeDisplayLabel (full access)"
show $O 765 765 "/status panel row: the display word"
show $O 839 839 "[status] permission_mode=: the display word"
show $O 944 945 "status --json: the wire word"
show $O 1806 1806 "[doctor] permission_mode=: the display word"
show $O 1851 1852 "doctor --json: the wire word"
show $D 237 240 "doctor's startup check ..."
show $D 529 535 "... with doctor's own label: full access"

echo "== $bad range(s) out of bounds"

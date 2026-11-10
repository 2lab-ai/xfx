#!/bin/bash
# Every fx line this receipt and its code cite, at the v0.0.16 pin 481104fd,
# each range checked against the file's length before it is printed.
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
C=src/core/config/config_runtime.zig
T=src/core/config/settings_store.zig
M=src/core/config/model_provider.zig
P=src/core/config/configured_provider.zig
X=src/core/config/context_limits.zig
Y=src/core/shared/types.zig
S=src/core/cli/cli_surface.zig
IO=src/core/shared/io.zig

echo "#### 1. The refusal: which user-layer causes refuse a launch"
show $L 689 703 "malformed_settings, settings_too_large and invalid_model_id refuse; durable_path_unsafe refuses unless the credential mode is .stored"

echo "#### 2. A key twice: std.json's default options refuse it, at every depth"
show $T 400 420 "loadPrimary: the profile parsed with default options (.{}); a parse error is .invalid"
show $C 463 521 "the profile: .invalid is malformed_settings for the user layer"
show $C 523 562 "the project file: parsed with default options too; a parse error is project malformed_settings, never a refusal"
show $P 109 129 "Registry.parse's precondition: the settings parser rejects duplicate keys at all depths"
show $C 357 368 "loadConfiguredProviders names the same behaviour explicitly"

echo "#### 3. The layers and the salvage"
show $C 563 632 "the global object (.profile), then this workspace's entry (.profile_workspace, tolerating a non-object statusLine); then the provider bound"
show $C 350 355 "resolve_provider_selection: the provider bound to the configured registry"
show $C 888 925 "salvageProviderRouting: provider, providers and models parsed again alone; InvalidProviderRouting when that fails"
show $C 966 1037 "mergeDetailedSettingsLayer: a parse error drops the layer; a failed salvage returns the first error (the load fails); otherwise the cause by error"
show $M 53 62 "bind: a configured name not in the registry is UnknownConfiguredProvider"
show $M 106 114 "parse: gateway, codex, grok in any case, else validate_id makes a configured name"
show $P 290 299 "validate_id: at most 64 bytes, a letter first, letters, digits, _ and -"

echo "#### 4. The parse, key by key, in its order"
show $C 1611 1636 "parseSettingsValueForLayer: providers (global only), the profile-only fields, then the project-safe ones"
show $C 1638 1860 "parseProfileOnlyFields: each key's type and grammar; the first error fails the layer"
show $C 1898 1953 "parseProjectSafeFields: max_agent_steps an integer, not negative"
show $C 265 265 "at most 32 skill authorities"
show $C 1865 1896 "parseSkillSymlinkAuthorities"
show $C 2024 2064 "parsePermissionConfig"
show $C 1061 1066 "parsePermissionAction: allow, ask, deny in any case"
show $T 894 902 "validateModel"
show $Y 2375 2382 "PermissionMode.parse: any case, no trim"
show $Y 2421 2433 "ReasoningEffort's name: 1-64 bytes of letters, digits, - _ ."
show $Y 2471 2476 "ReasoningEffort.parse: auto, adaptive, default in any case, else a name"
show $X 5 16 "the eleven context limit names"
show $X 34 41 "a name parsed exactly"
show $X 178 204 "context_limits: an object of names to an integer, off, or parseUnsigned digits"

echo "#### 5. Which surfaces stop on a failed load"
show $S 1336 1357 "status: loadStartupStatus fails, and so does the command"
show $L 517 587 "loadStartupStatus calls loadMergedSettingsDetailed"
show src/core/cli/doctor_runtime.zig 92 105 "doctor reads the settings itself and reports a failure as checks"

echo "#### 6. Cited by PROFILE-REFUSE-AT-LAUNCH, quoted here (its opus r4 NIT 2)"
show $C 1415 1429 "readOptionalFile: openExistingRegularFile, then the kind and size of what was opened"
show $IO 255 265 "openExistingRegularFile: the open, fstat, kind and links, then makeFileBlocking"
show $IO 289 315 "makeFileBlocking: F_GETFL and F_SETFL without NONBLOCK; a failure is FileControlFailed"

echo "#### 7. Not copied: the /status panel's reader"
show $C 1283 1308 "loadMergedSettingsFromPaths: no salvage and no cause classes; any parse error is the panel's error"

echo "#### 8. Round 2: the connections, the preference cap, the limits, doctor, setup"
P2=src/core/config/model_preferences.zig
show $P 6 9 "max_providers 32, max_id_bytes 64, max_model_bytes 1024"
show $T 18 18 "settings_store's max_model_bytes, validateModel's limit: 1024"
show $Y 2414 2414 "ReasoningEffort's max_name_bytes: 64"
show $P2 1 52 "max_preferences 35: putOwned (one per provider name, the 36th refused) and mergeOwnedFrom (two layers merged, the same cap)"
show $P 105 130 "Registry.parse: not an object is InvalidObject, more than 32 is LimitExceeded, then each definition"
show $P 166 177 "parse_definition's first checks: the id, the fields (a definition that is not an object is InvalidObject), the protocol"
show $C 1057 1059 "parsePermissionMode: PermissionMode.parse, any case"
show src/core/cli/doctor_runtime.zig 244 251 "formatConfigLoadFailure: '<prefix>: <ErrorName>'"
show $S 1925 1940 "runPasteSetup: the keychain first, then a pasted key; no settings are read"

echo "== $bad range(s) out of bounds"

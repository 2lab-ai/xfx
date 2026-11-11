#!/bin/bash
# Every fx line this receipt and its code cite, at the v0.0.16 pin 481104fd,
# and the Zig standard library's `std.json` at 0.17.0, the version fx builds
# with (`build.zig.zon`'s `minimum_zig_version`, the CI's `version: "0.17.0"`):
# each range checked against the file's length before it is printed.
# usage: fx_src.sh <fx source checkout> [<directory holding Zig 0.17.0's lib/std/json>]
#   Without the second argument the three std.json files are fetched from the
#   tag into $TMPDIR; either way each must have the sha256 below.
set -u
g=$1 rev=481104fd bad=0
z=${2:-}
ZIG_TAG=0.17.0
SHA_dynamic=37b8c7e934cf2887b6677ee345e6661a78af8ee4dcdc3225ecd563b352b23fa0
SHA_Scanner=6668cf992d2d3a0c7a680bdbe76e53534ed843f26b3b07367fc95082b956f563
SHA_static=6a5ebd3c69f615353fbc8df0fe7b439ffaa4418dad5ee3927e2452454ea0611e
if [ -z "$z" ]; then
  z=$(mktemp -d "${TMPDIR:-/tmp}/zigjson.XXXXXX")
  for f in dynamic Scanner static; do
    curl -sfL -o "$z/$f.zig" "https://codeberg.org/ziglang/zig/raw/tag/$ZIG_TAG/lib/std/json/$f.zig" \
      || { echo "!! could not fetch $f.zig"; exit 2; }
  done
fi
for f in dynamic Scanner static; do
  want=$(eval echo "\$SHA_$f")
  got=$(shasum -a 256 "$z/$f.zig" | cut -d' ' -f1)
  [ "$got" = "$want" ] || { echo "!! $f.zig is not $ZIG_TAG's ($got)"; exit 2; }
  echo "std/json/$f.zig@$ZIG_TAG sha256 $got"
done

show() { # path first last why
  local total
  total=$(git -C "$g" show "$rev:$1" | wc -l | tr -d ' ')
  if [ "$3" -gt "$total" ] || [ "$2" -gt "$3" ]; then
    echo "!! $1:$2-$3 out of range ($total lines)"; bad=$((bad + 1)); return
  fi
  echo "== $1:$2-$3@$rev ($total lines) -- $4"
  git -C "$g" show "$rev:$1" | sed -n "$2,$3p" | nl -ba -v "$2"
}
zshow() { # file first last why
  local total
  total=$(wc -l < "$z/$1.zig" | tr -d ' ')
  if [ "$3" -gt "$total" ] || [ "$2" -gt "$3" ]; then
    echo "!! std/json/$1.zig:$2-$3 out of range ($total lines)"; bad=$((bad + 1)); return
  fi
  echo "== lib/std/json/$1.zig:$2-$3@$ZIG_TAG ($total lines) -- $4"
  sed -n "$2,$3p" "$z/$1.zig" | nl -ba -v "$2"
}
C=src/core/config/config_runtime.zig
T=src/core/config/settings_store.zig
P=src/core/config/configured_provider.zig
X=src/core/config/context_limits.zig
L=src/core/app/app_lifecycle.zig

echo "#### 1. The parse: std.json.parseFromSlice into a std.json.Value, default options, both layers"
show $T 400 412 "loadPrimary: the profile, .{}; a parse error is .invalid (malformed_settings, which refuses a launch)"
show $C 523 545 "the project file: .{} too; a parse error is project malformed_settings, never a refusal"
show $C 1434 1438 "parseSettingsJsonForLayer: the same parse, for a layer read whole"
show $L 689 703 "malformed_settings refuses every launch"

echo "#### 2. The size cap is the only bound on nesting"
show $T 17 17 "the profile: 64 KiB"
show $C 19 19 "the project file: 64 KiB"
show $C 1414 1429 "readOptionalFile: a larger file is StreamTooLong (settings_too_large)"
show $P 131 145 "fx's own note: std.json.Value parses iteratively; input size bounds nesting"
zshow Scanner 1 40 "the scanner: memory O(depth), nesting depth needs allocation, unpaired surrogates refused"
zshow Scanner 50 85 "the nesting levels are a BitStack on the allocator"
zshow dynamic 76 132 "Value.jsonParse: an explicit stack of Values, no recursion"
zshow dynamic 133 183 "handleCompleteValue: a completed value goes to its container in a loop; a key twice is DuplicateField"

echo "#### 3. Numbers: an integer, a float, or the number's own text"
zshow static 19 56 "ParseOptions: duplicate_field_behavior .error and parse_numbers true by default"
zshow dynamic 16 47 "parseFromNumberSlice: past i64 or not finite as f64 (1e400) is .number_string; 1e-400 is the float 0"
zshow Scanner 1759 1762 "isNumberFormattedLikeAnInteger: -0 is a float"

echo "#### 4. No settings key takes anything but its own type"
show $C 1934 1946 "max_agent_steps and max_tool_result_bytes: anything but .integer is InvalidMaxAgentStepsType"
show $C 1645 1650 "auto_compact_percent: the same"
show $X 193 199 "context_limits: .integer or .string, else InvalidContextLimitValue"

echo "#### 5. The writers keep the number's text (not copied here: the parity row)"
show $T 1818 1847 "settings_store's writer: .number_string written as read"

[ "$bad" = 0 ] || { echo "!! $bad ranges out of range"; exit 1; }

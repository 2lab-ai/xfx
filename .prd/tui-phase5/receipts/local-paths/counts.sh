#!/usr/bin/env bash
# Files holding each shape at a revision, raw (before any exception), by the
# shapes scripts/check-no-local-paths.sh matches (round 7). The user name and
# the home path are built at run time, never written here.
#   counts.sh <repo> <rev>
set -euo pipefail
repo=$1 rev=$2
esc=$'\033'
control="${esc}\\[[0-9;:?]*[@-~]"
start="(^|[^A-Za-z0-9._~+-]|${control})"
user='[A-Za-z0-9_][A-Za-z0-9._-]*'
end='([^A-Za-z0-9._-]|$)'
mac="${start}/User[s]/${user}${end}"
linux="${start}/hom[e]/${user}${end}"
root='/tmp/claud[e]-[0-9]+'
broken="(^|[^A-Za-z0-9._~+/-]|${control})claud[e]-[0-9]+/"
folders='/var/folder[s]/'
count() { git -C "$repo" -c core.fsmonitor=false grep -a -l -E "$@" "$rev" -- | wc -l | tr -d ' '; }
printf 'macOS home\t%s\n' "$(count -e "$mac")"
printf 'Linux home\t%s\n' "$(count -e "$linux")"
printf 'harness scratch root\t%s\n' "$(count -e "$root")"
printf 'broken after tmp/\t%s\n' "$(count -e "$broken")"
printf 'macOS temporary root\t%s\n' "$(count -e "$folders")"
printf 'any shape\t%s\n' "$(count -e "$mac" -e "$linux" -e "$root" -e "$broken" -e "$folders")"
name=$(id -un)
printf 'the user name\t%s\n' "$(git -C "$repo" grep -a -l -i -F -e "$name" "$rev" -- | wc -l | tr -d ' ')"
printf 'the home path\t%s\n' "$(git -C "$repo" grep -a -l -F -e "$HOME" "$rev" -- | wc -l | tr -d ' ')"

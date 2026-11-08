#!/usr/bin/env bash
# Files holding each shape at a revision, raw (before any exception).
set -euo pipefail
repo=$1; rev=$2
esc=$'\033'
start="(^|[^A-Za-z0-9._~+>-]|${esc}\\[[0-9;:?]*[@-~])"
user='[A-Za-z0-9_][A-Za-z0-9._-]*'
count() { git -C "$repo" -c core.fsmonitor=false grep -a -l -E -e "$1" "$rev" -- | wc -l | tr -d ' '; }
printf 'macOS home\t%s\n' "$(count "${start}/User[s]/${user}/")"
printf 'Linux home\t%s\n' "$(count "${start}/hom[e]/${user}/")"
printf 'harness scratch root\t%s\n' "$(count '/tmp/claud[e]-[0-9]+')"
printf 'macOS temporary root\t%s\n' "$(count '/var/folder[s]/')"
printf 'any shape\t%s\n' "$(git -C "$repo" grep -a -l -E -e "${start}/User[s]/${user}/" -e "${start}/hom[e]/${user}/" -e '/tmp/claud[e]-[0-9]+' -e '/var/folder[s]/' "$rev" -- | wc -l | tr -d ' ')"
name=$(id -un); home=$HOME
printf 'the user name\t%s\n' "$(git -C "$repo" grep -a -l -i -F -e "$name" "$rev" -- | wc -l | tr -d ' ')"
printf 'the home path\t%s\n' "$(git -C "$repo" grep -a -l -F -e "$home" "$rev" -- | wc -l | tr -d ' ')"

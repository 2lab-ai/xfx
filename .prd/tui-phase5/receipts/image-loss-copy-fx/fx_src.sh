#!/bin/bash
# Prints every fx v0.0.16 line this receipt cites, at the pin, each range under
# its path:lines@rev.
#   usage: fx_src.sh <fx-upstream checkout>              -> fx-src.txt
#          fx_src.sh <fx-upstream checkout> --check <md>  -> every `file.zig:N` or
#            `file.zig:N-M` the file cites lies inside a range printed here for
#            that file; prints each citation outside, and exits 1 if any is.
set -euo pipefail
fx=$1
P=481104fd
ranges=()
show() { # path start end what
  ranges+=("$1 $2 $3")
  [[ ${mode:-print} == print ]] || return 0
  printf '\n== %s:%s-%s@%s -- %s\n' "$1" "$2" "$3" "$P" "$4"
  git -C "$fx" show "$P:$1" | awk -v a="$2" -v b="$3" 'NR>=a && NR<=b {printf "%5d  %s\n", NR, $0}'
}
mode=print
[[ ${2:-} == --check ]] && mode=check

[[ $mode == print ]] && printf '# fx v0.0.16 (pin %s): every line image-loss-copy-fx cites. Read with git show; never checked out.\n' "$P"

# --- R: a prompt image whose blob is gone or not a regular file: the session is unreadable
show src/core/session/session_adapter.zig 2871 2893 "decodeUser: each prompt image's blob path, or InvalidSessionFormat"
show src/core/session/session_adapter.zig 4320 4327 "the conversation is read through decodeUser"
show src/core/session_manager/api.zig 433 442 "blobPath: the blob exists; its bytes are not read"
show src/core/session_manager/session.zig 1666 1681 "blobExists: stat without following; not a regular file is NotFound"
show src/core/session_manager/storage.zig 341 346 "stat does not follow a symlink"
show src/core/session_manager/storage.zig 135 143 "openDir: opened for reading (iterate), without following a link"
show src/core/session_manager/storage.zig 400 417 "translate: a link, no permission, a wrong kind are Refused, which notFoundOr makes Io"
show src/core/app/app_entry_runtime.zig 222 224 "initFailure: the launch's refusals"
show src/core/app/app_entry_runtime.zig 278 301 "the launch: saved session is unreadable; any other error is printed by its name"
show src/core/app/app_entry_runtime.zig 537 556 "formatUnexpectedError: fx: <ErrorName>; Io is no storage fault with a sentence"
show src/core/app/app_input_runtime.zig 91 99 "the picker: anything but busy or being updated is unavailable"
show src/core/app/app_input_runtime.zig 1644 1662 "the picker's failed resume"
show src/ui/footer/resume_menu_presentation.zig 463 467 "Unable to resume this session."
show src/core/cli/cli_ask.zig 926 936 "ask --resume: the resume's error goes up"
show src/core/cli/cli_ask.zig 1189 1197 "askErrorNotice: no notice for these"
show src/core/cli/cli_ask.zig 1306 1326 "ask: no notice is return err; --json writes the error's name"
show src/core/cli/cli_ask.zig 8063 8079 "fx's test: a resume's InvalidSessionFormat is the JSON error"
show src/core/app/app_entry_runtime.zig 158 166 "runBeforeInteractive: a command's error"
show src/core/app/app_entry_runtime.zig 563 567 "tryWriteErrorMessage: fx: <ErrorName>"

# --- Q: a prompt image's bytes are read and checked when a request is built; a failure fails it
show src/core/images/image_attachments.zig 1132 1137 "unsafeSnapshotPathError"
show src/core/images/image_attachments.zig 1213 1243 "ImageFile: a blob is opened without following; imageFile: none is no file"
show src/core/images/image_attachments.zig 1260 1320 "loadVerifiedSnapshot: MissingImageSnapshot, ImageTooLarge, ImageSnapshotCorrupt"
show src/core/images/image_attachments.zig 1861 1877 "writeImageFilePartJson(WithBudget): try loadVerifiedSnapshot"
show src/gateway/vercel_protocol.zig 873 890 "the encoder writes each history image with a try"
show src/core/agent/runtime/request_image_budget.zig 94 107 "the budget before the build counts an unreadable image as 0: no error there"
show src/core/agent/runtime/orchestrator.zig 7520 7525 "the image route refuses a non-native model first"
show src/core/agent/runtime/orchestrator.zig 7712 7716 "try buildRequest"
show src/core/app/app_process_runtime.zig 62 74 "processQueuedWork: request failed, topic system, tone error"
show src/core/app/app_process_runtime.zig 99 136 "detailedErrorSummary and formatErrorBody: <context>: <detail> (<Name>), or <context>: <Name>"
show src/core/cli/cli_ask.zig 1802 1823 "ask: a turn's error goes up; --json keeps its name"

# --- H1, carried over (HISTORY-NOTICES-COPY-FX): ReplayedHistory's doc still cites the silent drop
show src/core/shared/types.zig 1667 1682 "projectProviderReplay: a replay state dropped without a word"

# --- W: before the build, an image whose file the request cannot vouch for is held back with a note
show src/core/images/image_attachments.zig 24 24 "image_too_large_notice"
show src/core/images/image_attachments.zig 805 850 "the dimension cache (a turn's; the file size is checked again on each request), attachmentDimensions, probeSnapshotDimensions, attachmentRawBytes, attachmentFitsEncodedLimit: an open or a header that fails is no size"
show src/core/images/image_data.zig 168 179 "requestMaxDimension, countRequestImages: every image of the request, a tool result's with the user's"
show src/core/images/image_data.zig 186 190 "fitsEncodedImageLimit: 5 MiB once encoded"
show src/core/images/image_attachments.zig 960 988 "writeWithheldAttachmentNotice"
show src/core/images/image_attachments.zig 1011 1045 "withholdOversizedAttachments: held back unless its size is known, within the limit, and 5 MiB encoded"
show src/core/images/image_attachments.zig 1050 1054 "savedImagePath: the blob"
show src/core/agent/runtime/request_image_budget.zig 1 22 "the request budget: 30 MiB, so a 20 MiB image (28 MiB encoded) is not held back by it alone"
show src/core/agent/runtime/request_image_budget.zig 76 92 "withholdImagesPastLimit"

# --- T: a tool's stored image that cannot be loaded is told to the model, in front of its result
show src/core/agent/runtime/orchestrator.zig 7612 7632 "the native route: stored tool images loaded, then unsafe attachments held back, before the build"
show src/core/agent/runtime/gateway_step.zig 256 283 "projectToolImageMessages: the non-native routes' retained-images note"
show src/core/agent/runtime/execution_memory.zig 975 991 "retainToolImages: one handle for every image of a result"
show src/core/agent/runtime/execution_memory.zig 993 1022 "materializeToolImages: the two notes"
show src/core/agent/runtime/execution_memory.zig 1071 1076 "prependImageNotice: the note, then the result cut to fit"
show src/core/tooling/tool_result_limits.zig 7 7 "the default result limit"
show src/core/session/result_store.zig 286 302 "loadToolImages: the handle's artifact, whole or not at all"
show src/core/session/result_store.zig 584 595 "openReaderManaged: a v2 session reads it as a blob"
show src/core/session/result_store.zig 728 742 "readStoredTextManaged: BlobNotFound is ResultHandleNotFound"
show src/core/session/session_child_store.zig 61 76 "BlobError"
show src/core/session/session_child_store.zig 832 836 "readBlob"
show src/core/session/session_adapter.zig 1134 1160 "get and blobReadFailed: NotFound, Corrupt, Io"
show src/core/session_manager/session.zig 1642 1665 "readBlob: Corrupt on the length or the hash; anything but NotFound is Io"

# Every `<name>.zig:<a>[-<b>]` in a code span of the file, and every `:<a>[-<b>]` after one in
# the same span, which belongs to the last file it named. Spans naming another revision are left.
read -r -d '' cites <<'PY' || true
import re, sys
text = open(sys.argv[1], encoding="utf-8").read()
for span in re.findall(r"`([^`]*)`", text):
    if "@" in span and "@481104fd" not in span:
        continue
    last = None
    for m in re.finditer(r"([A-Za-z_][A-Za-z0-9_]*\.zig)?:(\d+)(?:-(\d+))?", span):
        name, a, b = m.group(1), int(m.group(2)), int(m.group(3) or m.group(2))
        if name:
            last = name
        if last is not None:
            print(last, a, b)
PY

if [[ $mode == check ]]; then
  bad=0 n=0
  list=$(python3 -I -c "$cites" "$3")
  while IFS=' ' read -r file a b; do
    [[ -n $file ]] || continue
    n=$((n + 1))
    ok=0
    for r in "${ranges[@]}"; do
      read -r path lo hi <<<"$r"
      [[ $path == */"$file" ]] || continue
      ((a >= lo && b <= hi)) && { ok=1; break; }
    done
    ((ok)) || { echo "outside every range: $file:$a-$b"; bad=1; }
  done <<<"$list"
  echo "checked $n citations"
  exit $bad
fi

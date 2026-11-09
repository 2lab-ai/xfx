#!/usr/bin/env python3
"""Writes p5-image-loss-gone-fx.jsonl: fx v0.0.16's seeded session `kH1xJ5Cm-oRk`
(p5-effort-fork-v2-source.jsonl) with its first prompt carrying image #1 as a blob,
`shot.png` by its digest, every line's CRC32C resealed.

    python3 -I -B seed_fx.py <scenarios dir>

The frame is fx's: a line's checksum is CRC32C (Castagnoli) of the bytes before
`,"crc":"` (vercel-labs/fx@481104fd src/core/session_manager/log.zig:26-34, :117-125); a
blob is named by the lowercase hex SHA-256 of its bytes (schema.zig:599-612); an image in
its blob is written with `blob` and no snapshot fields (session_adapter.zig, WireImage).
Every original line's checksum is reproduced first. The blob itself is not written: the
scenario resumes a session its prompt image's blob is gone from.
"""
import hashlib
import json
import os
import sys


def crc32c(data: bytes) -> int:
    crc = 0xFFFFFFFF
    for byte in data:
        crc ^= byte
        for _ in range(8):
            crc = (crc >> 1) ^ (0x82F63B78 if crc & 1 else 0)
    return crc ^ 0xFFFFFFFF


SUFFIX = ',"crc":"'


def seal(payload: str) -> str:
    return f'{payload}{SUFFIX}{crc32c(payload.encode()):08x}"}}\n'


def main():
    scenarios = sys.argv[1]
    with open(os.path.join(scenarios, "p5-effort-fork-v2-source.jsonl"), encoding="utf-8") as handle:
        lines = handle.readlines()
    for line in lines:
        payload = line[: line.rindex(SUFFIX)]
        if seal(payload) != line:
            raise SystemExit(f"a checksum is not reproduced: {line[:80]!r}")
    with open(os.path.join(scenarios, "image-ws", "shot.png"), "rb") as handle:
        digest = hashlib.sha256(handle.read()).hexdigest()
    old = '"data":{"text":"first prompt alpha","images":[],"work_id":null}'
    new = (
        '"data":{"text":"[Image #1] first prompt alpha","images":[{"id":1,"path":"shot.png",'
        f'"media_type":"image/png","blob":"{digest}"}}],"work_id":null}}'
    )
    out = []
    changed = 0
    for line in lines:
        payload = line[: line.rindex(SUFFIX)]
        if old in payload:
            payload = payload.replace(old, new)
            json.loads(payload + "}")
            changed += 1
        out.append(seal(payload))
    if changed != 1:
        raise SystemExit(f"the first prompt was found {changed} times")
    target = os.path.join(scenarios, "p5-image-loss-gone-fx.jsonl")
    with open(target, "w", encoding="utf-8") as handle:
        handle.writelines(out)
    print(f"{len(lines)} checksums reproduced; line 6 carries image #1 as blob {digest}")


if __name__ == "__main__":
    main()

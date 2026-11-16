#!/usr/bin/env python3
"""Which 4 KiB part of the hostile paste holds the tokens a screen shows.

tests/tui.rs's hostile_paste(false): `ESC[200~ x ESC[201~` (13 bytes), then
one 512-byte line per token `K<7 digits>1\\r` and 0xff fill; paste_with_pauses
writes it in 4096-byte parts. CI's third rejected session (PR #103, the paced
case) showed K00005121 through K00005191 typed: tokens 512 to 519.

    python3 -I -B ci_part.py 512 519
"""
import sys

HEAD = len(b"\x1b[200~x\x1b[201~")
for token in map(int, sys.argv[1:]):
    start = HEAD + 512 * token
    print(f"token {token} (K{token:07}1) starts at byte {start}: part {start // 4096} "
          f"(bytes {start // 4096 * 4096}-{start // 4096 * 4096 + 4095})")
first, last = (HEAD + 512 * int(t) for t in (sys.argv[1], sys.argv[-1]))
print(f"tokens {sys.argv[1]}-{sys.argv[-1]}: parts {first // 4096}-{last // 4096}; "
      f"part 64 holds tokens {-(-(64 * 4096 - HEAD) // 512)}-{(65 * 4096 - HEAD - 1) // 512}")

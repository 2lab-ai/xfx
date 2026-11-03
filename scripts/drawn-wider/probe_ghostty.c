/*
 * How many cells libghostty-vt advances for each scalar, alone between two
 * markers: `[` scalar `]` on a fresh 100x5 terminal with grapheme clustering
 * (mode 2027) on, as herdr runs it, less the two markers.
 *
 * Reads one hexadecimal code point per line on stdin; writes `hex cells`.
 * Built by scripts/drawn-wider/sweep.sh against a libghostty-vt install
 * prefix (its include/ and lib/).
 */
#include <ghostty/vt.h>
#include <stdio.h>
#include <stdlib.h>

static size_t utf8(unsigned cp, unsigned char *o) {
  if (cp < 0x80) { o[0] = (unsigned char)cp; return 1; }
  if (cp < 0x800) { o[0] = 0xc0 | (cp >> 6); o[1] = 0x80 | (cp & 0x3f); return 2; }
  if (cp < 0x10000) {
    o[0] = 0xe0 | (cp >> 12); o[1] = 0x80 | ((cp >> 6) & 0x3f); o[2] = 0x80 | (cp & 0x3f);
    return 3;
  }
  o[0] = 0xf0 | (cp >> 18); o[1] = 0x80 | ((cp >> 12) & 0x3f);
  o[2] = 0x80 | ((cp >> 6) & 0x3f); o[3] = 0x80 | (cp & 0x3f);
  return 4;
}

int main(void) {
  char line[32];
  while (fgets(line, sizeof line, stdin)) {
    unsigned cp = (unsigned)strtoul(line, NULL, 16);
    GhosttyTerminal t = NULL;
    GhosttyTerminalOptions opt = { .cols = 100, .rows = 5, .max_scrollback = 0 };
    if (ghostty_terminal_new(NULL, &t, opt) != GHOSTTY_SUCCESS) return 1;
    ghostty_terminal_mode_set(t, ghostty_mode_new(2027, false), true);
    unsigned char buf[8];
    size_t n = 0;
    buf[n++] = '[';
    n += utf8(cp, buf + n);
    buf[n++] = ']';
    ghostty_terminal_vt_write(t, buf, n);
    uint16_t x = 0;
    ghostty_terminal_get(t, GHOSTTY_TERMINAL_DATA_CURSOR_X, &x);
    printf("%x %d\n", cp, (int)x - 2);
    ghostty_terminal_free(t);
  }
  return 0;
}

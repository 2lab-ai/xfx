/* libghostty-vt's advance for a sequence of code points between `[` and `]`
 * (mode 2027 on, as herdr runs it): one line of space-separated hex in, the
 * line and the cells out. scripts/drawn-wider/probe_ghostty.c, for clusters. */
#include <ghostty/vt.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static size_t utf8(unsigned cp, unsigned char *o) {
  if (cp < 0x80) { o[0] = (unsigned char)cp; return 1; }
  if (cp < 0x800) { o[0] = 0xc0 | (cp >> 6); o[1] = 0x80 | (cp & 0x3f); return 2; }
  if (cp < 0x10000) { o[0] = 0xe0 | (cp >> 12); o[1] = 0x80 | ((cp >> 6) & 0x3f); o[2] = 0x80 | (cp & 0x3f); return 3; }
  o[0] = 0xf0 | (cp >> 18); o[1] = 0x80 | ((cp >> 12) & 0x3f); o[2] = 0x80 | ((cp >> 6) & 0x3f); o[3] = 0x80 | (cp & 0x3f); return 4;
}
int main(void) {
  char line[256];
  while (fgets(line, sizeof line, stdin)) {
    line[strcspn(line, "\n")] = 0;
    GhosttyTerminal t = NULL;
    GhosttyTerminalOptions opt = { .cols = 100, .rows = 5, .max_scrollback = 0 };
    if (ghostty_terminal_new(NULL, &t, opt) != GHOSTTY_SUCCESS) return 1;
    ghostty_terminal_mode_set(t, ghostty_mode_new(2027, false), true);
    unsigned char buf[64]; size_t n = 0; buf[n++] = '[';
    char copy[256]; strcpy(copy, line);
    for (char *tok = strtok(copy, " "); tok; tok = strtok(NULL, " ")) n += utf8((unsigned)strtoul(tok, NULL, 16), buf + n);
    buf[n++] = ']';
    ghostty_terminal_vt_write(t, buf, n);
    uint16_t x = 0; ghostty_terminal_get(t, GHOSTTY_TERMINAL_DATA_CURSOR_X, &x);
    printf("%s|%d\n", line, (int)x - 2);
    ghostty_terminal_free(t);
  }
  return 0;
}

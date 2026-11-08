/*
 * What libghostty-vt holds on its screen after the bytes a binary wrote to its
 * terminal: a capture's `.wire` (capture.sh's `wire`, tmux's pipe-pane of
 * everything the binary wrote) written into a fresh <cols>x<rows> terminal
 * with grapheme clustering (mode 2027) on, as herdr runs it and as
 * scripts/drawn-wider/probe_ghostty.c measured each scalar.
 *
 * usage: replay_ghostty <cols> <rows> <wire> [<offset> <cols2> <rows2>]
 *
 * With the last three, the first <offset> bytes are written at <cols>x<rows>,
 * the terminal is resized to <cols2>x<rows2> (what the window resize a
 * capture's `resize` makes), and the rest is written there: <offset> is the
 * size of the `wire` taken just before the `resize`.
 *
 * Writes the active screen as text, one line per row, the way tmux's
 * capture-pane writes one: a cell's grapheme, a wide cell's second column
 * nothing, an empty cell a space, trailing spaces taken off. Then, after a
 * line `--- cells`, every row holding a cell that is not printable ASCII:
 * `row <n>:` and each such cell as `<col>:<U+XXXX[+U+YYYY...]>/<w|s|h>` --
 * one-based column, its code points, `w` a wide cell, `s` the spacer of a
 * wide cell, `h` a spacer head (a wide cell wrapped to the next row).
 *
 * Built against a libghostty-vt install prefix (its include/ and lib/):
 *   clang -O2 -I<prefix>/include -L<prefix>/lib -lghostty-vt \
 *     -Wl,-rpath,<prefix>/lib -o replay_ghostty replay_ghostty.c
 */
#include <ghostty/vt.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static size_t utf8(unsigned cp, char *o) {
  if (cp < 0x80) { o[0] = (char)cp; return 1; }
  if (cp < 0x800) { o[0] = (char)(0xc0 | (cp >> 6)); o[1] = (char)(0x80 | (cp & 0x3f)); return 2; }
  if (cp < 0x10000) {
    o[0] = (char)(0xe0 | (cp >> 12)); o[1] = (char)(0x80 | ((cp >> 6) & 0x3f));
    o[2] = (char)(0x80 | (cp & 0x3f));
    return 3;
  }
  o[0] = (char)(0xf0 | (cp >> 18)); o[1] = (char)(0x80 | ((cp >> 12) & 0x3f));
  o[2] = (char)(0x80 | ((cp >> 6) & 0x3f)); o[3] = (char)(0x80 | (cp & 0x3f));
  return 4;
}

int main(int argc, char **argv) {
  if (argc != 4 && argc != 7) { fprintf(stderr, "usage: replay_ghostty <cols> <rows> <wire> [<offset> <cols2> <rows2>]\n"); return 2; }
  int cols = atoi(argv[1]), rows = atoi(argv[2]);
  if (cols < 1 || cols > 1000 || rows < 1 || rows > 1000) { fprintf(stderr, "a size from 1 to 1000\n"); return 2; }
  long resize_at = -1;
  int cols2 = cols, rows2 = rows;
  if (argc == 7) {
    resize_at = atol(argv[4]);
    cols2 = atoi(argv[5]);
    rows2 = atoi(argv[6]);
    if (resize_at < 0 || cols2 < 1 || cols2 > 1000 || rows2 < 1 || rows2 > 1000) { fprintf(stderr, "a bad resize\n"); return 2; }
  }
  FILE *f = fopen(argv[3], "rb");
  if (!f) { perror(argv[3]); return 2; }
  GhosttyTerminal t = NULL;
  GhosttyTerminalOptions opt = { .cols = (uint16_t)cols, .rows = (uint16_t)rows, .max_scrollback = 0 };
  if (ghostty_terminal_new(NULL, &t, opt) != GHOSTTY_SUCCESS) return 1;
  ghostty_terminal_mode_set(t, ghostty_mode_new(2027, false), true);
  unsigned char buf[65536];
  size_t n;
  long written = 0;
  while ((n = fread(buf, 1, sizeof buf, f)) > 0) {
    size_t at = 0;
    if (resize_at >= written && resize_at < written + (long)n) {
      at = (size_t)(resize_at - written);
      ghostty_terminal_vt_write(t, buf, at);
      ghostty_terminal_resize(t, (uint16_t)cols2, (uint16_t)rows2, 8, 16);
    }
    ghostty_terminal_vt_write(t, buf + at, n - at);
    written += (long)n;
  }
  fclose(f);
  if (resize_at >= written && argc == 7) ghostty_terminal_resize(t, (uint16_t)cols2, (uint16_t)rows2, 8, 16);
  cols = cols2;
  rows = rows2;

  /* The text and the cell map, one row at a time. */
  char *text = malloc((size_t)cols * 64 + 1);
  char *map = malloc((size_t)cols * 96 + 64);
  char **maps = calloc((size_t)rows, sizeof *maps);
  for (int y = 0; y < rows; y++) {
    size_t len = 0, mlen = 0;
    int odd = 0;
    for (int x = 0; x < cols; x++) {
      GhosttyPoint p = { .tag = GHOSTTY_POINT_TAG_ACTIVE };
      p.value.coordinate.x = (uint16_t)x;
      p.value.coordinate.y = (uint32_t)y;
      GhosttyGridRef ref = GHOSTTY_INIT_SIZED(GhosttyGridRef);
      if (ghostty_terminal_grid_ref(t, p, &ref) != GHOSTTY_SUCCESS) { text[len++] = '?'; continue; }
      GhosttyCell cell;
      ghostty_grid_ref_cell(&ref, &cell);
      GhosttyCellWide wide = GHOSTTY_CELL_WIDE_NARROW;
      ghostty_cell_get(cell, GHOSTTY_CELL_DATA_WIDE, &wide);
      uint32_t cps[16];
      size_t count = 0;
      ghostty_grid_ref_graphemes(&ref, cps, 16, &count);
      if (wide == GHOSTTY_CELL_WIDE_SPACER_TAIL) {
        mlen += (size_t)sprintf(map + mlen, " %d:/s", x + 1);
        continue;
      }
      if (count == 0) { text[len++] = ' '; continue; }
      int plain = count == 1 && cps[0] >= 0x20 && cps[0] < 0x7f && wide == GHOSTTY_CELL_WIDE_NARROW;
      for (size_t i = 0; i < count; i++) len += utf8(cps[i], text + len);
      if (!plain) {
        odd = 1;
        mlen += (size_t)sprintf(map + mlen, " %d:", x + 1);
        for (size_t i = 0; i < count; i++) mlen += (size_t)sprintf(map + mlen, "%sU+%04X", i ? "+" : "", cps[i]);
        mlen += (size_t)sprintf(map + mlen, "/%c", wide == GHOSTTY_CELL_WIDE_WIDE ? 'w' : wide == GHOSTTY_CELL_WIDE_SPACER_HEAD ? 'h' : 'n');
      }
    }
    while (len > 0 && text[len - 1] == ' ') len--;
    fwrite(text, 1, len, stdout);
    fputc('\n', stdout);
    if (odd) {
      map[mlen] = 0;
      maps[y] = strdup(map);
    }
  }
  printf("--- cells\n");
  for (int y = 0; y < rows; y++)
    if (maps[y]) printf("row %d:%s\n", y + 1, maps[y]);
  ghostty_terminal_free(t);
  return 0;
}

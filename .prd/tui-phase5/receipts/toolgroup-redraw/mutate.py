import sys, pathlib
R = pathlib.Path(sys.argv[1])
name = sys.argv[2]
def rep(path, old, new):
    p = R / path
    s = p.read_text()
    assert s.count(old) == 1, (name, path, s.count(old))
    p.write_text(s.replace(old, new))
M = {
 # A: the redraw arm removed -- a recorded row falls through to colour matching
 "A-no-redraw": [("src/tui/grid.rs",
   "            if let Some(redraw) = self.redraw(line).cloned() {",
   "            if let Some(redraw) = self.redraw(line).cloned().filter(|_| false) {")],
 # B: the shape guard removed -- a drawing is put on a row whatever it holds
 "B-no-shape-guard": [("src/tui/grid.rs",
   "        if !same_text {\n            return 0;\n        }",
   "        let _ = same_text;")],
 # C: the block not materialized at emission -- rows as queued
 "C-no-materialize": [("src/tui/transcript.rs",
   "                Some(redraw) => ellipsized(redraw.drawn(palette.mode), self.cols),",
   "                Some(_) => ellipsized(row, self.cols),")],
 # D: the colour-matching roles restored for a group's row (T-U3-TC's)
 "D-colour-roles": [("src/tui/theme.rs",
   "            DocumentRow::ToolGroup => return None,",
   "            DocumentRow::ToolGroup => &[STATUSLINE, HINT, COMMAND, NOTICE, COMMENT],")],
 # E: a cancelled call's own line drawn again too
 "E-stopped-redrawn": [("src/tui/toolgroup.rs",
   "(kind == DocumentRow::ToolGroup).then(|| match palette.mode {",
   "(kind != DocumentRow::Plain).then(|| match palette.mode {")],
 # F: a reused row loses its drawing (kind only)
 "F-reused-row-drops-drawing": [("src/tui/frame.rs",
   "            self.target.record(line, kind(index), redraw(index));",
   "            self.target.set_kind(line, kind(index));")],
 # G: a scroll does not carry the drawing (records reset on scroll)
 "G-scroll-drops-drawing": [("src/tui/grid.rs",
   "        self.records.drain(..moved.min(self.records.len()));\n        self.records.resize(height, Record::default());",
   "        self.records.drain(..moved.min(self.records.len()));\n        self.records.resize(height, Record::default());\n        for record in &mut self.records { record.redraw = None; }")],
 # H: drawings not cut to the written width
 "H-drawings-uncut": [("src/tui/transcript.rs",
   "redraw_of(index).map(|redraw| redraw.map(|row| ellipsized(row, self.cols)))",
   "redraw_of(index).cloned()")],
 # I: the drawing for the other mode (old palette) put back
 "I-wrong-mode": [("src/tui/grid.rs",
   "moved += self.redraw_row(line, redraw.drawn(palette.mode), geometry, scratch);",
   "moved += self.redraw_row(line, redraw.drawn(match palette.mode { crate::tui::theme::Mode::Dark => crate::tui::theme::Mode::Light, crate::tui::theme::Mode::Light => crate::tui::theme::Mode::Dark }), geometry, scratch);")],

 # Round 2.
 # R: a row painted in the other mode written as it was painted
 "R-no-recolour-at-landing": [("src/tui/transcript.rs",
   "                None if painted_in != palette.mode => {",
   "                None if painted_in != palette.mode && false => {")],
 # S: a bold foreground recoloured loses its bold
 "S-recolour-drops-bold": [("src/tui/transcript.rs",
   "        (true, Some(foreground)) => format!(\"\\u{1b}[1;{foreground}\"),",
   "        (true, Some(_)) => wanted.to_string(),")],
 # U: a direct (38;2) foreground not recognised
 "U-recolour-skips-direct": [("src/tui/transcript.rs",
   "        [\"38\", \"2\", red, green, blue] => Color::Rgb(number(red)?, number(green)?, number(blue)?),",
   "")],
 # K: a row recoloured as written by the plain rule, whatever its kind
 "K-recolour-ignores-kind": [("src/tui/transcript.rs",
   "    let wanted = palette.document_retint_in(kind, colour)?;",
   "    let wanted = palette.document_retint_in(DocumentRow::Plain, colour)?;")],
 # V: a bold foreground recoloured as written keeps the old colour
 "V-recolour-skips-bold": [("src/tui/transcript.rs",
   "        Some(colour) => (true, colour),",
   "        Some(_) => return None,")],
 # M: a superseded block not coalesced
 "M-no-coalesce": [("src/tui/transcript.rs",
   "        if matches!(self.queue.back(), Some(Op::Block(..))) {\n            self.queue.pop_back();\n        }",
   "")],
 # N: coalescing whatever is at the back of the queue
 "N-coalesce-any-op": [("src/tui/transcript.rs",
   "        if matches!(self.queue.back(), Some(Op::Block(..))) {",
   "        if self.queue.back().is_some() {")],
 # O: a call's kept colouring found without its mode
 "O-memo-ignores-mode": [("src/tui/toolgroup.rs",
   ".find(|kept| kept.light == light && kept.base == base && kept.command == command)",
   ".find(|kept| kept.base == base && kept.command == command)")],
 # P: a call's kept colouring found without its run colour
 "P-memo-ignores-grey": [("src/tui/toolgroup.rs",
   ".find(|kept| kept.light == light && kept.base == base && kept.command == command)",
   ".find(|kept| kept.light == light && kept.command == command)")],
}
for path, old, new in M[name]:
    rep(path, old, new)
print("applied", name)

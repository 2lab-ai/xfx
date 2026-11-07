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
   "(kind == DocumentRow::ToolGroup).then(|| Redraw::new(dark, light))",
   "(kind != DocumentRow::Plain).then(|| Redraw::new(dark, light))")],
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
}
for path, old, new in M[name]:
    rep(path, old, new)
print("applied", name)

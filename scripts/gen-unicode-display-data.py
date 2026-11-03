"""Transcribe upstream's generated Unicode tables into Rust.

usage: python3 -I scripts/gen-unicode-display-data.py \
    <fx>/src/core/shared/unicode_display_data.zig \
    <fx>/src/core/agent/presentation/unicode_classes.zig \
    src/markdown/unicode_display_data.rs <out_classes_data.rs>

<fx> is a vercel-labs/fx checkout at the revision `src/markdown` cites. The first
output is committed as-is; the second holds the two range tables that
`src/markdown/unicode_classes.rs` carries between its own head and tail. Run it
after moving the upstream pin and diff the result -- an unchanged table is a
byte-identical file.
"""
import re
import sys

display_src = open(sys.argv[1]).read()
classes_src = open(sys.argv[2]).read()


def block(src, name):
    m = re.search(r"const %s = \[_\]\w+\{\n(.*?)\n\};" % re.escape(name), src, re.S)
    if not m:
        raise SystemExit("missing " + name)
    return m.group(1)


def ranges(src, name):
    body = block(src, name)
    found = re.findall(r"\.first = 0x([0-9A-Fa-f]+), \.last = 0x([0-9A-Fa-f]+)", body)
    lines = [l for l in body.split("\n") if l.strip()]
    assert len(found) == len(lines), (name, len(found), len(lines))
    out = [(int(a, 16), int(b, 16)) for a, b in found]
    for a, b in out:
        assert a <= b
    for (a0, b0), (a1, b1) in zip(out, out[1:]):
        assert b0 < a1, name
    return out


def emit_ranges(rust_name, rs):
    lines = ["#[rustfmt::skip]", "pub(super) const %s: [Range; %d] = [" % (rust_name, len(rs))]
    for a, b in rs:
        lines.append("    Range { first: 0x%04X, last: 0x%04X }," % (a, b))
    lines.append("];")
    return "\n".join(lines)


header = display_src.split("\n\npub const Range")[0]
assert header.startswith("//!")
max_depth = int(re.search(r"max_rgi_sequence_codepoints: usize = (\d+);", display_src).group(1))

node_body = block(display_src, "rgi_trie_nodes")
nodes = [(int(s), int(l), t == "true") for s, l, t in re.findall(
    r"\.edge_start = (\d+), \.edge_len = (\d+), \.terminal = (true|false)", node_body)]
assert len(nodes) == len([l for l in node_body.split("\n") if l.strip()])
edge_body = block(display_src, "rgi_trie_edges")
edges = [(int(c, 16), int(ch)) for c, ch in re.findall(r"\.codepoint = 0x([0-9A-Fa-f]+), \.child = (\d+)", edge_body)]
assert len(edges) == len([l for l in edge_body.split("\n") if l.strip()])

# The packing upstream does at comptime (display_width.zig:46-111).
alphabet = []
ids = []
for index, (codepoint, child) in enumerate(edges):
    assert child == index + 1, "child numbering is no longer implicit"
    if codepoint not in alphabet:
        alphabet.append(codepoint)
    ids.append(alphabet.index(codepoint))
assert len(alphabet) <= 256
packed_nodes = []
for start, length, terminal in nodes:
    assert start < (1 << 13) and length < (1 << 8)
    packed_nodes.append(start | (length << 13) | ((1 if terminal else 0) << 21))
# Edges within a node must be sorted for the binary search.
for start, length, _ in nodes:
    cps = [edges[i][0] for i in range(start, start + length)]
    assert cps == sorted(cps) and len(set(cps)) == len(cps)


def emit_array(rust_name, ty, values, per_line, fmt):
    lines = ["#[rustfmt::skip]", "pub(super) const %s: [%s; %d] = [" % (rust_name, ty, len(values))]
    for i in range(0, len(values), per_line):
        lines.append("    " + " ".join(fmt % v + "," for v in values[i:i + per_line]))
    lines.append("];")
    return "\n".join(lines)


out = []
out.append(header.replace("display_width.zig", "display_width.rs"))
out.append("//!")
out.append("//! Transcribed mechanically from upstream `src/core/shared/unicode_display_data.zig`")
out.append("//! at vercel-labs/fx `c1db9191`. The four range tables are copied verbatim and")
out.append("//! packed at compile time in `display_width.rs`, as upstream packs them. The RGI")
out.append("//! trie is stored already packed: upstream derives the edge alphabet, the per-edge")
out.append("//! alphabet ids, and the bit-packed nodes at comptime (`display_width.zig:46-111`);")
out.append("//! here that derivation was run once over the upstream tables and its output is")
out.append("//! what follows. A node packs `edge_start` in bits 0-12, `edge_len` in bits 13-20,")
out.append("//! and `terminal` in bit 21; edge `i` leads to node `i + 1`.")
out.append("")
out.append("/// An inclusive code point range.")
out.append("#[derive(Debug, Clone, Copy, PartialEq, Eq)]")
out.append("pub(super) struct Range {")
out.append("    pub(super) first: u32,")
out.append("    pub(super) last: u32,")
out.append("}")
out.append("")
for zig, rust in (("wide_ranges", "WIDE_RANGES"), ("emoji_presentation_ranges", "EMOJI_PRESENTATION_RANGES"),
                  ("emoji_modifier_ranges", "EMOJI_MODIFIER_RANGES"), ("variation_bases", "VARIATION_BASES")):
    out.append(emit_ranges(rust, ranges(display_src, zig)))
    out.append("")
out.append("pub(super) const MAX_RGI_SEQUENCE_CODEPOINTS: usize = %d;" % max_depth)
out.append("")
def emit_field(name, values, per_line, fmt):
    lines = ["    %s: [" % name]
    for i in range(0, len(values), per_line):
        lines.append("        " + " ".join(fmt % v + "," for v in values[i:i + per_line]))
    lines.append("    ],")
    return "\n".join(lines)


out.append("/// The RGI trie as `display_width.rs` reads it (`display_width.zig:66-70`).")
out.append("pub(super) struct RgiData {")
out.append("    /// `edge_start` in bits 0-12, `edge_len` in bits 13-20, `terminal` in bit 21.")
out.append("    pub(super) nodes: [u32; %d]," % len(packed_nodes))
out.append("    /// Per edge, its code point's index in `codepoints`.")
out.append("    pub(super) edge_codepoint_ids: [u8; %d]," % len(ids))
out.append("    /// Every code point any edge carries, in first-appearance order.")
out.append("    pub(super) codepoints: [u32; %d]," % len(alphabet))
out.append("}")
out.append("")
out.append("#[rustfmt::skip]")
out.append("pub(super) static RGI_DATA: RgiData = RgiData {")
out.append(emit_field("nodes", packed_nodes, 8, "0x%06X"))
out.append(emit_field("edge_codepoint_ids", ids, 16, "%d"))
out.append(emit_field("codepoints", alphabet, 8, "0x%05X"))
out.append("};")
out.append("")
open(sys.argv[3], "w").write("\n".join(out))

cls = []
cls.append(emit_ranges("SOURCE_SPACE_RANGES", ranges(classes_src, "source_space_ranges")).replace("pub(super) const", "const"))
cls.append("")
cls.append(emit_ranges("SOURCE_PUNCTUATION_RANGES", ranges(classes_src, "source_punctuation_ranges")).replace("pub(super) const", "const"))
cls.append("")
open(sys.argv[4], "w").write("\n".join(cls))
print("nodes", len(nodes), "edges", len(edges), "alphabet", len(alphabet), "depth", max_depth)

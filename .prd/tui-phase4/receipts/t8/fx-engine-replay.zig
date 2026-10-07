// Feeds a captured pane's bytes to fx's own terminal engine (c1db919,
// core/terminal/engine.zig) and prints the screen it ends on and how many rows
// it scrolled. A scratch harness for the T8 receipt; not part of fx or xfx.
//
// Run it from a copy of fx's src/core (this file at its top, beside
// terminal/), with the wires copied to t8wires/ under these names:
//
//   fx-stream-12.wire      <- fx-stream-wire-12x80-end.wire
//   before-stream-12.wire  <- xfx-before-stream-wire-12x80-end.wire
//   after-stream-12.wire   <- xfx-after-stream-wire-12x80-end.wire
//   before-notice-12.wire  <- xfx-before-notice-wire-12x80-end.wire
//   after-notice-12.wire   <- xfx-after-notice-wire-12x80-end.wire
//
// and `zig test fx-engine-replay.zig` (zig 0.16). Its output for this
// receipt is fx-engine-replay.txt.
const std = @import("std");
const engine = @import("terminal/engine.zig");

fn run(comptime name: []const u8, comptime rows: u16, comptime cols: u16) !void {
    const bytes = @embedFile("t8wires/" ++ name ++ ".wire");
    const alloc = std.testing.allocator;
    var grid = try engine.Grid.init(alloc, cols, rows);
    defer grid.deinit();
    var stats: engine.FeedStats = .{};
    try grid.feedWithStats(bytes, &stats);
    std.debug.print("== {s} scroll_rows={d} sync_active={}\n", .{ name, stats.scroll_rows, grid.sync_active });
    var out: std.ArrayList(u8) = .empty;
    defer out.deinit(alloc);
    var row: u16 = 1;
    while (row <= rows) : (row += 1) {
        out.clearRetainingCapacity();
        try grid.rowTextTrimmed(row, &out);
        std.debug.print("{s}\n", .{out.items});
    }
}

test "t8 wires" {
    try run("fx-stream-12", 12, 80);
    try run("before-stream-12", 12, 80);
    try run("after-stream-12", 12, 80);
    try run("before-notice-12", 12, 80);
    try run("after-notice-12", 12, 80);
}

"""fx v0.0.16's @ list height over terminal rows x composer rows x a waiting
prompt's banner, and its item rows with and without a status line, from a
line-for-line transcription of `activeListPickerReservedRows`,
`filePickerReservedRows` and `filePickerItemRows`
(`src/ui/footer/picker_presentation.zig:647-672@481104fd`, in fx-src.txt).
`-|` is Zig's saturating subtraction. usage: python3 -I height_table.py [--rust]
"""

import sys

MAX_MODEL_PICKER_ROWS = 6  # input_presentation.zig:25, list_window.zig:3
MAX_FILE_PICKER_ROWS = 10  # picker_presentation.zig:656


def sub(a, b):
    return max(a - b, 0)


def active_list_picker_reserved_rows(terminal_rows, input_extra, banner_rows):
    fixed_rows = 5 + input_extra + banner_rows
    available_rows = sub(terminal_rows, fixed_rows)
    if available_rows == 0:
        return 1
    return min(MAX_MODEL_PICKER_ROWS, available_rows)


def file_picker_reserved_rows(terminal_rows, input_extra, banner_rows):
    fixed_rows = 5 + input_extra + banner_rows
    minimum_transcript_rows = 5
    grown = min(MAX_FILE_PICKER_ROWS, sub(sub(terminal_rows, fixed_rows), minimum_transcript_rows))
    return max(active_list_picker_reserved_rows(terminal_rows, input_extra, banner_rows), grown)


def file_picker_item_rows(picker_rows, has_status):
    return sub(picker_rows, int(has_status and picker_rows > 1))


ROWS = list(range(6, 25)) + [30, 40]
EXTRAS = [0, 1, 2, 3]
BANNERS = [0, 2, 3]


def main():
    # fx's own cases (picker_presentation.zig:1588-1597) hold in the transcription.
    for args, want in [((40, 0, 0), 10), ((24, 0, 0), 10), ((18, 0, 0), 8), ((16, 0, 0), 6),
                       ((12, 0, 0), 6), ((6, 0, 0), 1), ((20, 2, 4), 6)]:
        assert file_picker_reserved_rows(*args) == want, (args, want)
    if "--rust" in sys.argv:
        for banner in BANNERS:
            for extra in EXTRAS:
                cells = ", ".join(str(file_picker_reserved_rows(r, extra, banner)) for r in ROWS)
                print(f"            ({extra}, {banner}, [{cells}]),")
        return
    print("fx v0.0.16 @ list rows (filePickerReservedRows) -- rows: terminal rows; columns: composer rows past the first (input_extra)")
    for banner in BANNERS:
        print(f"\nbanner_rows = {banner}" + ("" if banner else " (no waiting prompt)") + "    item rows with a status line in parentheses (filePickerItemRows)")
        print("rows | " + " | ".join(f"extra {e:>2}" for e in EXTRAS))
        for r in ROWS:
            cells = []
            for e in EXTRAS:
                n = file_picker_reserved_rows(r, e, banner)
                cells.append(f"{n:>3} ({file_picker_item_rows(n, True):>2})")
            print(f"{r:>4} | " + " | ".join(f"{c:>8}" for c in cells))


if __name__ == "__main__":
    main()

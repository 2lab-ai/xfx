#!/usr/bin/env python3
"""Train 16: train 14's semantic.py (train 15 had no use for it), its fixes
this train's; its mechanics unchanged. The merges' semantic fixes -- text
git merged clean that does not build or that the merged tree's own checks
refuse -- each written down here, so `reproduce.sh` can redo it. A fix is a
text replaced in one file, with the number of places it must be found; any
other count stops the script, changing nothing.

Merge 3 (TEST-ENV-ISOLATION onto merge 2). The unit sits on main `32afd307`
(#88); everything newer -- trains 13 to 15, #96 (v0.2.0) and the profile
line (merge 1) -- was written before its rules. Its `clippy.toml` refuses
`std::process::Command::new` and `std::env::var_os` but where an allow lets
one through, its `tests/*.rs` all declare `mod support;`, and its
`scripts/check-env-reads.py` holds every compile-time name to a list. At the
merged tree `cargo fmt --check` and both clippy runs fail
(`check-merges.txt`, merge 3's first run), so:
- `tests/cli.rs` (the profile line, PROFILE-REFUSE-AT-LAUNCH) declared
  `#[path = "support/profiles.rs"] mod profiles;`; with the unit's `mod
  support;` (whose `mod.rs` the profile line gave `pub mod profiles;`) the
  file is loaded twice (`clippy::duplicate_mod`). It takes `use
  support::profiles;`, as `tests/interactive.rs` does.
- The profile line's three `mkfifo` children, made with
  `std::process::Command::new`: in `tests/support/profiles.rs` by
  `super::command` (the unit's one integration-test `Command`,
  `tests/support/mod.rs`, as `tests/tool_loop.rs` makes its FIFO), in
  `src/config.rs`'s tests by `crate::process::command` (as `src/image.rs`'s
  and `src/tui/shell.rs`'s tests make theirs), in rustfmt's layout.
- `src/tui/file_index.rs`' benchmark test (AT-RANK-PERF, main, train 14)
  reads `XFX_AT_BENCH_TREE` with `std::env::var_os`: it reads it through
  `env_exception!`, the unit's way for a named variable a test is given,
  and `scripts/check-env-reads.py` allows the name (ALLOWED_NAMES, beside
  the drawn-wider sweep's two) and lists it for that file (EXCEPTIONS); the
  check refuses it without both (its first run at the merge, `envreads.txt`).
  Through `process_env!` a unit test reads its case's fake process, where
  the person's tree would never be found.
- **Join MUST 1** (STATUS, the 11-11 05:33Z train split): `src/lib.rs`
  reads `env!("XFX_BUILD_ID")` (#96, `85c98501`), which build.rs sets; the
  check's COMPILE_TIME_NAMES gains it. build.rs no longer sets
  `XFX_BUILD_REVISION` (#96 folded the revision into the id), so that name
  leaves the list: left there, an `option_env!` of it would read the
  person's own environment at compile time. The build-script comment says
  the names and the function (`git_build_id`, #96's) build.rs has now.

Merge 7 (round 3: main `e0aedbf6` onto the train). TEST-ENV-ISOLATION
renamed ci.yml's toolchain step "Install the stable toolchain"; main's #101
(CI-TOOLCHAIN-PIN) kept the name "Install the pinned toolchain" and made the
step install and default to 1.99.1. git merged the two lines clean: a step
named stable that installs 1.99.1. The name is main's again.

    python3 -I -B semantic.py [--only build] <merge-number> <worktree>
"""
import sys

FIXES = {
    "3": [
        (
            "build",
            "tests/cli.rs",
            '#[cfg(unix)]\n#[path = "support/profiles.rs"]\nmod profiles;\n',
            "#[cfg(unix)]\nuse support::profiles;\n",
            1,
        ),
        (
            "build",
            "tests/support/profiles.rs",
            """            let made = xfx::process::status(std::process::Command::new("mkfifo").arg(&file))
                .expect("run mkfifo");
""",
            """            let made =
                xfx::process::status(super::command("mkfifo").arg(&file)).expect("run mkfifo");
""",
            1,
        ),
        (
            "build",
            "src/config.rs",
            """                    let made =
                        crate::process::status(std::process::Command::new("mkfifo").arg(file))
                            .expect("run mkfifo");
""",
            """                    let made = crate::process::status(crate::process::command("mkfifo").arg(file))
                        .expect("run mkfifo");
""",
            1,
        ),
        (
            "build",
            "src/config.rs",
            """        let made = crate::process::status(std::process::Command::new("mkfifo").arg(&file))
""",
            """        let made = crate::process::status(crate::process::command("mkfifo").arg(&file))
""",
            1,
        ),
        (
            "build",
            "src/tui/file_index.rs",
            'let entries = match std::env::var_os("XFX_AT_BENCH_TREE") {',
            'let entries = match env_exception!(var_os("XFX_AT_BENCH_TREE")) {',
            1,
        ),
        (
            "build",
            "scripts/check-env-reads.py",
            """    # Every integration test binary, before main: the one it was started as
""",
            """    # A tree on disk a person names for the @ search's benchmark to walk in
    # place of fx's inventory (AT-RANK-PERF's receipt), read by that one
    # unit test.
    ("src/tui/file_index.rs", ("XFX_AT_BENCH_TREE",)),
    # Every integration test binary, before main: the one it was started as
""",
            1,
        ),
        (
            "build",
            "scripts/check-env-reads.py",
            """    "XFX_DRAWN_WIDER_OUT": "the drawn-wider sweep's own",
}
""",
            """    "XFX_DRAWN_WIDER_OUT": "the drawn-wider sweep's own",
    "XFX_AT_BENCH_TREE": "the @ search benchmark's own: a tree a person names for it",
}
""",
            1,
        ),
        (
            "build",
            "scripts/check-env-reads.py",
            """    "XFX_BUILD_REVISION": "build.rs's: the revision the binary is built from",
    "XFX_BUILD_CHANNEL": "build.rs's: the channel the build claims",
""",
            """    "XFX_BUILD_ID": "build.rs's: the id the build is stamped with, its revision in it",
    "XFX_BUILD_CHANNEL": "build.rs's: the channel the build claims",
""",
            1,
        ),
        (
            "build",
            "scripts/check-env-reads.py",
            """    # workflow set (XFX_BUILD_REVISION, XFX_BUILD_CHANNEL, PROFILE), drops
    # every GIT_* name from the git it starts, and starts that git (two
    # calls: `git_path` and `git_revision`).
""",
            """    # workflow set (XFX_BUILD_ID, XFX_BUILD_CHANNEL, PROFILE), drops
    # every GIT_* name from the git it starts, and starts that git (two
    # calls: `git_path` and `git_build_id`).
""",
            1,
        ),
    ],
    "7": [
        (
            "name",
            ".github/workflows/ci.yml",
            "      - name: Install the stable toolchain\n        run: |\n          rustup toolchain install 1.99.1 ",
            "      - name: Install the pinned toolchain\n        run: |\n          rustup toolchain install 1.99.1 ",
            1,
        ),
    ],
}

args = sys.argv[1:]
only = None
if args[0] == "--only":
    only, args = args[1], args[2:]
merge, root = args
fixes = [f for f in FIXES.get(merge, []) if only is None or f[0] == only]
for kind, path, old, new, count in fixes:
    text = open(f"{root}/{path}", encoding="utf-8").read()
    found = text.count(old)
    if found != count:
        sys.exit(f"merge {merge}: {path}: {found} places, not {count} -- read by hand")
    open(f"{root}/{path}", "w", encoding="utf-8").write(text.replace(old, new))
    print(f"merge {merge}: {kind} fix: {path}: {count} place(s), {len(old.splitlines())} lines -> {len(new.splitlines())}: {(old.splitlines() or [""])[0].strip()!r} ...")
print(f"# merge {merge}: {len(fixes)} fixes" + (f" ({only} only)" if only else ""))

# PROFILE-JSON-LIMITS -- receipt

A settings file holding a number past `f64`'s range, or nested 128 containers
deep, starts fx v0.0.16 (Zig `std.json`) and locked xfx out at every launch
(serde_json). xfx now reads each settings layer as fx's parse reads it.

- Branch `fix/p5-profile-json-limits`, base `f74d2fd5` (PERMISSION-MODE-CASE's
  head). Commits: `e0ad88a9` (the fix), `c70991ae` (one test pinned to
  serde_json's bound), `bbc4d657` (the parity row and this receipt's
  inputs), `78c0d164` (a test: a number past `f64` decides at every key
  what a float does). The receipt's outputs and this README follow, docs
  only. Every xfx run named "head" below is a binary built at `78c0d164`:
  each of its 29 `status` runs says `build_revision=78c0d1640b56`
  (`xfx-head-fixtures.txt`). The base's say `f74d2fd507da`
  (`xfx-base-fixtures.txt`). The gate and the mutants ran on clones of
  `78c0d164`.
- Depth N means N containers one inside another, the settings object
  counted. serde_json reads 127 and refuses 128.

## 1. fx's rule: from source (`fx-src.txt`, by `fx_src.sh`)

`fx_src.sh` quotes fx at `481104fd` and `lib/std/json` at Zig 0.17.0, the
version fx builds with. The three std.json files are fetched from the tag and
checked against the sha256s the script pins.

- **The parse** (fx-src section 1): both layers are
  `std.json.parseFromSlice(std.json.Value, alloc, bytes, .{})`. That is the
  profile at `settings_store.zig:408` and the project file at
  `config_runtime.zig:539`. A parse error on the profile is `.invalid`, which
  is `malformed_settings`. That refuses every launch
  (`app_lifecycle.zig:697-700`). On the project file it is a diagnostic and
  never a refusal.
- **Depth** (section 2): there is no limit. `Value.jsonParse` keeps its open
  containers in an allocated `Array` and closes them in a loop
  (`dynamic.zig:76-183@0.17.0`). The scanner tracks nesting in a heap
  `BitStack` (`Scanner.zig:50-85`), and its header says nesting "requires
  memory allocation" (`:39`). fx's own note says the same:
  `configured_provider.zig:133`. The only bound is the 64 KiB cap
  (`settings_store.zig:17`, `config_runtime.zig:19`, `:1414-1429`).
- **Numbers** (section 3): `parseFromNumberSlice` (`dynamic.zig:30-47`)
  reads an integer-formatted number as an `i64`. On overflow it keeps the
  text, `.number_string`. Anything else is parsed as an `f64`: finite, it is
  a `.float`, which makes `1e-400` the float `0`. Not finite (`1e400`,
  `-1e400`), it is `.number_string`. `-0` is a float
  (`Scanner.zig:1759-1762`). A key twice, at any depth, is `DuplicateField`
  by default (`static.zig:22-26`, `dynamic.zig:148-157`).
- **What reads a number** (section 4): `max_agent_steps` takes only
  `.integer` (`config_runtime.zig:1936`), so a float or a `.number_string`
  there is `InvalidMaxAgentStepsType`. `max_tool_result_bytes` and
  `auto_compact_percent` do the same. `context_limits` takes `.integer` or
  `.string` (`context_limits.zig:193-199`). In `src/core/config`, `.float`
  and `.number_string` appear only in the two writers, so to every reader a
  `.number_string` is what a `.float` is.
- **The writer** (section 5) writes a `.number_string` back as read
  (`settings_store.zig:1845`). Not copied (section 6).

## 2. fx's rule: from the oracle (`fx-fixtures.txt`, by `probe.sh`)

`probe.sh fx <oracle>` checks the oracle's sha256 (`8e6cda2e...`) before and
after the run. Each fixture runs in a scratch HOME on loopback with
`FX_AUTO_UPGRADE=0`, at `permissions`, `ask`, `status` and an interactive
launch in a private tmux pane. "Starts" means `ask` went on to its
credential message and the interactive launch was still running after 6 s.
"Refused" means each of those exited 1 with `fx: InvalidProfileConfiguration`.

| fixture (profile `~/.fx/settings.json` unless noted) | fx |
|---|---|
| `{"zzz": <depth-1 arrays>}`, depth 127, 128, 129, 1,000; 32,764 (65,535 bytes, the deepest 64 KiB holds) | starts |
| depth 100,000 (200,007 bytes) | refused; `status`: `config user: settings_too_large` |
| 1,000 objects deep; 200 deep at `zzz` in this workspace's entry | starts |
| 200 deep at `max_agent_steps`; a key twice 200 deep; torn 200 deep | refused; `status`: `config user: malformed_settings` |
| `zzz`: `1e400`, `-1e400`, 30 digits, -30 digits, 400 digits, `1e-400`, `-0` | starts |
| `max_agent_steps`: `1e400`, `-1e400`, 30 digits, `1e-400`, `-0` | refused |
| `max_agent_steps`: `9223372036854775807` | starts, `agent_step_limit=9223372036854775807` |
| `max_agent_steps: 3` beside `1e400` and a 200-deep array | starts, `agent_step_limit=3` |
| project `.fx.json`: `1e400`, or 200 deep, beside `max_agent_steps: 3` | starts, `agent_step_limit=3`, no note |
| project `.fx.json`: `max_agent_steps` `1e400`, or `5.0` | starts, `config project: malformed_settings` (layer dropped) |

## 3. What xfx does now (`src/config/layer_json.rs`)

`read_opened` (`src/config.rs`) reads each layer with `layer_json::object`
instead of `serde_json::from_str` plus the `KeysOnce` pass. That covers the
profile, the project file, the `/status` panel's check and the allowlist's
re-read. `KeysOnce` is now only a test oracle.

- **No recursion.** One loop over an explicit `Vec` of open containers. A
  syntax error or a key twice is refused at any depth, as std.json refuses
  it (`what_std_json_refuses_is_refused_at_any_depth`: 200 deep, among
  others).
- **Values keep their meaning.** Strings and numbers are decoded by
  serde_json one token at a time, so a file serde_json read reads into the
  same `Value`. The crate's serde_json features are unchanged
  (`preserve_order` only).
- **Past `f64`**, the number is the largest `f64` of its sign: a number and
  no integer. Every key xfx reads refuses it as fx refuses
  `.number_string`.
- **Past depth 127**, a container is read through and kept as an empty one
  of its kind, 128 deep. Nothing deeper is built, so no `Value` walk can
  recurse past 128.

## 4. RED -> GREEN

- **RED** (`red.txt`, by `red.sh`): the tests over the base's `src` on a
  `git clone --no-local` (of `e0ad88a9`; the tests it runs are unchanged
  since). Three fail:
  - `cli::ask_goes_on_with_a_profile_fx_starts_with` with
    `a number past f64: xfx: InvalidProfileConfiguration`;
  - `config::tests::a_number_past_f64_or_a_deep_value_is_read_as_fxs_parse_reads_it`
    with `{"zzz": 1e400}`, `left: Some(Rejected)`;
  - `config::tests::a_project_file_fx_reads_is_applied` with
    `(None, [(MalformedSettings, None)])` against `(Some(3), [])`.

  `ask_refuses_a_profile_fx_refuses` passes at the base too: those files
  were refused already and must stay refused. The `layer_json` tests and the
  two `no_key_xfx_reads_*` tests call code the base does not have.
- **Oracle pairs**, the same fixtures (`outcomes.py`):
  - base against fx (`outcomes-base.txt`): 10 fixtures that start fx are
    refused at every xfx launch. They are depth 128, 129, 1,000, 32,764, 1,000
    objects, the entry's, `1e400`, `-1e400`, 400 digits, and `steps-ok`. The
    project file's `max_agent_steps: 3` is dropped as `malformed_settings`
    twice (`proj-big`, `proj-deep`).
  - head against fx (`outcomes-head.txt`): **29 of 29 match on the refusal
    and on how every entry ended**, and 28 of 29 on the step limit. The
    differences left are older than this unit:
    - `steps-i64-max`: xfx keeps the step limit in a `u32`.
    - `proj-steps-big` and `proj-steps-float`: xfx notes a bad key in the
      project file (`invalid_value; key=max_agent_steps`) where fx drops the
      layer (`malformed_settings`). The `5.0` control shows the same split at
      the base.

    Both are on the parity row.
- **Unit tests** (`unit-tests.txt`, at `78c0d164`): 13 pass, 0 fail. That
  is the four `layer_json` tests, the four new `config` tests, the key-twice
  test, and the four `cli ask_*`.

## 5. Stack safety

- `a_layer_nested_at_any_depth_is_read_without_recursion` reads depth 127,
  128, 129, 1,000, 32,764 and 100,000, as arrays and as objects. It runs on
  a `std::thread::spawn` thread with the default stack (no `stack_size`).
  The same test on the recursive mutant below crashes, so that stack is
  bounded. Each read is checked for being 128 deep at most, and for
  equalling serde_json's value wherever serde_json reads it
  (`unit-tests.txt`).
- The mutant "a recursive reader" puts recursion where the explicit stack
  is. It crashes that test with a stack overflow (`mutants.txt`). So does
  "keep every level": building all 100,000 levels overflows in the `Value`'s
  own recursive drop, which is why nothing past 128 is built.
- A launch on the 32,764-deep profile starts (`xfx-head-fixtures.txt`,
  `deep-max`), and so does `ask` on it in `cli::ask_goes_on_...`.

## 6. Values xfx reads keep their meaning

- `within_serde_jsons_limits_the_reader_is_serde_jsons_parse` compares 200,000
  run-ons of JSON pieces and 50,000 well-formed documents of a few keys
  against serde_json's parse, with `KeysOnce` as the key-twice oracle. The
  counts are `{"a key twice": 18707, "past f64": 5288, "refused alike":
  157810, "same": 35181}` (`unit-tests.txt`). A file with a number past
  `f64` reads as its twin with `±1.7976931348623157e308` in that place.
- `no_key_xfx_reads_looks_past_the_depth_kept` runs every key xfx reads
  (`PROFILE_ONLY_KEYS`, `max_agent_steps`, `context_limits`, `providers`,
  `additional_directories`), in the global object and in this workspace's
  entry. A value 200 deep reaches the same decision in every reader as the
  same shape 8 deep. Only one note quotes the value it cannot read (a
  `permission` action), and what a line shows of it, 256 bytes, is the
  file's own text.
- `no_key_xfx_reads_takes_a_number_past_f64_as_other_than_a_float`: for the
  same keys, `1e400` and a 400-digit integer reach the same decisions and
  notes as `1.5`, and `-1e400` the same as `-1.5`. fx's readers treat a
  `.number_string` as a `.float`, since none takes either (section 1). A
  placeholder some key takes would fail here: `null`, which `effort`
  accepts, or `0`, which `max_agent_steps` accepts. One note shows the
  placeholder's spelling. A permission pattern's action of `1e400` refuses
  `ask` on both, as `1.5` does (fx and xfx: `InvalidProfileConfiguration`).
  But the note xfx's `doctor` adds quotes `1.7976931348623157e+308` where
  the file says `1e400` (`permission-note.txt`, at `78c0d164`). The parity
  row records it.
- `a_number_past_f64_or_a_deep_value_is_read_as_fxs_parse_reads_it`:
  - each of fx's number classes and depth 129 at an ignored key loads with no
    refusal and no diagnostic, in the global object and in the entry;
  - each at `max_agent_steps` is `Rejected`;
  - `max_agent_steps: 3` beside depth 128, 1,000 or 32,753, or beside
    `1e400` and `-1e400`, is 3.
- `a_project_file_fx_reads_is_applied`: the project file's
  `max_agent_steps: 3` beside `1e400`, `-1e400` or 200 deep is 3. A `1e400`,
  `5.0` or 200-deep value at that key is its `invalid_value`.

## 7. Mutants (`mutants.txt`, by `mutants.py`)

Each mutant is one edit on a `git clone --no-local` of `78c0d164`, run with
the tests that should see it, then undone. **22 of 22 killed**: serde_json's
parse back, the depth kept off by one either way and unbounded, the
placeholder as `0`, `null`, unsigned or refused, the number grammar
loosened four ways, a key twice taken (anywhere, or past the depth kept), an
escape one byte long, a string left undecoded, anything after the value, no
`\r` in space, a trailing comma, no colon, and a recursive reader. Each
line names the tests that failed. The recursive reader and "keep every
level" crash with a stack overflow. The `null` placeholder is caught at the
key level too (`no_key_xfx_reads_takes_a_number_past_f64_as_other_than_a_float`,
through `effort`).

## 8. Gate (`gate.txt`, by `gates.sh`)

`suite-lock.sh gates.sh` on a `git clone --no-local` of the branch at
`78c0d164`, with a short TMPDIR, the whole script under one hold of the
lock. **22 of 22 steps exit 0** on the first plain run, the tree clean
before and after. The step logs give:

- `cargo test`: 5,686 passed, 0 failed;
- the fault-injection run: 5,723 passed, 0 failed;
- `smoke.sh`: 47 checks, 0 failures;
- `smoke-tui.sh`: 37 scenarios plus the oracle, 1,167 checks, 0 failures.

The plain run had no catalog-wait failures, so there was nothing to rerun.

## 9. Parity (`docs/parity.md`, the config diagnostics row)

The row now says each layer is read as fx's `std.json` reads it: at any
depth, without recursion, with only the 64 KiB cap bounding nesting, and a
number past `f64` read as one no integer key takes. PROFILE-JSON-LIMITS is
gone from its owners. Recorded as not copied:

- A settings write still reads the file with serde_json
  (`provider/profile.rs`, `read_existing`). Over such a file it fails and says
  why (`it is not valid JSON: number out of range`), where fx's writer keeps
  the `.number_string`.
- The MCP approvals saved in the profile are read the same way
  (`mcp::config::load_choices`, `read_existing(path).ok()`). Over such a file
  none is read, where fx's `loadProjectMcpChoicesFromHome` reads them
  (`config_runtime.zig:399`).
- `max_agent_steps` past `u32` is `invalid_value` here, where fx takes it
  (`steps-i64-max`; older than this unit).
- A permission pattern's action past `f64` refuses the launch on both.
  xfx's own note quotes it as the placeholder, `1.7976931348623157e+308`
  (`permission-note.txt`).

The writer and the MCP choices fall outside "settings layers only". Making
them read like the layers needs a lossless read: the writer would write the
placeholder back.

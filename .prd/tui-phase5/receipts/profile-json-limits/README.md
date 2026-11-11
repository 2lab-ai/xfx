# PROFILE-JSON-LIMITS -- receipt

A settings file holding a number past `f64`'s range, or nested 128 containers
deep, starts fx v0.0.16 (Zig `std.json`) and locked xfx out at every launch
(serde_json). xfx now reads each settings layer as fx's parse reads it.

- Branch `fix/p5-profile-json-limits`, base `f74d2fd5` (PERMISSION-MODE-CASE's
  head). Commits: `e0ad88a9` (the fix), `c70991ae` (one test pinned to
  serde_json's bound), `bbc4d657` (the parity row and this receipt's
  inputs), `78c0d164` (a test: a number past `f64` decides at every key
  what a float does). Round 1's receipt outputs follow, docs only, to
  `4fc3b373`. Round 2 (section 10): `6a198b9d` (the code), then `45009bc7`
  and the receipt's docs.
- Round 1's "head" runs (`mutants.txt`, `gate.txt`, `unit-tests.txt`, and
  the first `xfx-head-fixtures.txt`) are of `78c0d164`. Round 2's are of
  `6a198b9d`: `xfx-head-fixtures.txt` and `outcomes-head.txt` are now its
  rerun, with each of the 29 `status` runs saying
  `build_revision=6a198b9dd056`, as are `pairs-xfx-head.txt`,
  `unit-tests-round2.txt`, `mutants-round2.txt` and `gate-round2.txt`. The
  base's runs say `f74d2fd507da` (`xfx-base-fixtures.txt`).
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
  `permission` action), and it quotes the value as read, not the file's
  text. An action that starts 4 deep is kept 125 deep with its last
  container empty, where the file has 200 levels and a `1`. That holds for
  an object chain and for an array chain as a pattern's action (125 `[`
  and as many `]`); round 2 pins both exactly. fx quotes nothing there:
  its launch refuses the file (`permission-note.txt`). [Round 1 said the
  note's first 256 bytes were the file's own text. That held only for an
  object chain and was the wrong claim: opus r1 N1, gpt6 NTH 2.]
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

## 9. Parity (`docs/parity.md`, the config diagnostics row and the MCP project servers row)

The config diagnostics row says each layer is read as fx's `std.json`
reads it: at any depth, without recursion, with only the 64 KiB cap
bounding nesting, and a number past `f64` read as one no integer key
takes. PROFILE-JSON-LIMITS is gone from its owners. Round 2 adds three
things to that row: the note quotes a value as read, bytes that are not
UTF-8 are `malformed_settings`, and the saved MCP choices are read the
same way (section 10). Recorded as not copied:

- A settings write still reads the file with serde_json
  (`provider/profile.rs`, `read_existing`). Over such a file it fails and
  says why (`it is not valid JSON: number out of range`), where fx's writer
  keeps the `.number_string`. Queued: **SETTINGS-JSON-WRITERS**, a lossless
  read for `read_existing` and its writers. The layer reader keeps neither
  value as written, so no writer may use it.
- `max_agent_steps` past `u32` is `invalid_value` here, where fx takes it
  (`steps-i64-max`; older than this unit). Queued: **AGENT-STEPS-RANGE**.
- A permission pattern's action past `f64`, or nested past 127, refuses the
  launch on both. xfx's own note quotes it as read: the placeholder
  `1.7976931348623157e+308`, or what is kept of the chain
  (`permission-note.txt`, section 6).

## 10. Round 2

Both round-1 reviews had no MUST. Round 2 takes opus r1's S1-S3 and N1-N2,
and gpt6's NTH 2 (the note).

- **S1, the saved MCP choices, read as fx reads them.**
  - fx: `loadProjectMcpChoicesFromHome` (`config_runtime.zig:384-414@481104fd`)
    parses the profile with the same default `parseFromSlice` as the layers
    (`:399`). It fails when the file does not read, when the root or
    `workspaces` is not an object (`:401-403`), or when `parseChoices`
    refuses a choice. `parseChoices` takes only an object for the entry
    (`project_config.zig:842`), lists that are arrays of non-empty strings
    (`:1339-1366`), and a bool for the switch (`:856-859`).
  - fx's three callers all go on without the workspace's servers when that
    load fails. `loadNativeConfigs` (`src/builtins/mcp.zig:58-63`) loads the
    profile's servers only and leaves a debug trace. The inspection
    (`:240-248`) lists the profile's only. ACP (`src/acp/sessions.zig:1035-1046`)
    adds no project server. Nothing reaches the user.
  - fx's oracle (`pairs-fx.txt`): an approval of `s` beside `1e400`, or 128
    deep, lists `s` as `not started`, as a plain approval does. An entry
    with no choices lists it `waiting for approval`. A list that is a
    string, a list holding `""`, or a switch `"yes"` lists no server:
    `No MCP servers configured`, `mcp_servers=0`.
  - xfx now (`src/mcp/config.rs`, `load_choices` and `fx_takes_choices`):
    - the profile is read by `config::read_profile_object`, the layer
      reader, read only;
    - the load is `None` wherever fx's fails;
    - the runtime then loads none of the workspace's servers
      (`src/mcp/runtime.rs`, `read_configuration`).

    xfx has no `mcp` command, so its side of the pair is the runtime test
    `saved_choices_are_read_as_fxs_parse_reads_them`:
    - an approval beside `1e400` or 128 deep is approved and the server
      starts;
    - the control, an entry with no choices, is read and pending;
    - a list that is a string, a switch `"yes"`, and a profile with a key
      twice leave the entry unread.

    The unit test `the_saved_choices_load_where_fxs_load_does` takes each of
    fx's failure cases one by one.
  - Where xfx still differs: an approval is bound to a digest, and the
    switch admits nothing. Both are on the MCP row already.
- **S2 and S3**: the parity row names SETTINGS-JSON-WRITERS and
  AGENT-STEPS-RANGE (section 9). There is no code for either here.
- **N1**: section 6 and the test's docstring are corrected. The test now
  pins both chains' notes exactly (`config::tests::no_key_xfx_reads_looks_past_the_depth_kept`).
- **N2, invalid UTF-8.** `read_opened` (`src/config.rs`) reads the file's
  bytes. Bytes that are not UTF-8 are `malformed_settings`, fx's parse
  error. A read that fails stays `unreadable_settings`.
  - fx (`pairs-fx.txt`): `config user: malformed_settings` and
    `config project: malformed_settings`.
  - The round-1 xfx (`pairs-xfx-round1.txt`): `unreadable_settings` in
    both, from `doctor` for the profile and `ask` for the project file.
  - The head (`pairs-xfx-head.txt`): `malformed_settings` in both.
  - The profile refuses the launch in all three.
- **RED** (`red-round2.txt`): the round-2 tests over the code at
  `4fc3b373`, with only test code changed.
  - `bytes_that_are_not_utf8_are_malformed_settings_in_either_layer` fails
    with `(false, [UnreadableSettings])` against `[MalformedSettings]`.
  - `saved_choices_are_read_as_fxs_parse_reads_them` fails with `Pending`
    against `Approved`, beside `1e400`.
  - The N1 test passes there: it pins notes the base already gave.
  - The `load_choices` unit test is written for the new
    `Option<Choices>` return, so it has no base to compile against.
- **GREEN** (`unit-tests-round2.txt`, at `6a198b9d`'s code): 17 pass and 0
  fail. That is every round-1 and round-2 test this unit added or changed,
  the `load_choices` neighbour `an_entry_rewritten_after_its_approval_was_saved_stays_pending`,
  and the four `cli ask_*`. The 29-fixture probe re-run on the round-2
  binary still matches fx at 29 of 29 on refusal and on how every entry
  ended (`outcomes-head.txt`).
- **Mutants** (`mutants-round2.txt`, on a `git clone --no-local` of
  `45009bc7`): **36 of 36 killed**, none unbuilt (round 2's `mutants.py`
  counts a mutant that does not build as no kill). They are round 1's 22
  and 14 new ones, one for each new line:
  - `load_choices` read by serde_json again;
  - a failed load treated as no choice;
  - fx's checks skipped;
  - `workspaces` or the entry not required to be an object;
  - a list not required to be an array, a name not required to be a
    string, an empty name allowed;
  - the rejections unchecked, the switch unchecked;
  - a profile that does not read treated as none;
  - the runtime loading the workspace anyway, killed by the runtime test;
  - not-UTF-8 labelled unreadable again, or decoded lossily.
- **Gate** (`gate-round2.txt`): the full gate on a clone of `45009bc7`, under the lock, is running as this is committed; the next commit fills this in.

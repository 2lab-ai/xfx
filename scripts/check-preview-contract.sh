#!/usr/bin/env bash
#
# Enforces the preview publication contract.
#
#   scripts/check-preview-contract.sh [path-to-preview-workflow]
#
# A preview build is only worth publishing if a user can install it with the
# exact command the README prints, and if what they then run can prove which
# commit it came from. That promise is spread across a workflow, a release, and
# a Homebrew formula in another repository, and none of those three can be
# checked by running the product: the failure modes are a tag nobody can parse,
# an asset named something the formula does not fetch, a release marked latest,
# a tap that was never pushed, and a formula rendered from a template that has
# since grown a placeholder. Every one of those ships green.
#
# So the contract is checked as *structure*: the workflow is parsed as YAML and
# interrogated -- triggers, the job graph, the exact four native rows, the order
# of the gates against the build, the environment the binary is stamped with,
# the five assets, the release flags, the tap push and its freshness guard, and
# the version of the linter the workflow gates itself with -- rather than
# grepped as text. A `grep` cannot see that `publish` depends on `build`, that
# the smoke test runs after the binary was built and not before, or that a step
# is `continue-on-error`.
#
# The comparator that keeps the tap from being rolled back is not asserted at
# all: it is extracted from the workflow and executed here over real versions,
# because "the function is mentioned" and "the function orders versions
# correctly" are different claims and only the second one protects anyone.
#
# The optional argument exists so a mutant of the workflow can be checked in a
# scratch directory: that is how the assertions below are proved to be load
# bearing rather than decorative.
#
# Requires ruby (Psych is in its standard library, so YAML needs no gem) and,
# when it happens to be installed, actionlint.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

workflow="${1:-.github/workflows/preview.yml}"
readme="README.md"

failures=0

fail() {
	printf 'check-preview-contract: %s\n' "$1" >&2
	failures=$((failures + 1))
}

# --- the contract constants --------------------------------------------------
#
# These are the same values the tap's own contract test holds, written once
# here so a drift between the two repositories is a failure on both sides
# rather than a preview that publishes assets no formula fetches.

# preview-YYYY-MM-DD-HHMMSS-<run_id>-<attempt>-<sha12>
export CONTRACT_TAG_GRAMMAR='^preview-([0-9]{4})-([0-9]{2})-([0-9]{2})-([0-9]{6})-([1-9][0-9]*)-([1-9][0-9]*)-([0-9a-f]{12})$'
# YYYY.MM.DD.HHMMSS.<run_id>.<attempt>
export CONTRACT_VERSION_GRAMMAR='^[0-9]{4}\.[0-9]{2}\.[0-9]{2}\.[0-9]{6}\.[1-9][0-9]*\.[1-9][0-9]*$'

export CONTRACT_ASSETS='xfx-macos-aarch64
xfx-macos-x86_64
xfx-linux-aarch64
xfx-linux-x86_64'

# The release carries exactly these five names and nothing else.
export CONTRACT_MANIFEST='SHA256SUMS
xfx-linux-aarch64
xfx-linux-x86_64
xfx-macos-aarch64
xfx-macos-x86_64'

# runner|rust target|published asset. Four native machines: every published
# binary was built and smoke-tested on the hardware it claims.
export CONTRACT_MATRIX='macos-15|aarch64-apple-darwin|xfx-macos-aarch64
macos-15-intel|x86_64-apple-darwin|xfx-macos-x86_64
ubuntu-24.04-arm|aarch64-unknown-linux-gnu|xfx-linux-aarch64
ubuntu-24.04|x86_64-unknown-linux-gnu|xfx-linux-x86_64'

# The whole gate, on the exact commit, before anything is packaged.
export CONTRACT_GATES='cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
./scripts/check-no-stubs.sh
./scripts/check-no-secrets.sh
./scripts/check-xfx-identity.sh'

# The oldest actionlint this workflow may pin. A linter is only worth running
# if it fails what GitHub would fail; one that predates the runner images the
# matrix above names does the opposite -- it rejects a machine GitHub has
# already built on. v1.7.7 (released 2025-01-19) has no `macos-15-intel` in its
# label table and failed run 32538186453 on all three workflows for it; the
# label first appears in v1.7.8 (2025-10-11). The floor is the current release
# rather than that first one, because a stale label table is exactly the
# failure mode, and pinning the newest known-good is what keeps it fresh.
export CONTRACT_ACTIONLINT_MINIMUM='1.7.12'

export CONTRACT_PLACEHOLDERS='@VERSION@
@TAG@
@SOURCE_SHA@
@SOURCE_SHA12@
@SHA_MACOS_AARCH64@
@SHA_MACOS_X86_64@
@SHA_LINUX_AARCH64@
@SHA_LINUX_X86_64@'

# The `--json` fields each `gh` subcommand accepts, pinned from
# `gh release <verb> --json` on gh 2.96.0. The two lists differ, and the
# difference is load bearing: `isLatest` belongs to `release list` only, and
# asking `release view` for it makes gh exit 1 on an unknown JSON field -- a
# publication failing for a reason that has nothing to do with the release.
# The pin is re-checked against the installed gh below so it cannot rot.
export CONTRACT_GH_RELEASE_VIEW_FIELDS='apiUrl assets author body createdAt databaseId id isDraft isImmutable isPrerelease name publishedAt tagName tarballUrl targetCommitish uploadUrl url zipballUrl'
export CONTRACT_GH_RELEASE_LIST_FIELDS='createdAt isDraft isImmutable isLatest isPrerelease name publishedAt tagName'

export CONTRACT_FORMULA_CLASS='XfxPreview'
export CONTRACT_FORMULA_TEMPLATE='Formula/xfx-preview.rb.tmpl'
export CONTRACT_FORMULA_RENDERED='Formula/xfx-preview.rb'
export CONTRACT_TAP_REMOTE='git@github.com:2lab-ai/homebrew-tap.git'
export CONTRACT_TAP_SECRET='TAP_PUSH_KEY'
export CONTRACT_KEEP='15'
export CONTRACT_QUALIFIED_INSTALL='brew install 2lab-ai/tap/xfx-preview'
export CONTRACT_UNQUALIFIED_INSTALL='brew install xfx-preview'
export CONTRACT_CHANNEL='preview'
export CONTRACT_CHANNEL_VARIABLE='XFX_BUILD_CHANNEL'
export CONTRACT_REVISION_VARIABLE='XFX_BUILD_REVISION'

# --- presence ----------------------------------------------------------------

if [ ! -f "$workflow" ]; then
	fail "there is no preview workflow at $workflow, so nothing publishes the preview channel"
	printf 'check-preview-contract: %d problem(s) found\n' "$failures" >&2
	exit 1
fi

if ! command -v ruby >/dev/null 2>&1; then
	fail 'ruby is required to parse the workflow; a text-only check cannot see the job graph'
	exit 1
fi

# --- the workflow contract ---------------------------------------------------

if ! ruby - "$workflow" <<'RUBY'; then
require "yaml"

workflow_path = ARGV.fetch(0)
document = YAML.safe_load(File.read(workflow_path), aliases: true)
raw = File.read(workflow_path)

problems = []
check = lambda do |condition, message|
  problems << message unless condition
end

def list(name, separator = "\n")
  ENV.fetch(name).split(separator).reject(&:empty?)
end

def steps_of(job)
  ((job || {})["steps"] || [])
end

# A run script with its comments removed.
#
# Every assertion below is about what a step *does*. Read against the raw text,
# "the release says --latest=false" is satisfied by a comment explaining why it
# says it, and "the step never scrapes a host key" is failed by a comment
# explaining why it does not -- both of which were observed while writing this,
# in both directions, on the same file.
def commands(run)
  run.to_s.lines.reject { |line| line.lstrip.start_with?("#") }.join
end

def runs_of(job)
  steps_of(job).map { |step| commands(step["run"]) }
end

def run_text(job)
  runs_of(job).join("\n")
end

# The index of the first step that runs `needle`, or nil. Order is a contract
# here: a gate after the build it was supposed to guard is decoration.
def step_index(job, needle)
  runs_of(job).index { |run| run.include?(needle) }
end

def needs_of(job)
  value = (job || {})["needs"]
  value.nil? ? [] : Array(value)
end

# Dot-separated version fields compared as numbers, never as text. The pair this
# exists for is exactly the pair a lexical compare gets backwards: "1.7.7" sorts
# after "1.7.12" as a string, and answering that question wrongly is what a
# floor on the linter is supposed to prevent.
def version_at_least(actual, minimum)
  left = actual.split(".").map(&:to_i)
  right = minimum.split(".").map(&:to_i)
  [left.length, right.length].max.times do |i|
    a = left[i] || 0
    b = right[i] || 0
    return true if a > b
    return false if a < b
  end
  true
end

def uses_step(job, action)
  steps_of(job).find { |step| step["uses"].to_s.start_with?(action) }
end

# ---------------------------------------------------------------- triggers ---

# GitHub's `on:` is YAML 1.1's boolean `true` unless it was quoted.
triggers = document["on"] || document[true] || {}
check.(triggers.dig("push", "branches") == ["main"],
       "the workflow does not publish on a push to main; it triggers on #{triggers.inspect}")
check.(triggers.keys == ["push"],
       "the workflow triggers on #{triggers.keys.inspect}; a workflow holding a deploy key may only run from a push to main, because every other trigger runs the version of it that lives on the ref being dispatched or proposed")

check.(document.dig("concurrency", "group") == "preview-publish",
       "the concurrency group is not `preview-publish`, so two publications can interleave")
check.(document.dig("concurrency", "cancel-in-progress") == false,
       "in-progress publications are cancellable; a cancelled run can leave a tag, some assets, and no tap")

check.(document.dig("permissions", "contents") == "read",
       "the workflow does not start read-only")

# --------------------------------------------------------------- job graph ---

jobs = document["jobs"] || {}
check.(jobs.keys.sort == %w[build preflight publish],
       "the jobs are #{jobs.keys.sort.inspect}, expected preflight, build and publish")

preflight = jobs["preflight"]
build = jobs["build"]
publish = jobs["publish"]

check.(needs_of(preflight).empty?, "preflight depends on something; it is supposed to be the root")
check.(needs_of(build) == %w[preflight],
       "build does not depend on preflight, so it would build without an identity")
check.(needs_of(publish).sort == %w[build preflight],
       "publish depends on #{needs_of(publish).inspect}, expected both preflight and build")
check.(publish&.dig("permissions", "contents") == "write",
       "publish cannot write contents, or was granted more than that")

# --------------------------------------------------------------- preflight ---

if preflight
  outputs = preflight["outputs"] || {}
  %w[tag version commit sha12].each do |name|
    value = outputs[name].to_s
    check.(value.include?("steps.") && value.include?("outputs.#{name}"),
           "preflight does not output #{name} from a step")
  end

  checkout = uses_step(preflight, "actions/checkout")
  check.(checkout&.dig("with", "ref") == "${{ github.sha }}",
         "preflight does not check out the exact commit the run is for")

  text = run_text(preflight)
  check.(text.include?("git rev-parse HEAD"),
         "preflight never confirms what was actually checked out")
  check.(text.include?(ENV.fetch("CONTRACT_TAG_GRAMMAR")),
         "preflight does not validate the tag against the exact preview grammar the tap parses")
  check.(text.include?(ENV.fetch("CONTRACT_VERSION_GRAMMAR")),
         "preflight does not validate the Homebrew version against its exact grammar")
  check.(text.include?("GITHUB_RUN_ID"),
         "the tag does not carry the numeric run id, so two runs in the same second collide")
  check.(text.include?("GITHUB_RUN_ATTEMPT"),
         "the tag does not carry the run attempt, so a re-run collides with the run it repeats")
  check.(text.include?("date -u +%Y-%m-%d-%H%M%S"),
         "the tag's timestamp is not UTC to the second")
  check.(text.include?("actionlint"),
         "no run lints the workflows; a syntactically valid file can still name a step nothing runs")

  # The linter's *version* is part of the contract, not an implementation
  # detail of the step. Every version it knows about is a table compiled into
  # it, so an old one is not a weaker gate but a wrong one: it fails a runner
  # label GitHub already accepts, which is a publication blocked by the tool
  # that was supposed to protect it. The pin is read out of the install line
  # rather than assumed, and an unpinned install is its own failure -- a linter
  # that can change under the workflow it guards is the reason the step pins at
  # all.
  minimum = ENV.fetch("CONTRACT_ACTIONLINT_MINIMUM")
  pins = text.scan(/actionlint@v?([0-9]+(?:\.[0-9]+)*)/).flatten.uniq
  if pins.empty?
    problems << "the workflow lint does not install actionlint at a pinned version; the gate that guards a publishing workflow must not be able to change under it"
  else
    pins.each do |pin|
      check.(version_at_least(pin, minimum),
             "the workflow lint pins actionlint v#{pin}, older than the required v#{minimum}; " \
             "a linter that predates this matrix's runner images rejects labels GitHub already runs, so the publication fails on a machine that works")
    end
  end

  check.(text.include?("./scripts/check-preview-contract.sh"),
         "the publication does not re-run its own contract check")
end

# ------------------------------------------------------------ native build ---

if build
  expected_matrix = list("CONTRACT_MATRIX").map do |row|
    runner, target, asset = row.split("|")
    { "runner" => runner, "target" => target, "asset" => asset }
  end
  actual_matrix = (build.dig("strategy", "matrix") || {})["include"] || []
  check.(actual_matrix.sort_by { |row| row["asset"].to_s } == expected_matrix.sort_by { |row| row["asset"] },
         "the build matrix is #{actual_matrix.inspect}, expected exactly #{expected_matrix.inspect}")
  check.(build.dig("strategy", "fail-fast") == false,
         "one failing target cancels the others, so a failure hides the state of the rest")

  checkout = uses_step(build, "actions/checkout")
  check.(checkout&.dig("with", "ref") == "${{ needs.preflight.outputs.commit }}",
         "a build job does not check out the exact commit preflight named")

  text = run_text(build)
  check.(text.include?("git rev-parse HEAD"),
         "a build job never confirms it is on the commit being published")
  check.(text.include?("rustc -vV"),
         "a build job never confirms the runner really is the target it claims")
  check.(text.include?("rustup toolchain install stable"),
         "a build job does not install the stable toolchain it builds with")

  build_step = step_index(build, "cargo build --locked --release")
  if build_step.nil?
    problems << "no step builds the release binary"
  else
    list("CONTRACT_GATES").each do |gate|
      index = step_index(build, gate)
      if index.nil?
        problems << "the gate `#{gate}` does not run before a preview binary is built"
      else
        check.(index < build_step,
               "the gate `#{gate}` runs after the binary was already built")
      end
    end

    stamp = steps_of(build).find { |step| commands(step["run"]).include?("cargo build --locked --release") }
    env = stamp["env"] || {}
    check.(env[ENV.fetch("CONTRACT_CHANNEL_VARIABLE")] == ENV.fetch("CONTRACT_CHANNEL"),
           "the release build is not stamped #{ENV.fetch('CONTRACT_CHANNEL_VARIABLE')}=#{ENV.fetch('CONTRACT_CHANNEL')}, so the binary would claim a channel it is not")
    check.(env[ENV.fetch("CONTRACT_REVISION_VARIABLE")] == "${{ needs.preflight.outputs.commit }}",
           "the release build is not stamped with the exact source commit through #{ENV.fetch('CONTRACT_REVISION_VARIABLE')}")

    status_step = step_index(build, "status --json")
    smoke_step = step_index(build, "./scripts/smoke.sh target/release/xfx")
    package_step = step_index(build, "${{ matrix.asset }}")

    if status_step.nil?
      problems << "nothing runs the binary that was just built to see what it says it is"
    else
      status_run = runs_of(build)[status_step]
      check.(status_step > build_step, "the channel is asserted before the binary exists")
      check.(status_run.include?("build_channel") && status_run.include?(ENV.fetch("CONTRACT_CHANNEL")),
             "the built binary is never asserted to report the preview channel")
      check.(status_run.include?("build_revision"),
             "the built binary is never asserted to report a build revision")
      check.(status_run.include?("needs.preflight.outputs.sha12"),
             "the reported revision is not compared against the commit being published")
    end

    check.(!smoke_step.nil? && smoke_step > build_step,
           "the release binary is not smoke-tested after it is built")
    if !package_step.nil? && !smoke_step.nil?
      check.(package_step > smoke_step,
             "the asset is named and checksummed before the binary passed the smoke test")
    end
  end

  package = steps_of(build).find { |step| commands(step["run"]).include?("cp target/release/xfx") }
  if package.nil?
    problems << "no step copies the built binary to its published name"
  else
    run = commands(package["run"])
    check.(run.include?('"${{ matrix.asset }}"'),
           "the published asset is not named from the matrix, so a row could publish another row's name")
    check.(run.include?("chmod"), "the published asset is not made executable")
    check.(run.include?("sha256sum") || run.include?("shasum -a 256"),
           "the published asset is not checksummed on the runner that built it")
  end

  upload = uses_step(build, "actions/upload-artifact")
  check.(upload&.dig("with", "name") == "${{ matrix.asset }}",
         "the artifact is not named after the asset it carries")
  check.(upload&.dig("with", "if-no-files-found") == "error",
         "an empty artifact upload would be tolerated, and publish would then be short an asset")
end

# ------------------------------------------------------------- publication ---

if publish
  assets = list("CONTRACT_ASSETS")
  manifest = list("CONTRACT_MANIFEST")

  checkout = uses_step(publish, "actions/checkout")
  check.(checkout&.dig("with", "ref") == "${{ needs.preflight.outputs.commit }}",
         "publish does not check out the exact commit it is publishing")
  check.(checkout&.dig("with", "persist-credentials") == false,
         "publish leaves the checkout's token in the working copy, next to a step that clones another repository")
  check.(!uses_step(publish, "actions/download-artifact").nil?,
         "publish never downloads the artifacts the build jobs produced")

  assemble = steps_of(publish).find { |step| commands(step["run"]).include?("SHA256SUMS") }
  if assemble.nil?
    problems << "no step assembles the release directory"
  else
    run = commands(assemble["run"])
    assets.each do |asset|
      check.(run.include?(asset), "the release directory is assembled without #{asset}")
    end
    check.(run.include?("sha256sum --check --strict") || run.include?("shasum -a 256 -c"),
           "the checksums written into SHA256SUMS are never verified against the files")
    check.(run.include?(".sha256"),
           "the checksum computed on the native runner is never compared with the file about to be published")
    check.(manifest.all? { |name| run.include?(name) },
           "the release directory is not compared against the exact five-name manifest")
  end

  release = steps_of(publish).find { |step| commands(step["run"]).include?("gh release create") }
  if release.nil?
    problems << "nothing creates the prerelease"
  else
    run = commands(release["run"])
    check.(run.include?("--prerelease"), "the release is not marked as a prerelease")
    check.(run.include?("--latest=false"),
           "the release does not say `--latest=false`, so a preview would become the repository's latest release")
    check.(!run.include?("--draft"), "the release is a draft, so nothing can install it")
    check.(run.include?("--target"), "the release is not tied to a commit")
    check.(run.include?("needs.preflight.outputs.commit") || (release["env"] || {}).values.any? { |value| value.to_s.include?("needs.preflight.outputs.commit") },
           "the release target is not the exact source commit")
    manifest.each do |name|
      check.(run.include?(name), "the release is created without #{name}")
    end
  end

  notes = run_text(publish)
  check.(notes.include?(ENV.fetch("CONTRACT_QUALIFIED_INSTALL")),
         "the release notes do not print the qualified install command a first-time user needs")
  check.(notes.include?(ENV.fetch("CONTRACT_UNQUALIFIED_INSTALL")),
         "the release notes do not print the exact `brew install xfx-preview` command this channel promises")

  confirm = steps_of(publish).find { |step| commands(step["run"]).include?("gh release view") }
  if confirm.nil?
    problems << "nothing reads the release back, so the flags are believed rather than checked"
  else
    run = commands(confirm["run"])
    %w[isPrerelease isDraft isLatest].each do |field|
      check.(run.include?(field), "the published release is never checked for #{field}")
    end
    check.(run.include?("git/ref/tags/"),
           "the published tag is never confirmed to point at the source commit")
    manifest.each do |name|
      check.(run.include?(name), "the published asset set is not compared against #{name}")
    end
  end

  # Every `--json` list is checked against the fields the subcommand it belongs
  # to actually accepts. gh exits 1 on an unknown field, so a wrong list is a
  # step that always fails -- and it fails after the release exists, which is
  # the one place in this workflow where "try again" is not free.
  supported = {
    "view" => list("CONTRACT_GH_RELEASE_VIEW_FIELDS", " "),
    "list" => list("CONTRACT_GH_RELEASE_LIST_FIELDS", " "),
  }
  run_text(publish).scan(/--json\s+([A-Za-z0-9_,]+)/) do |(fields)|
    verb = Regexp.last_match.pre_match.scan(/gh release (view|list)\b/).flatten.last
    next if verb.nil?

    fields.split(",").reject(&:empty?).each do |field|
      check.(supported.fetch(verb).include?(field),
             "`gh release #{verb} --json` is asked for `#{field}`, which it does not support; " \
             "the fields it accepts are #{supported.fetch(verb).join(', ')}")
    end
  end

  # ------------------------------------------------------------------ tap ---

  secret = ENV.fetch("CONTRACT_TAP_SECRET")
  release_at = release.nil? ? nil : steps_of(publish).index(release)

  # A `uses:` step with the key in its environment hands a credential for
  # another repository to code this repository does not review.
  steps_of(publish).each do |step|
    next unless (step["env"] || {}).key?(secret)

    check.(step["uses"].nil?,
           "the step `#{step['name']}` gives #{secret} to the action #{step['uses'].inspect} instead of to a shell in this file")
  end

  tap_steps = steps_of(publish).select { |step| (step["env"] || {}).key?(secret) && step["run"] }
  if tap_steps.empty?
    problems << "no step updates the tap, so `brew install xfx-preview` would not see this build"
  end

  # Whatever a step touching the key does, it does these.
  tap_steps.each do |step|
    name = step["name"].to_s
    check.((step["env"] || {})[secret] == "${{ secrets.#{secret} }}",
           "`#{name}` does not read the repository-scoped deploy key from secrets.#{secret}")
    check.(step["continue-on-error"] != true,
           "`#{name}` is continue-on-error; a preview nobody can install would report success")

    run = commands(step["run"])
    guard = run[/if \[ -z "\$\{#{secret}:-\}" \]; then(.*?)\n\s*fi/m, 1]
    if guard.nil?
      problems << "`#{name}` has no explicit branch for a missing #{secret}"
    else
      check.(guard.include?("exit 1"),
             "in `#{name}` a missing #{secret} does not fail the run; the release would be published with no way to install it")
      check.(!guard.include?("exit 0"),
             "in `#{name}` a missing #{secret} is treated as success")
    end
    check.(run.include?("known_hosts"), "`#{name}` does not pin GitHub's host key")
    check.(!run.include?("ssh-keyscan"),
           "`#{name}` trusts a host key handed to it by the network it is defending against")
    check.(run.include?("unset #{secret}"),
           "`#{name}` leaves the key in its environment after loading it into the agent")
  end

  # The prerequisite: the cheaply checkable reasons the tap step would fail are
  # found before a release exists, because a published release cannot be
  # recalled. Its scope is deliberately bounded -- key present, SSH
  # authentication, the tap cloned at master, the template there -- and it must
  # not claim, by pushing anything, that the later update is already decided.
  prepare = tap_steps.find { |step| commands(step["run"]).include?("git ls-remote") }
  if prepare.nil?
    problems << "nothing checks the tap is reachable before the release is created; a missing key, a key the tap does not know, or a missing template would be discovered after publishing"
  else
    run = commands(prepare["run"])
    prepare_at = steps_of(publish).index(prepare)
    check.(release_at.nil? || prepare_at < release_at,
           "the tap prerequisite runs after `gh release create`, so the release exists before anyone knows it can be installed")
    check.(run.include?(ENV.fetch("CONTRACT_TAP_REMOTE")),
           "the prerequisite does not reach the tap over SSH")
    check.(run.include?("git clone"),
           "the prerequisite does not clone the tap, so the push step has nothing prepared to render in")
    check.(run.include?("origin master") && run.include?("reset --hard origin/master"),
           "the prerequisite does not fetch and reset the exact master branch it will push to")
    check.(run.include?("tap/#{ENV.fetch('CONTRACT_FORMULA_TEMPLATE')}"),
           "the prerequisite does not require the formula template that the render depends on")
    check.(run.match?(/-f tap\/#{Regexp.escape(ENV.fetch('CONTRACT_FORMULA_TEMPLATE'))}/),
           "the prerequisite never tests that the template file exists")

    # It checks; it does not change anything, and it does not push -- not even
    # a dry run. A tap mutated before the release would point at a download
    # that does not exist yet, and a dry run would be a claim about a decision
    # the server has not made: whether the update is accepted is settled when
    # the update is attempted, by the push step, which fails hard.
    pushes = run.scan(/git -C tap push[^\n]*/)
    check.(pushes.empty?,
           "the prerequisite pushes to the tap before the release exists: #{pushes.inspect}; a real push mutates it and a dry run cannot promise the later push is accepted")
    check.(!run.include?("git -C tap commit"),
           "the prerequisite commits to the tap before the release exists")
    check.(!run.include?(">tap/#{ENV.fetch('CONTRACT_FORMULA_RENDERED')}"),
           "the prerequisite renders the formula before the release exists")
  end

  push_step = tap_steps.find { |step| commands(step["run"]).include?("git -C tap push origin") }
  if push_step.nil?
    problems << "nothing pushes the rendered formula to the tap"
  else
    run = commands(push_step["run"])
    push_at = steps_of(publish).index(push_step)
    check.(release_at.nil? || push_at > release_at,
           "the tap is pushed before the release it names exists, so the formula would point at a download that is not there")
    check.(!run.include?("git clone"),
           "the push step clones the tap again instead of reusing the checkout the prerequisite proved")
    check.(run.include?(ENV.fetch("CONTRACT_FORMULA_TEMPLATE")),
           "the formula is not rendered from the tracked template")
    check.(run.include?(ENV.fetch("CONTRACT_FORMULA_RENDERED")),
           "the rendered formula is not written to #{ENV.fetch('CONTRACT_FORMULA_RENDERED')}")
    check.(run.include?("ruby -c"), "the rendered formula is never syntax-checked")
    check.(run.include?("class #{ENV.fetch('CONTRACT_FORMULA_CLASS')}"),
           "the rendered formula is never confirmed to be the #{ENV.fetch('CONTRACT_FORMULA_CLASS')} formula")
    check.(run.match?(/grep -q '@\[A-Z0-9_\]\*@'/),
           "an unrendered placeholder would be pushed to the tap")

    expected_placeholders = list("CONTRACT_PLACEHOLDERS").sort
    rendered_placeholders = run.scan(/@[A-Z0-9_]+@/).uniq.sort
    check.(rendered_placeholders == expected_placeholders,
           "the render substitutes #{rendered_placeholders.inspect}, but the formula template needs #{expected_placeholders.inspect}")

    check.(run.include?("preview_version_gt"),
           "the push step has no numeric freshness comparator")
    check.(run.scan(/refuse_downgrade/).length >= 3,
           "the freshness guard is not re-asked after every fetch and before the commit")
    check.(run.include?("for attempt in 1 2 3"),
           "the tap push does not retry a lost race")
    fetch_at = run.index("git -C tap fetch")
    commit_at = run.index("git -C tap commit")
    check.(!fetch_at.nil? && !commit_at.nil? && fetch_at < commit_at,
           "the tap is committed without being refetched first")
    check.(run.include?("exit 1"),
           "the push step cannot fail, so an unusable tap would be reported as a publication")
  end

  # ---------------------------------------------------------------- prune ---

  prune = steps_of(publish).find { |step| commands(step["run"]).include?("gh release delete") }
  if prune.nil?
    problems << "old previews are never pruned"
  else
    run = commands(prune["run"])
    check.(run.include?('startswith("preview-")') && run.include?("isPrerelease"),
           "the prune does not restrict itself to preview prereleases, so it could delete a stable release")
    check.(run.include?(".[#{ENV.fetch('CONTRACT_KEEP')}:]"),
           "the prune does not keep exactly #{ENV.fetch('CONTRACT_KEEP')} previews")
    check.(run.include?("sort_by(.createdAt)"),
           "the prune does not order previews by creation, so which fifteen survive is arbitrary")
    check.(run.include?("--cleanup-tag"),
           "a pruned preview leaves its tag behind")
    check.(run.include?("preview-*"),
           "the delete loop does not re-check that what it is about to delete is a preview")

    prune_index = steps_of(publish).index(prune)
    check.(push_step.nil? || steps_of(publish).index(push_step) < prune_index,
           "previews are pruned before the tap was updated")
  end
end

# ------------------------------------------------------- what must not be ---

# The lookbehind keeps `scripts/check-no-secrets.sh` from reading as a secret.
secret_refs = raw.scan(/(?<![-\w])secrets\.[A-Za-z_][A-Za-z0-9_]*/).uniq
check.(secret_refs == ["secrets.#{ENV.fetch('CONTRACT_TAP_SECRET')}"],
       "the workflow reads #{secret_refs.inspect}; the only credential it may use is the repository-scoped tap deploy key")

["cargo publish", "crates.io", "refs/tags/v"].each do |forbidden|
  check.(!raw.include?(forbidden),
         "the preview workflow mentions `#{forbidden}`; a preview is not a release and must not make one")
end

problems.each { |problem| warn "check-preview-contract: #{problem}" }
exit(problems.empty? ? 0 : 1)
RUBY
	fail 'the workflow does not satisfy the preview publication contract'
fi

# --- the gh field pin, against the gh that is installed ----------------------
#
# The workflow check above rejects a `--json` field its subcommand does not
# accept, using the lists pinned at the top of this file. A pin is worth only
# as much as its last comparison with reality, so when gh is here it is asked
# directly rather than trusted: its field list is what it prints when `--json`
# arrives with no value, which is why the invocation below looks like a
# mistake. When gh is absent, unauthenticated, or outside a GitHub checkout it
# says so and the pin stands unverified -- this never passes quietly for having
# asked nothing.

if command -v gh >/dev/null 2>&1; then
	for verb in view list; do
		case "$verb" in
		view) pinned="$CONTRACT_GH_RELEASE_VIEW_FIELDS" ;;
		list) pinned="$CONTRACT_GH_RELEASE_LIST_FIELDS" ;;
		*) pinned='' ;;
		esac

		usage="$(gh release "$verb" --json 2>&1 || true)"
		if ! printf '%s\n' "$usage" | grep -q 'comma-separated fields'; then
			printf 'check-preview-contract: gh did not report its --json fields for `release %s`; the pinned list is unverified here\n' "$verb"
			continue
		fi

		live="$(printf '%s\n' "$usage" | sed -n 's/^[[:space:]][[:space:]]*\([a-zA-Z][a-zA-Z0-9_]*\)$/\1/p')"
		for field in $pinned; do
			if ! printf '%s\n' "$live" | grep -qx "$field"; then
				fail "the pinned field list for \`gh release $verb --json\` includes '$field', which the installed gh does not accept"
			fi
		done
	done
fi

# --- the freshness comparator, executed rather than asserted -----------------

work="$(mktemp -d "${TMPDIR:-/tmp}/xfx-preview-contract.XXXXXX")"
trap 'rm -rf "$work"' EXIT HUP INT TERM

ruby -ryaml - "$workflow" >"$work/publish.sh" <<'RUBY'
document = YAML.safe_load(File.read(ARGV.fetch(0)), aliases: true)
job = (document["jobs"] || {})["publish"] || {}
print (job["steps"] || []).map { |step| step["run"].to_s }.join("\n")
RUBY

# --- the exact-tag latest query, executed rather than asserted ---------------
#
# jq's `//` is "alternative", not "default": it fires on `false` exactly as it
# fires on `null`. A query that reads `isLatest` and then defaults it that way
# answers "missing" for the one case this readback exists to see -- a preview
# that is correctly not the latest release -- and fails a correct publication
# after the release has been created. So the shipped filter is run here over
# literal releases rather than being read for plausibility.

awk "/^latest_of_tag='\$/, /^'\$/" "$work/publish.sh" >"$work/latest-query.sh"
if [ ! -s "$work/latest-query.sh" ]; then
	fail 'the exact-tag latest query could not be extracted, so the readback is unproven'
elif ! command -v jq >/dev/null 2>&1; then
	printf 'check-preview-contract: jq is not installed; the latest query is unverified here\n'
else
	# shellcheck source=/dev/null
	. "$work/latest-query.sh"

	answers() {
		local want="$1" tag="$2" releases="$3" got
		got="$(printf '%s' "$releases" | jq -r --arg tag "$tag" "$latest_of_tag")"
		if [ "$got" != "$want" ]; then
			fail "the latest query answers '$got' for $releases, expected '$want'"
		fi
	}

	# The case the whole readback is about, and the case `//` gets wrong.
	answers false T '[{"tagName":"T","isLatest":false}]'
	answers true T '[{"tagName":"T","isLatest":true}]'
	# Absent is not the same answer as false, and must not be reported as one.
	answers missing T '[{"tagName":"OTHER","isLatest":true}]'
	answers missing T '[]'
	# The exact tag is selected, not the first release that happens to be there.
	answers false T '[{"tagName":"OTHER","isLatest":true},{"tagName":"T","isLatest":false}]'
fi

awk '/^preview_version_gt\(\) \{$/, /^\}$/' "$work/publish.sh" >"$work/version-gt.sh"
if [ ! -s "$work/version-gt.sh" ]; then
	fail 'the shipped preview_version_gt() could not be extracted, so the no-downgrade guard is unproven'
else
	# shellcheck source=/dev/null
	. "$work/version-gt.sh"

	newer() {
		if ! preview_version_gt "$1" "$2"; then
			fail "the freshness guard refuses a strictly newer version: $1 is newer than $2"
		fi
	}
	not_newer() {
		if preview_version_gt "$1" "$2"; then
			fail "the freshness guard would downgrade the tap: it accepts $1 over $2"
		fi
	}

	version='2026.08.22.054213.32601234567.1'
	# A later second, a later day, a later run, a later attempt.
	newer "$version" '2026.08.22.054212.32601234567.1'
	newer "$version" '2026.08.21.235959.99999999999.9'
	newer '2026.08.22.054213.32601234568.1' "$version"
	newer '2026.08.22.054213.32601234567.2' "$version"
	# The same build is not newer than itself: a re-render must not churn the tap.
	not_newer "$version" "$version"
	not_newer '2026.08.22.054212.32601234567.1' "$version"
	not_newer '2026.08.21.235959.99999999999.9' "$version"
	not_newer '2026.08.22.054213.32601234567.1' '2026.08.22.054213.32601234567.2'
	# `08` and `09` are not octal here, whatever the shell would like to think.
	newer '2026.08.22.094213.32601234567.1' '2026.08.22.084213.32601234567.1'
	newer '2026.09.22.054213.32601234567.1' '2026.08.22.054213.32601234567.1'
fi

# --- the documentation the contract promises ---------------------------------

if [ -f "$readme" ]; then
	if ! grep -Fq "$CONTRACT_QUALIFIED_INSTALL" "$readme"; then
		fail "$readme does not document the qualified install command ($CONTRACT_QUALIFIED_INSTALL)"
	fi
	if ! grep -Fq "$CONTRACT_UNQUALIFIED_INSTALL" "$readme"; then
		fail "$readme does not document the exact command this channel promises ($CONTRACT_UNQUALIFIED_INSTALL)"
	fi
fi

# --- actionlint, when it is here ---------------------------------------------
#
# The workflow lints itself in CI on a machine where actionlint is installed on
# purpose. Locally it is a bonus, not a requirement: a developer without it
# still gets every assertion above.

if command -v actionlint >/dev/null 2>&1; then
	if ! actionlint "$workflow" >"$work/actionlint.txt" 2>&1; then
		# Which actionlint said so, named in the failure rather than left to be
		# guessed: a rejection of a runner label this repository already builds
		# on is far more often a stale local binary than a bad workflow, and
		# that is the same mistake the pinned floor above exists for.
		fail "actionlint $(actionlint --version | head -1) rejects the preview workflow (the workflow pins v$CONTRACT_ACTIONLINT_MINIMUM or newer):"
		cat "$work/actionlint.txt" >&2
	fi
fi

if [ "$failures" -ne 0 ]; then
	printf 'check-preview-contract: %d problem(s) found\n' "$failures" >&2
	exit 1
fi

printf 'check-preview-contract: ok (%s)\n' "$workflow"

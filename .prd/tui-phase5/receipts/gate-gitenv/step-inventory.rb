# GATE-GITENV-F: ci.yml's gate job, step by step, for each matrix row, at a
# commit: which steps run on that row. A step's `if:` is read when it is
# `matrix.<key>`; any other condition is printed as it is.
#
#   git show <rev>:.github/workflows/ci.yml | ruby step-inventory.rb <label>
require "yaml"
doc = YAML.safe_load($stdin.read, aliases: true)
job = doc.fetch("jobs").fetch("gate")
rows = job.dig("strategy", "matrix", "include")
puts "== #{ARGV[0]}"
rows.each do |row|
  steps = job["steps"].map do |step|
    cond = step["if"].to_s.strip.sub(/\A\$\{\{\s*/, "").sub(/\s*\}\}\z/, "")
    runs = if cond.empty? then true
           elsif (key = cond[/\Amatrix\.([A-Za-z0-9_]+)\z/, 1]) then !!row[key]
           else "if #{step["if"]}"
           end
    [step["name"] || step["uses"], runs]
  end
  ran = steps.select { |_, r| r == true }.map(&:first)
  skipped = steps.reject { |_, r| r == true }
  puts "#{row["target"]} (#{row["runner"]}): #{ran.length} of #{steps.length} steps"
  skipped.each { |name, r| puts "  not run: #{name}#{r.is_a?(String) ? " (#{r})" : ""}" }
end
harness = job["steps"].select { |s| s["run"].to_s =~ %r{\./scripts/test-[a-z-]+\.sh} }
puts "script-test steps: #{harness.map { |s| s["run"].to_s[%r{\./scripts/test-[a-z-]+\.sh}] }.join(", ")}"

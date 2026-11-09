# Train 12, join check 3: on which of ci.yml's gate rows each step that runs
# LOCAL-PATHS's two scripts runs, at the checkout given. Each step's `if` is
# read as the two forms ci.yml's gate job uses -- `matrix.part != 'release'`
# and `matrix.script_tests` -- and any other refuses (exit 2), so a changed
# condition is not read as true.
#
#   ruby -ryaml ci_rows.rb <checkout>
ci = YAML.load_file(File.join(ARGV[0], ".github/workflows/ci.yml"))
job = ci["jobs"]["gate"]
rows = job["strategy"]["matrix"]["include"]
holds = lambda do |cond, row|
  case cond.to_s.strip
  when "" then true
  when "matrix.part != 'release'" then row["part"] != "release"
  when "matrix.script_tests" then row["script_tests"] == true
  else
    warn "an `if` this script does not read: #{cond}"
    exit 2
  end
end
found = 0
job["steps"].each do |step|
  run = step["run"].to_s
  next unless run.include?("check-no-local-paths")
  found += 1
  on = rows.select { |row| holds.(step["if"], row) }
  puts "#{step["name"]}: `#{run.strip}` if #{step["if"].inspect}"
  rows.each do |row|
    label = [row["runner"], row["target"], row["part"]].compact.join(" ")
    puts "    #{on.include?(row) ? "runs" : "skips"}  #{label}"
  end
end
puts "# #{found} steps run LOCAL-PATHS's scripts, of #{job["steps"].size} in the gate job; #{rows.size} rows"
exit(found == 2 ? 0 : 1)

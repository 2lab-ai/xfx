# usage: ruby partition.rb <ci.yml>
# For each matrix row of jobs.gate, which steps run on success (if: evaluated
# for the four forms ci.yml uses), and whether the split x86 parts cover the
# unsplit step list exactly once.
require "yaml"
doc = YAML.safe_load(File.read(ARGV[0]), aliases: true)
job = doc["jobs"]["gate"]
rows = job["strategy"]["matrix"]["include"]
def runs?(cond, row)
  return true if cond.nil?
  c = cond.strip
  return false if c.start_with?("failure()")
  return !!row["script_tests"] if c == "matrix.script_tests"
  if (m = c.match(/\Amatrix\.part != '(\w+)'\z/))
    return row["part"].to_s != m[1]
  end
  abort "unknown condition: #{c}"
end
setup = 4
lists = rows.map { |r| [r, job["steps"].each_with_index.select { |s, i| runs?(s["if"], r) }.map { |s, i| i }] }
lists.each do |r, idx|
  puts "#{r["target"]}#{r["part"] ? " (#{r["part"]})" : ""}: #{idx.size} steps"
end
x = lists.select { |r, _| r["target"] == "x86_64-apple-darwin" }
whole = job["steps"].each_with_index.select { |s, i| runs?(s["if"], {"target" => "x86_64-apple-darwin"}) }.map { |_, i| i }
union = x.flat_map { |_, idx| idx.select { |i| i >= setup } }
setup_both = x.all? { |_, idx| (0...setup).all? { |i| idx.include?(i) } }
dups = union.group_by { |i| i }.select { |_, v| v.size > 1 }.keys
puts "x86 setup steps in both parts: #{setup_both}"
puts "x86 parts' non-setup steps, sorted, == the unsplit row's: #{union.sort == whole.select { |i| i >= setup }}"
puts "steps in both parts beyond setup: #{dups.map { |i| job["steps"][i]["name"] }.inspect}"
x.each { |r, idx| puts "  #{r["part"]}: " + idx.select { |i| i >= setup }.map { |i| job["steps"][i]["name"] }.join(" | ") }

# usage: ruby extract-steps.rb <ci.yml> <job> <outdir> <target>
# Writes every `run:` step of <job> to <outdir>/NN.sh, in order, with
# ${{ matrix.target }} substituted; skips `uses:` steps and the toolchain
# install (it would change the local default toolchain). Prints an index.
require "yaml"
path, job, out, target = ARGV
doc = YAML.safe_load(File.read(path), aliases: true)
steps = doc["jobs"].fetch(job)["steps"]
Dir.mkdir(out) unless Dir.exist?(out)
n = 0
steps.each do |s|
  name = s["name"]
  if s["uses"]
    puts "skip\t(uses #{s["uses"]})\t#{name}"
    next
  end
  if name == "Install the pinned toolchain"
    puts "skip\t(local toolchain kept)\t#{name}"
    next
  end
  run = s["run"].gsub("${{ matrix.target }}", target)
  abort "unsubstituted expression in #{name}" if run.include?("${{")
  n += 1
  file = format("%s/%02d.sh", out, n)
  File.write(file, "# #{name}\n#{run}")
  puts "run\t#{format("%02d", n)}\t#{name}"
end

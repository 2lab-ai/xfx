#!/usr/bin/perl
# Lists negated `.contains(<short literal>)` occurrences (also `!...any(|x| x.contains(..))`),
# multi-line aware. Usage: negscan.pl FILE...
use strict; use warnings;
for my $f (@ARGV) {
  open my $h, '<', $f or next; local $/; my $s = <$h>; close $h;
  while ($s =~ /(?<![A-Za-z0-9_!=])!(?!=)\s*([^;!{}]*?)\.contains\(\s*(?:"((?:[^"\\]|\\.){1,4})"|'((?:[^'\\]|\\.){1,2})')\s*\)/gs) {
    my $lit = defined $2 ? "\"$2\"" : "'$3'";
    my $pos = $-[0];
    my $line = (substr($s, 0, $pos) =~ tr/\n//) + 1;
    my $expr = $1; $expr =~ s/\s+/ /g;
    print "$f:$line\t$lit\t!$expr\n";
  }
}

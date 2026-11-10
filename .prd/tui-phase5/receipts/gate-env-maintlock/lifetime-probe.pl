#!/usr/bin/perl
# lifetime-probe.pl <runs>: new_decoy as on 32afd307 (no forcing), then
# how long objects/maintenance.lock stays after `git commit` returns,
# polled every 0.1 ms for at most 2 s. Prints the distribution in ms.
use strict;
use warnings;
use Time::HiRes qw(time usleep);
use File::Temp qw(tempdir);
my $runs = shift // 50;
my $root = tempdir("lifetime-probe.XXXXXX", DIR => $ENV{TMPDIR} // "/tmp", CLEANUP => 1);
$ENV{GIT_CONFIG_GLOBAL} = "/dev/null";
$ENV{GIT_CONFIG_NOSYSTEM} = "1";
my @sg = ("git", "-c", "core.fsmonitor=false");
my (@life, $absent);
$absent = 0;
for my $i (1 .. $runs) {
	my $d = "$root/d$i";
	mkdir $d or die;
	system(@sg, "-C", $d, "init", "-q") == 0 or die;
	open(my $fh, ">", "$d/d.txt") or die; print $fh "decoy\n"; close $fh;
	system(@sg, "-C", $d, "add", "--", "d.txt") == 0 or die;
	system(@sg, "-C", $d, "-c", "user.name=decoy", "-c", "user.email=decoy\@example.invalid",
		"-c", "commit.gpgsign=false", "commit", "-q", "-m", "decoy") == 0 or die;
	my $t0 = time;
	my $lock = "$d/.git/objects/maintenance.lock";
	if (!-e $lock) { $absent++; next; }
	while (-e $lock && time - $t0 < 2) { usleep(100); }
	push @life, (time - $t0) * 1000;
}
@life = sort { $a <=> $b } @life;
my $n = @life;
printf "lifetime-probe runs=%d lock-absent-at-return=%d lock-present=%d\n", $runs, $absent, $n;
if ($n) {
	printf "lock lifetime after the commit returned, ms: min %.2f median %.2f p90 %.2f max %.2f\n",
		$life[0], $life[int($n / 2)], $life[int($n * 0.9)], $life[-1];
}

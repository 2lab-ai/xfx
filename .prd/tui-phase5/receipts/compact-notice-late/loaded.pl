#!/usr/bin/perl
# usage: loaded.pl <spinners> <n> <test-binary> <test-name>
# Forks <spinners> busy loops (each ends itself after 1800 s), runs the one
# test <n> times one after another (--exact --test-threads 1), prints each
# run's result with the 1-minute load average when it began, then kills the
# busy loops it forked (by the pids it holds) and reaps them.
use strict; use warnings;
my ($spin, $n, $bin, $name) = @ARGV;
my @kids;
sub stop { kill 'TERM', @kids if @kids; waitpid($_, 0) for @kids; @kids = (); }
$SIG{INT} = $SIG{TERM} = sub { stop(); exit 130 };
END { stop() }
for (1 .. $spin) {
    my $pid = fork(); die "fork: $!" unless defined $pid;
    if ($pid == 0) { @kids = (); alarm 1800; 1 while 1; exit 0 }
    push @kids, $pid;
}
sleep 5;
my ($pass, $fail) = (0, 0);
for my $i (1 .. $n) {
    my $load = (split ' ', `sysctl -n vm.loadavg`)[1];
    my $out = `'$bin' '$name' --exact --test-threads 1 2>&1`;
    my $code = $? >> 8;
    if ($code == 0) { $pass++; print "run $i pass load=$load\n" }
    else {
        $fail++;
        my @why = grep { /panicked at|waiting for|left:|right:/ } split /\n/, $out;
        print "run $i FAIL load=$load: ", join(' | ', map { substr($_, 0, 200) } @why[0 .. ($#why < 2 ? $#why : 2)]), "\n";
    }
}
print "total: $pass passed, $fail failed of $n (spinners=$spin)\n";

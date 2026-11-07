# N busy loops for the RED/GREEN runs, each ending itself after SECS; writes
# their pids to a file and waits for them.
my ($n, $secs, $pids) = @ARGV;
my @kids;
for (1 .. $n) {
    my $pid = fork();
    die "fork: $!" unless defined $pid;
    if ($pid == 0) { alarm $secs; 1 while 1; exit 0 }
    push @kids, $pid;
}
open my $f, '>', $pids; print $f "@kids\n"; close $f;
waitpid($_, 0) for @kids;

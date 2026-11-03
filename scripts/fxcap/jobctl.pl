#!/usr/bin/perl
# Runs a program as a job-control shell runs a foreground job, for capture.sh's
# `jobctl`: in a process group of its own, which owns the terminal, with this
# process -- in the same session, in another group -- as its parent.
#
# usage: jobctl.pl <pidfile> <program> [<args>...]
#
# Why: capture.sh `exec`s the binary as its pane's session leader, so the
# binary's process group has no parent inside its session and is orphaned, and
# the kernel discards a stop signal sent to an orphaned group: fx's
# `raise(SIGTSTP)` after its restore (vercel-labs/fx@481104fd
# src/core/app/app_lifecycle.zig:1167-1180) does not stop it, and neither does
# an outside SIGTSTP. Under this wrapper the group is not orphaned, so SIGTSTP
# really stops the binary and SIGCONT really continues it.
#
# Unlike a shell, this never takes the terminal back while the job is stopped,
# and writes nothing: the screen a stop leaves is the binary's own restore, and
# a SIGCONT sent to the binary's pid (capture.sh's `signal SIGCONT`) finds it
# still the terminal's foreground, as `fg` would have made it.
#
# The child writes its own pid to <pidfile> before it execs, so the pid there
# is the program's, exactly. When the program ends, this exits as it did: with
# its status, or by the same signal, so the pane's dead status is the program's.
use strict;
use warnings;
use POSIX qw(setpgid tcsetpgrp waitpid WUNTRACED WIFEXITED WEXITSTATUS WIFSIGNALED WTERMSIG WIFSTOPPED _exit);

my $pidfile = shift @ARGV;
die "usage: jobctl.pl <pidfile> <program> [<args>...]\n" unless defined $pidfile && @ARGV;

my $pid = fork();
die "jobctl: fork: $!\n" unless defined $pid;
if ($pid == 0) {
    setpgid(0, 0);
    # A process outside the foreground group that sets the foreground group
    # is sent SIGTTOU; ignored for that one call, then back to the default, so
    # the program does not inherit an ignored SIGTTOU across exec.
    $SIG{TTOU} = 'IGNORE';
    tcsetpgrp(0, $$) or do { print STDERR "jobctl: tcsetpgrp: $!\n"; _exit(126) };
    $SIG{TTOU} = 'DEFAULT';
    open(my $fh, '>', $pidfile) or do { print STDERR "jobctl: $pidfile: $!\n"; _exit(126) };
    print $fh "$$\n";
    close($fh);
    exec { $ARGV[0] } @ARGV or print STDERR "jobctl: exec $ARGV[0]: $!\n";
    _exit(127);
}
# Both sides set the group, so neither the exec nor a signal can come first.
setpgid($pid, $pid);

while (1) {
    my $got = waitpid($pid, WUNTRACED);
    next if $got == -1 && $!{EINTR};
    die "jobctl: waitpid: $!\n" if $got == -1;
    # The W* macros read the native status, not perl's $? (perlvar).
    my $status = ${^CHILD_ERROR_NATIVE};
    next if WIFSTOPPED($status);
    exit WEXITSTATUS($status) if WIFEXITED($status);
    if (WIFSIGNALED($status)) {
        my $signal = WTERMSIG($status);
        $SIG{$_} = 'DEFAULT' for keys %SIG;
        kill $signal, $$;
        exit 128 + $signal;
    }
}

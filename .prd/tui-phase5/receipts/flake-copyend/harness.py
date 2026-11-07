"""FLAKE-COPYEND's slow-start harness: adds one case to src/tui/copy.rs's
tests in a throwaway clone. Another thread takes the process's start lock
(`crate::process::hold_every_start`, the lock every start takes one at a
time, process.rs SPAWNING) and holds it for 1.5 s while the case runs -- as
another test's start does in the full suite -- so the copy's tool cannot
start for 1.5 s, past the 1 s grace.

usage: python3 -I harness.py <clone> base|head

base: the case is the old an_ending_session_lets_a_copy_that_answers_land
      body (a 0.2 s tool, the session's Drop) with the hold before the copy.
head: the case is the new body (the gated tool, `Errands::end`) with the
      same hold.
Then run: cargo test --locked --lib tui::copy::tests::slow_start_lands
"""

import sys

HOLD = '''
    /// Another start holds the process's start lock for 1.5 s from now.
    fn hold_the_start_lock() -> std::thread::JoinHandle<()> {
        let (taken, waiting) = mpsc::channel();
        let holder = std::thread::spawn(move || {
            let lock = crate::process::hold_every_start();
            taken.send(()).expect("the case is waiting");
            std::thread::sleep(Duration::from_millis(1500));
            drop(lock);
        });
        waiting.recv().expect("the lock was taken");
        holder
    }
'''

BASE = HOLD + '''
    #[test]
    fn slow_start_lands() {
        let tool = crate::clipboard::fake::FakeTool::running("sleep 0.2\\ncat > \\"${0%/*}/copied\\"");
        let copies = Copies::new(tool.clipboard());
        assert!(copies.start(Arc::from("a first reply")));
        assert!(outcome(&copies, Duration::from_secs(30)));
        let holder = hold_the_start_lock();
        assert!(copies.start(Arc::from("the reply")));
        drop(copies);
        holder.join().expect("the holder");
        assert_eq!(tool.copied().as_deref(), Some("the reply".as_bytes()));
    }
}
'''

HEAD = HOLD + '''
    #[test]
    fn slow_start_lands() {
        let tool = gated();
        let copies = Copies::new(tool.clipboard());
        let holder = hold_the_start_lock();
        assert!(copies.start(Arc::from("the reply")));
        let mut looked = false;
        copies.errands.end(|began| {
            if !looked {
                tool.open();
                looked = true;
            }
            began.elapsed() >= LOADED
        });
        holder.join().expect("the holder");
        assert!(looked, "the end gave a copy still running no grace");
        assert_eq!(tool.copied().as_deref(), Some("the reply".as_bytes()));
    }
}
'''

clone, side = sys.argv[1], sys.argv[2]
path = clone + "/src/tui/copy.rs"
text = open(path).read()
assert text.rstrip().endswith("}"), "copy.rs ends with its tests module"
text = text.rstrip()[:-1] + (BASE if side == "base" else HEAD)
open(path, "w").write(text)
print("harness added (%s)" % side)

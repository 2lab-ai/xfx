"""Applies one named mutant to a throwaway clone's sources. Run with the clone as cwd."""
import sys
def rep(p, a, b):
    s = open(p).read()
    assert s.count(a) == 1, (p, a[:80], s.count(a))
    open(p, 'w').write(s.replace(a, b))

name = sys.argv[1]
OSA_NEW_OLD = '''        let mut command = self.command("osascript");
        command.arg("-e").arg(clipboard_script(&path));
        let outcome = crate::process::spawn(&mut command).and_then(|mut child| child.wait());'''
OSA_NEW_HEAD = '''        let mut command = self.command("osascript");
        command.arg("-e").arg(clipboard_script(&path));
        let outcome = crate::process::status(&mut command);'''
def osa_current():
    s = open('src/clipboard.rs').read()
    return OSA_NEW_HEAD if OSA_NEW_HEAD in s else OSA_NEW_OLD
if name == 'osascript-chain-status':
    # The pre-fix form (main a592c3a2): a chain ending in `.status();`.
    rep('src/clipboard.rs', osa_current(), '''        let outcome = self
            .command("osascript")
            .arg("-e")
            .arg(clipboard_script(&path))
            .status();''')
elif name == 'osascript-split-status':
    rep('src/clipboard.rs', osa_current(), '''        let mut command = self.command("osascript");
        command.arg("-e").arg(clipboard_script(&path));
        let outcome = command.status();''')
elif name == 'mcp-ufcs-spawn':
    s = open('src/mcp/transport.rs').read()
    a = 'crate::process::spawn(&mut builder)'
    assert s.count(a) == 1
    open('src/mcp/transport.rs', 'w').write(s.replace(a, 'std::process::Command::spawn(&mut builder)'))
elif name == 'mcp-bare-spawn':
    s = open('src/mcp/transport.rs').read()
    a = 'crate::process::spawn(&mut builder)'
    assert s.count(a) == 1
    open('src/mcp/transport.rs', 'w').write(s.replace(a, 'builder.spawn()'))
elif name == 'wakeup-bare-pipe':
    rep('src/tui/signals.rs', 'let (read, write) = crate::process::pipe()?;', 'let (read, write) = rustix::pipe::pipe()?;')
elif name == 'allow-in-transport':
    s = open('src/mcp/transport.rs').read()
    a = '        let mut child =\n            crate::process::spawn(&mut builder)'
    assert s.count(a) == 1, s.count(a)
    open('src/mcp/transport.rs', 'w').write(s.replace(a, '        #[allow(clippy::disallowed_methods)]\n        let mut child =\n            builder.spawn()'))
elif name == 'clippy-toml-drops-status':
    lines = [l for l in open('clippy.toml').read().split('\n') if '"std::process::Command::status"' not in l]
    open('clippy.toml', 'w').write('\n'.join(lines))
elif name == 'no-lock':
    rep('src/process.rs', '''pub fn spawn(command: &mut Command) -> io::Result<Child> {
    let _one_at_a_time = SPAWNING.lock().unwrap_or_else(PoisonError::into_inner);
    command.spawn()''', '''pub fn spawn(command: &mut Command) -> io::Result<Child> {
    command.spawn()''')
elif name == 'spawn-fails':
    rep('src/process.rs', '''pub fn spawn(command: &mut Command) -> io::Result<Child> {
    let _one_at_a_time = SPAWNING.lock().unwrap_or_else(PoisonError::into_inner);
    command.spawn()''', '''pub fn spawn(command: &mut Command) -> io::Result<Child> {
    let _one_at_a_time = SPAWNING.lock().unwrap_or_else(PoisonError::into_inner);
    let _ = command;
    Err(io::Error::other("mutant: no start"))''')
elif name == 'given-up-before-lock':
    rep('src/process.rs', '''    let _one_at_a_time = SPAWNING.lock().unwrap_or_else(PoisonError::into_inner);
    if given_up() {
        return Ok(None);
    }''', '''    let given_up = given_up();
    let _one_at_a_time = SPAWNING.lock().unwrap_or_else(PoisonError::into_inner);
    if given_up {
        return Ok(None);
    }''')
elif name == 'pipe-no-lock':
    rep('src/process.rs', '    let _no_start = SPAWNING.lock().unwrap_or_else(PoisonError::into_inner);\n', '')
elif name == 'pipe-no-cloexec':
    rep('src/process.rs', '''    for end in [&read, &write] {
        rustix::io::fcntl_setfd(end, rustix::io::FdFlags::CLOEXEC)?;
    }
''', '')
elif name == 'clipboard-look-before-lock':
    rep('src/clipboard.rs', 'crate::process::spawn_unless(&mut command, || stop.stopped())', 'crate::process::spawn_unless(&mut command, || false)')
else:
    raise SystemExit('no such mutant: ' + name)
print('applied', name)

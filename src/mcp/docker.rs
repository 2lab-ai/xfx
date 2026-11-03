//! A `docker run …` server's container, removed with the server.
//!
//! Stopping a `docker run -i` client does not always stop its container: the
//! daemon owns it, not the process xfx started. So, as upstream does
//! (`vercel-labs/fx@c1db9191 src/core/mcp/docker_run.zig:94-133`), a direct
//! `docker run` gets `--cidfile <TMPDIR>/xfx-mcp-<hex>.cid` inserted after
//! `run`, and when the server stops, the id the daemon wrote there is removed
//! with `docker rm -f <id>` -- the same `docker` the server was started with,
//! never one found by name. Nothing else is rewritten: a command that already
//! names a `--cidfile`, or that runs docker through anything else, is started
//! exactly as written.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The longest a container id may be (`max_container_id_bytes`).
const MAX_CONTAINER_ID_BYTES: u64 = 128;

/// How long the removal may take (`cleanup_timeout`, fifteen seconds).
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(15);

/// What to start, and what to remove afterwards when there is anything.
pub struct Prepared {
    pub argv: Vec<String>,
    pub cleanup: Option<Cleanup>,
}

/// The container a server's `--cidfile` will name.
pub struct Cleanup {
    docker: String,
    cidfile: PathBuf,
}

/// `prepare` (`docker_run.zig:94-133`).
pub fn prepare(argv: &[String]) -> Prepared {
    let unchanged = || Prepared {
        argv: argv.to_vec(),
        cleanup: None,
    };
    if !is_direct_docker_run(argv) || has_cidfile(argv) {
        return unchanged();
    }
    let Some(root) = temporary_root().filter(|root| root.is_absolute()) else {
        return unchanged();
    };
    let cidfile = root.join(format!("xfx-mcp-{}.cid", nonce_hex()));
    let mut rewritten = Vec::with_capacity(argv.len() + 2);
    rewritten.push(argv[0].clone());
    rewritten.push(argv[1].clone());
    rewritten.push("--cidfile".to_string());
    rewritten.push(cidfile.to_string_lossy().into_owned());
    rewritten.extend(argv[2..].iter().cloned());
    Prepared {
        argv: rewritten,
        cleanup: Some(Cleanup {
            docker: argv[0].clone(),
            cidfile,
        }),
    }
}

fn is_direct_docker_run(argv: &[String]) -> bool {
    if argv.len() < 2 || argv[1] != "run" {
        return false;
    }
    let command = Path::new(&argv[0])
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    command.eq_ignore_ascii_case("docker") || command.eq_ignore_ascii_case("docker.exe")
}

fn has_cidfile(argv: &[String]) -> bool {
    argv[2..]
        .iter()
        .any(|arg| arg == "--cidfile" || arg.starts_with("--cidfile="))
}

/// `TMPDIR`, `TEMP` or `TMP`, else `/tmp` (`temporaryRoot`).
fn temporary_root() -> Option<PathBuf> {
    ["TMPDIR", "TEMP", "TMP"]
        .iter()
        .find_map(|name| std::env::var_os(name).filter(|value| !value.is_empty()))
        .map(PathBuf::from)
        .or_else(|| cfg!(unix).then(|| PathBuf::from("/tmp")))
}

/// Sixteen random bytes as hex, from the kernel.
fn nonce_hex() -> String {
    let mut bytes = [0u8; 16];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut file| {
        use std::io::Read;
        file.read_exact(&mut bytes)
    });
    if read.is_err() {
        // A unique-enough fallback: the cidfile is created by the daemon, so a
        // collision makes `docker run` fail rather than share a container.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        bytes[..16].copy_from_slice(&now.to_le_bytes());
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

impl Cleanup {
    /// Removes the container the cidfile names, then the cidfile.
    pub fn run(self) {
        let id = read_container_id(&self.cidfile);
        let _ = std::fs::remove_file(&self.cidfile);
        let Some(id) = id else {
            return;
        };
        let Ok(mut child) = crate::process::command(&self.docker)
            .args(["rm", "-f", &id])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            return;
        };
        let started = Instant::now();
        while started.elapsed() < CLEANUP_TIMEOUT {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// `readContainerId`: 12 to 64 hex digits, from a regular file of at most
/// 128 bytes.
fn read_container_id(path: &Path) -> Option<String> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_CONTAINER_ID_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let id = text.trim();
    ((12..=64).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(argv: &[&str]) -> Vec<String> {
        argv.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn a_direct_docker_run_gets_a_cidfile_after_run_and_nothing_else_is_rewritten() {
        let prepared = prepare(&words(&[
            "/usr/local/bin/docker",
            "run",
            "--rm",
            "-i",
            "fixture",
        ]));
        assert_eq!(prepared.argv.len(), 7);
        assert_eq!(prepared.argv[2], "--cidfile");
        assert!(prepared.argv[3].ends_with(".cid"), "{:?}", prepared.argv);
        assert!(prepared.argv[3].contains("xfx-mcp-"));
        assert_eq!(&prepared.argv[4..], ["--rm", "-i", "fixture"]);
        assert!(prepared.cleanup.is_some());
        for argv in [
            words(&["docker", "run", "--cidfile=/x", "i"]),
            words(&["docker", "ps"]),
            words(&["podman", "run", "i"]),
            words(&["/bin/sh", "-c", "docker run i"]),
        ] {
            let prepared = prepare(&argv);
            assert_eq!(prepared.argv, argv);
            assert!(prepared.cleanup.is_none());
        }
    }

    #[test]
    fn only_a_hex_container_id_is_read_back() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("x.cid");
        std::fs::write(&path, "0123456789abcdef\n").unwrap();
        assert_eq!(
            read_container_id(&path).as_deref(),
            Some("0123456789abcdef")
        );
        std::fs::write(&path, "short").unwrap();
        assert_eq!(read_container_id(&path), None);
        std::fs::write(&path, "0123456789abcdef; rm -rf /").unwrap();
        assert_eq!(read_container_id(&path), None);
    }
}

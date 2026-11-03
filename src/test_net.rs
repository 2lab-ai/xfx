//! For tests that prove a client sends a credential to no proxy.
//!
//! reqwest reads `HTTP_PROXY` and `ALL_PROXY` when a client is built, and the
//! environment is the whole process's: setting one here would reach every test
//! running beside this one. So the case runs again in a copy of this test
//! binary, alone, with the proxy set in the copy's environment only, against a
//! loopback server and a loopback proxy that both record what reaches them.
//!
//! A case is one `#[test]` that is both halves: [`server_in_this_copy`] says
//! which one it is.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// Where the copy finds the server its parent started.
const SERVER: &str = "XFX_TEST_PROXY_CASE_SERVER";

/// Every variable that can route a request through a proxy or exempt one.
const PROXY_VARIABLES: [&str; 8] = [
    "HTTP_PROXY",
    "http_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "ALL_PROXY",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];

/// What the proxy answers whatever reaches it: nothing a client can use.
const BAD_GATEWAY: &str =
    "HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";

/// The server's origin when this process is the copy, `None` in the parent.
pub(crate) fn server_in_this_copy() -> Option<String> {
    std::env::var(SERVER).ok()
}

/// A `200` with a JSON body.
pub(crate) fn json_answer(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
         connection: close\r\n\r\n{body}",
        body.len()
    )
}

/// A redirect to `location`.
pub(crate) fn redirect(status: u16, location: &str) -> String {
    format!(
        "HTTP/1.1 {status} Redirect\r\nlocation: {location}\r\ncontent-length: 0\r\n\
         connection: close\r\n\r\n"
    )
}

/// Runs the case `test` (its full name, `module::tests::name`) again in a copy
/// of this test binary with `HTTP_PROXY` and `ALL_PROXY` naming a loopback
/// proxy, against a server that answers `answer`, and asserts that the proxy
/// received nothing, the server one request, and the copy passed.
pub(crate) fn assert_no_proxy_is_used(test: &str, answer: String) {
    let server = Listener::answering(answer);
    let proxy = Listener::answering(BAD_GATEWAY.to_string());
    let proxy_url = proxy.origin();
    let mut copy = crate::process::command(std::env::current_exe().expect("this test binary"));
    for variable in PROXY_VARIABLES {
        copy.env_remove(variable);
    }
    for variable in ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] {
        copy.env(variable, &proxy_url);
    }
    copy.env(SERVER, server.origin());
    copy.args(["--exact", "--test-threads=1", test]);
    let output = copy.output().expect("a copy of this test binary");
    let reached = proxy.finish();
    let served = server.finish();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        reached.is_empty(),
        "the proxy received {} request(s), bearer attached: {:?}",
        reached.len(),
        reached
            .iter()
            .map(|request| request
                .to_ascii_lowercase()
                .contains("\r\nauthorization: bearer "))
            .collect::<Vec<_>>()
    );
    assert_eq!(served.len(), 1, "the server was not asked once: {report}");
    // Counted, not only exited-zero: a filter that matched nothing passes.
    assert!(
        output.status.success() && report.contains("test result: ok. 1 passed"),
        "{report}"
    );
}

/// A loopback server that answers every connection with the same bytes and
/// keeps what it read, until [`Self::finish`].
pub(crate) struct Listener {
    origin: String,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Vec<String>>,
}

impl Listener {
    pub(crate) fn answering(answer: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
        listener.set_nonblocking(true).expect("a polled listener");
        let origin = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("an address").port()
        );
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let mut requests = Vec::new();
            while !stopped.load(Ordering::SeqCst) {
                let Ok((mut connection, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                let _ = connection.set_nonblocking(false);
                let _ = connection.set_read_timeout(Some(Duration::from_secs(5)));
                requests.push(read_request(&mut connection));
                let _ = connection.write_all(answer.as_bytes());
                let _ = connection.flush();
                let _ = connection.shutdown(std::net::Shutdown::Write);
            }
            requests
        });
        Self {
            origin,
            stop,
            thread,
        }
    }

    /// `http://127.0.0.1:<port>`.
    pub(crate) fn origin(&self) -> String {
        self.origin.clone()
    }

    /// Every request that reached it, head and body.
    pub(crate) fn finish(self) -> Vec<String> {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.join().expect("the listener")
    }
}

/// One request's head and as much body as its `Content-Length` says.
fn read_request(connection: &mut std::net::TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if request.len() >= end + 4 + length {
                break;
            }
        }
        match connection.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => request.extend_from_slice(&buffer[..read]),
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}

"""MCP-E round 2: each mutant of the unit on a --no-local clone, one at a
time -- apply it, run the tests that should catch it, revert it. Usage:
python3 -I mutants2.py <clone> <out> [--dry]   (--dry: count matches only)"""
import subprocess, sys, os

clone, out = sys.argv[1], sys.argv[2]
dry = "--dry" in sys.argv
env = dict(os.environ, TMPDIR="<tmp>")
os.makedirs(env["TMPDIR"], exist_ok=True)
D = "mcp::tests::discover::"
T = "mcp::transport::tests::"
E = "src/mcp/era.rs"
R = "src/mcp/runtime.rs"
X = "src/mcp/transport.rs"
W = "src/mcp/wire.rs"
MUTANTS = [
    # Round 1's, on the code as it is now.
    ("M1 initialize accepts a version not its own (2024-11-05)", E,
     "            Event::InitializeResult(Version::Other)\n        }",
     "            Event::InitializeResult(Version::Legacy(0))\n        }",
     D + "a_stdio_server_that_speaks_only_2024_11_05"),
    ("M2 the probe is not cut to what is left of the start", R,
     "probe_due = (probe, Instant::now() + probe_wait(after, left()));",
     "probe_due = (probe, Instant::now() + after);",
     D + "the_probe_and_initialize_share_the_start"),
    ("M3 an HTTP request outlives its caller's wait", X,
     "                result = tokio::time::timeout(left, &mut work) => {\n"
     "                    let _ = reply.send(result.unwrap_or(Err(McpError::Named(\"McpRequestTimedOut\"))));\n"
     "                }\n"
     "                // The caller stopped waiting: dropping the exchange drops its\n"
     "                // connection.\n"
     "                _ = &mut stop => {}",
     "                result = &mut work => {\n"
     "                    let _ = (left, &mut stop);\n"
     "                    let _ = reply.send(result);\n"
     "                }",
     D + "a_request_whose_caller_stops_waiting"),
    ("M3b the probe's wait is not its caller's", X,
     ".and_then(|()| wait_until(&receiver, Instant::now() + timeout, cancel))",
     ".and_then(|()| wait_until(&receiver, Instant::now() + timeout * 1000, cancel))",
     D + "an_http_probe_the_server_holds_open"),
    ("M4 an error without an id does not answer the probe", X,
     "                    None if matches!(response.body, wire::Body::Failure(_)) => {",
     "                    None if false => {",
     D + "a_hostile_discover_answer"),
    ("M5 a 2026 stdio server's request is answered", X,
     "        matches!(self, Self::Initializing | Self::Legacy(_))",
     "        matches!(self, Self::Initializing | Self::Legacy(_) | Self::Modern(_))",
     D + "a_2026_stdio_server_is_served"),
    ("M6 a request with no params carries no _meta", X,
     "    let mut ordered = Map::new();\n    ordered.insert(\"_meta\".into(), era::meta(meta, id));",
     "    let mut ordered = Map::new();\n    if params.as_object().is_some_and(|p| !p.is_empty()) { ordered.insert(\"_meta\".into(), era::meta(meta, id)); }",
     D + "a_stdio_server_is_asked_server_discover_first"),
    ("M7 each try's ids start again at 1", R,
     "                    .map_err(started)?\n                    .with_ids(Arc::clone(&server.ids))\n                    .with_idle(config.operation_timeout()),",
     "                    .map_err(started)?\n                    .with_idle(config.operation_timeout()),",
     D + "a_stdio_server_that_speaks_only_2024_11_05"),
    ("M8 a 2026 HTTP POST carries no Mcp-Method", X,
     "            .header(\"Mcp-Method\", method);",
     ";",
     D + "a_2026_http_server_is_probed"),
    ("M9 a stdio line refused for the probe is read as a version list", R,
     "        return era::classify_discover(config, refused_body(&message));",
     "        let _ = message;\n        return era::Event::DiscoverResult(era::Listed::default());",
     D + "a_hostile_discover_answer"),
    ("M10 a modern server that falls quiet is initialized anyway", E,
     "        if self.modern_seen && !self.advertised.legacy {",
     "        if false && self.modern_seen && !self.advertised.legacy {",
     D + "a_hostile_discover_answer"),
    ("M11 a 404 with -32601 is a legacy answer", E,
     "    if status == 404\n",
     "    if false && status == 404\n",
     D + "a_404_with_method_not_found"),
    ("M12 the unsupported-version words lose fx's parenthesis", E,
     "\"it answered with an MCP version xfx doesn't support (xfx speaks 2025-03-26 and newer)\"\n",
     "\"it answered with an MCP version xfx doesn't support\"\n",
     D + "a_stdio_server_that_speaks_only_2024_11_05"),
    ("M13 a version list matches a string that only contains a version", W,
     "        if element.len() == needle.len() + 2\n            && element.starts_with('\"')\n            && &element[1..element.len() - 1] == needle\n        {",
     "        if element.starts_with('\"') && element.contains(needle) {",
     D + "a_hostile_discover_answer"),
    ("M14 an HTTP body that is no JSON-RPC is a broken stream", X,
     "            match wire::decode(&text) {\n                Err(_) => Ok(none),",
     "            match wire::decode(&text) {\n                Err(_) => Err(McpError::Named(\"McpNoResult\")),",
     D + "an_http_body_that_is_no_json_rpc"),
    # Round 2's: each puts back what round 1 did, or breaks what fx does.
    ("M15 an event stream is read only once it ends (round 1)", X,
     "    while let Some(chunk) = stream.next().await {\n"
     "        let chunk = chunk.map_err(|_| McpError::Named(\"McpConnectionClosed\"))?;\n"
     "        if let Some(answer) = events.feed(&chunk)? {\n"
     "            return Ok(answer);\n"
     "        }\n"
     "    }",
     "    let mut held = Vec::new();\n"
     "    while let Some(chunk) = stream.next().await {\n"
     "        let chunk = chunk.map_err(|_| McpError::Named(\"McpConnectionClosed\"))?;\n"
     "        held.extend_from_slice(&chunk);\n"
     "    }\n"
     "    if let Some(answer) = events.feed(&held)? {\n"
     "        return Ok(answer);\n"
     "    }",
     D + "a_probe_answered_on_an_event_stream"),
    ("M16 an answer without an id does not answer its POST", X,
     "    named.is_none_or(|named| named == id)",
     "    named == Some(id)",
     D + "a_probe_answered_on_an_event_stream"),
    ("M17 a string id is never the client's own", W,
     "        b'\"' => Ok(decimal(&value[1..value.len() - 1])),",
     "        b'\"' => Ok(None),",
     T + "a_stream_event_answers_its_post"),
    ("M18 a stream's answer under an error status is refused (as JSON's)", X,
     "    if stream {\n        let frame = read_events(response, id).await?;",
     "    if stream && status.is_success() {\n        let frame = read_events(response, id).await?;",
     D + "a_probe_answered_on_an_event_stream"),
    ("M19 a stdio probe given up on is never cancelled (round 1)", X,
     "                    self.cancel_later(id, sent + self.idle);",
     "                    let _ = sent;\n                    self.pending.lock().unwrap_or_else(PoisonError::into_inner).remove(&id);",
     D + "a_stdio_probe_given_up_on_is_cancelled"),
    ("M20 an HTTP probe given up on is closed at once (round 1)", X,
     "        let result = if method == \"server/discover\" {",
     "        let result = if false {",
     D + "an_http_probe_the_server_holds_open"),
    ("M21 an HTTP probe given up on outlives its timeout outside a session", X,
     "            if !target.era().in_session() {\n                return;\n            }",
     "            let _ = target.era();",
     D + "an_abandoned_http_probe_ends_at_its_request_timeout"),
    ("M22 a version written with an escape matches", W,
     "        if element.len() == needle.len() + 2\n            && element.starts_with('\"')\n            && &element[1..element.len() - 1] == needle\n        {",
     "        if decode_string(element).as_deref() == Some(needle) {",
     D + "a_hostile_discover_answer"),
    ("M23 a field named twice is taken", W,
     "                        if slot.is_some() {\n                            return Err(DecodeError::DuplicateField);\n                        }",
     "",
     D + "a_hostile_discover_answer"),
    ("M24 the decoder reads deeper than fx's", W,
     "pub const MAX_DEPTH: usize = 2584;",
     "pub const MAX_DEPTH: usize = 2600;",
     D + "a_hostile_discover_answer"),
    ("M25 the decoder reads less deep than fx's", W,
     "pub const MAX_DEPTH: usize = 2584;",
     "pub const MAX_DEPTH: usize = 2583;",
     D + "a_hostile_discover_answer"),
    ("M26 a line that names no request fails the probe (banner)", X,
     "                if let Some(id) = wire::response_id(text) {",
     "                if let Some(id) = wire::response_id(text)\n                    .or(*self.probe.lock().unwrap_or_else(PoisonError::into_inner))\n                {",
     D + "a_2026_server_that_writes_a_banner"),
    ("M27 a server's request is answered while detection runs", X,
     "        matches!(self, Self::Initializing | Self::Legacy(_))",
     "        matches!(self, Self::Detecting | Self::Initializing | Self::Legacy(_))",
     D + "a_servers_request_is_answered_only_once_initialize"),
    ("M28 a request is answered before initialize begins a session", R,
     "                    transport.set_era(WireEra::Initializing);\n",
     "",
     D + "a_servers_request_is_answered_only_once_initialize"),
    ("M29 no waiting room past 32 in flight", X,
     "                        } else if waiting.len() < WAITING_ROOM {",
     "                        } else if false {",
     D + "past_32_exchanges_in_flight"),
    ("M30 a refused line's excerpt is cut at 40 bytes", W,
     "pub const EXCERPT_MAX: usize = 80;",
     "pub const EXCERPT_MAX: usize = 40;",
     "mcp::wire::tests::a_refused_reply_fails_with_why"),
]

with open(out, "w") as log:
    for name, path, old, new, test in MUTANTS:
        full = os.path.join(clone, path)
        text = open(full).read()
        n = text.count(old)
        if n != 1:
            log.write(f"{name}: NOT APPLIED ({n} matches)\n"); log.flush(); continue
        if dry:
            log.write(f"{name}: applies\n"); continue
        open(full, "w").write(text.replace(old, new))
        try:
            p = subprocess.run(["cargo", "test", "--locked", "--lib", test],
                               cwd=clone, env=env, capture_output=True, text=True, timeout=900)
            res = [l for l in p.stdout.splitlines() if l.startswith("test result") or " FAILED" in l]
            err = [l for l in p.stderr.splitlines() if l.startswith("error")]
            ran = any(l.startswith("test result") for l in res)
            verdict = "COMPILE-ERROR" if err and not ran else ("KILLED" if p.returncode != 0 else "SURVIVED")
            if verdict == "KILLED" and not any(" FAILED" in l for l in res):
                verdict = "SURVIVED (no test failed)"
            log.write(f"{name} [{path}] -> {test}: {verdict}\n")
            for l in res + err[:3]:
                log.write(f"    {l}\n")
        except subprocess.TimeoutExpired:
            log.write(f"{name}: TIMEOUT (killed at 900 s)\n")
        finally:
            subprocess.run(["git", "-C", clone, "checkout", "--", path], check=True)
        log.flush()

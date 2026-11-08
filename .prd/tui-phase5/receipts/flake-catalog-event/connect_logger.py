#!/usr/bin/env python3
"""A loopback stand-in for an HTTPS proxy: logs the first request line of
each connection (a CONNECT names the host a client was going to), answers
403 and closes. Nothing is forwarded, so no request leaves the machine.

usage: connect_logger.py <port|0> <log> <seconds-to-live> [delay-ms]; the port bound is written to <log>.port
"""
import socket
import sys
import threading
import time

port, log_path, ttl = int(sys.argv[1]), sys.argv[2], float(sys.argv[3])
# Optional: how long to hold each answer, in ms -- a stand-in for a remote round trip.
delay = float(sys.argv[4]) / 1000 if len(sys.argv) > 4 else 0.0
deadline = time.monotonic() + ttl
lock = threading.Lock()


def serve(conn):
    try:
        conn.settimeout(5)
        data = b""
        while b"\r\n" not in data and len(data) < 4096:
            chunk = conn.recv(1024)
            if not chunk:
                break
            data += chunk
        line = data.split(b"\r\n", 1)[0].decode("latin-1")
        with lock, open(log_path, "a") as log:
            log.write(line + "\n")
        time.sleep(delay)
        conn.sendall(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
    except OSError:
        pass
    finally:
        conn.close()


server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(("127.0.0.1", port))
server.listen(64)
port = server.getsockname()[1]
with open(log_path + ".port", "w") as f:
    f.write(str(port))
server.settimeout(1)
print(f"listening 127.0.0.1:{port}", flush=True)
while time.monotonic() < deadline:
    try:
        conn, _ = server.accept()
    except socket.timeout:
        continue
    threading.Thread(target=serve, args=(conn,), daemon=True).start()
server.close()

#!/usr/bin/env python3
"""Fast workspace check via rust-analyzer through ra-multiplex.

Uses textDocument/diagnostic (pull model) to check all .rs source files
against the already-running rust-analyzer instance. No cargo rebuild needed.
"""

import fcntl
import glob
import json
import os
import selectors
import subprocess
import sys
import time

WORKSPACE = "/home/tavis/src/iidy"

def encode_msg(obj):
    body = json.dumps(obj)
    return f"Content-Length: {len(body)}\r\n\r\n{body}".encode()

def send(proc, obj):
    proc.stdin.write(encode_msg(obj))
    proc.stdin.flush()

def read_all_msgs(buf):
    msgs = []
    while True:
        he = buf.find(b"\r\n\r\n")
        if he == -1:
            break
        length = None
        for line in buf[:he].decode().split("\r\n"):
            if line.lower().startswith("content-length:"):
                length = int(line.split(":")[1].strip())
        if length is None:
            break
        bs = he + 4
        if len(buf) < bs + length:
            break
        msgs.append(json.loads(buf[bs:bs + length]))
        buf = buf[bs + length:]
    return msgs, buf

def collect_source_files():
    files = sorted(glob.glob(os.path.join(WORKSPACE, "src", "**", "*.rs"), recursive=True))
    return files

def main():
    proc = subprocess.Popen(
        ["ra-multiplex", "client"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
    )
    flags = fcntl.fcntl(proc.stdout.fileno(), fcntl.F_GETFL)
    fcntl.fcntl(proc.stdout.fileno(), fcntl.F_SETFL, flags | os.O_NONBLOCK)
    sel = selectors.DefaultSelector()
    sel.register(proc.stdout, selectors.EVENT_READ)

    send(proc, {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "processId": os.getpid(),
        "rootUri": f"file://{WORKSPACE}",
        "workspaceFolders": [{"uri": f"file://{WORKSPACE}", "name": "iidy"}],
        "capabilities": {
            "textDocument": {
                "diagnostic": {"dynamicRegistration": True},
                "publishDiagnostics": {"relatedInformation": True},
            },
            "workspace": {"configuration": True},
        },
    }})

    buf = b""
    phase = "init"
    deadline = time.time() + 30
    source_files = collect_source_files()
    # Map request ID -> file path for diagnostic responses
    pending = {}
    next_id = 100
    diagnostics = {}  # uri -> [diag, ...]

    while time.time() < deadline and proc.poll() is None:
        events = sel.select(timeout=0.5)
        if events:
            try:
                data = os.read(proc.stdout.fileno(), 65536)
                if data:
                    buf += data
            except BlockingIOError:
                pass

        msgs, buf = read_all_msgs(buf)
        for msg in msgs:
            mid = msg.get("id")
            method = msg.get("method", "")

            # Initialize response
            if mid == 1 and "result" in msg and phase == "init":
                send(proc, {"jsonrpc": "2.0", "method": "initialized", "params": {}})
                phase = "opening"
                continue

            # Handle server requests
            if mid is not None and method:
                if method == "workspace/configuration":
                    items = msg.get("params", {}).get("items", [])
                    send(proc, {"jsonrpc": "2.0", "id": mid, "result": [{}] * len(items)})
                elif method in ("client/registerCapability", "window/workDoneProgress/create"):
                    send(proc, {"jsonrpc": "2.0", "id": mid, "result": None})
                continue

            # Diagnostic responses
            if mid in pending:
                filepath = pending.pop(mid)
                if "result" in msg:
                    items = msg["result"].get("items", [])
                    if items:
                        diagnostics[filepath] = items
                if not pending:
                    deadline = time.time() + 1  # all done, brief wait for stragglers
                continue

            # Push diagnostics (bonus)
            if method == "textDocument/publishDiagnostics":
                uri = msg["params"]["uri"]
                diags = msg["params"]["diagnostics"]
                path = uri.replace(f"file://{WORKSPACE}/", "")
                if diags:
                    diagnostics[path] = diags
                continue

        # After init handshake is done, open files and request diagnostics
        if phase == "opening":
            for filepath in source_files:
                uri = f"file://{filepath}"
                try:
                    text = open(filepath).read()
                except (IOError, UnicodeDecodeError):
                    continue
                send(proc, {"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
                    "textDocument": {"uri": uri, "languageId": "rust",
                                     "version": 1, "text": text}}})
                req_id = next_id
                next_id += 1
                relpath = os.path.relpath(filepath, WORKSPACE)
                pending[req_id] = relpath
                send(proc, {"jsonrpc": "2.0", "id": req_id,
                             "method": "textDocument/diagnostic",
                             "params": {"textDocument": {"uri": uri}}})
            phase = "waiting"
            deadline = time.time() + 15

    sel.close()
    try:
        send(proc, {"jsonrpc": "2.0", "id": 99, "method": "shutdown", "params": None})
        send(proc, {"jsonrpc": "2.0", "method": "exit", "params": None})
    except (BrokenPipeError, OSError):
        pass
    try:
        proc.kill()
    except OSError:
        pass

    # Report
    errors = 0
    warnings = 0
    for path, diags in sorted(diagnostics.items()):
        for d in diags:
            severity = d.get("severity", 1)
            line = d["range"]["start"]["line"] + 1
            col = d["range"]["start"]["character"] + 1
            msg_text = d["message"]
            source = d.get("source", "")
            if severity == 1:
                errors += 1
                print(f"{path}:{line}:{col}: error: {msg_text} [{source}]")
            elif severity == 2:
                warnings += 1
                print(f"{path}:{line}:{col}: warning: {msg_text} [{source}]")

    if errors:
        print(f"\n{errors} error(s), {warnings} warning(s)")
        return 1
    elif warnings:
        print(f"\n{warnings} warning(s)")
        return 0
    else:
        print("No errors or warnings.")
        return 0

if __name__ == "__main__":
    sys.exit(main())

"""Raw client for the PCSX2-MCP DebugServer (newline-delimited JSON over TCP 127.0.0.1:21512).

Usage: python dbg.py '{"cmd":"status"}'   (one request per argument; prints each response)
Commands (source: source/DebugServer.cpp next to the patched emulator, machine path KH2_PCSX2_MCP): status, read_registers,
write_register, set_pc, read_memory, write_memory, disassemble, evaluate, set_breakpoint, remove_breakpoint, set_memcheck,
remove_memcheck, list_breakpoints, list_memchecks, pause, resume, step, step_over, get_threads, get_modules, get_backtrace,
is_valid_address, read_string, clear_breakpoints. Read the matching `cmd ==` block in DebugServer.cpp for each command's field names.
Needs the patched emulator ($KH2_PCSX2_MCP, not the installed PCSX2 $KH2_PCSX2). Only one PCSX2 at a time: create
local/pcsx2.lock in the main checkout (your id inside) before starting it, delete it when you close it.
"""
import json, socket, sys

def call(req, port=21512, timeout=10):
    with socket.create_connection(("127.0.0.1", port), timeout=timeout) as s:
        s.sendall((json.dumps(req) + "\n").encode())
        buf = b""
        while not buf.endswith(b"\n"):
            chunk = s.recv(65536)
            if not chunk:
                break
            buf += chunk
    return json.loads(buf)

if __name__ == "__main__":
    for a in sys.argv[1:]:
        print(json.dumps(call(json.loads(a))))

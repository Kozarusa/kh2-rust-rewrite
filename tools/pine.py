"""Talk to a running PCSX2 through PINE (enabled in PCSX2.ini: EnablePINE = true, slot 28011 on Windows = TCP localhost).
Reads and writes the PS2's main RAM (EE addresses, e.g. 0x340ce8 for KH2's inventory block) while the real game runs.

usage:
  python tools/pine.py status                  emulator status, title, game id
  python tools/pine.py r32 0x00340e48 [n]      read n 32-bit words
  python tools/pine.py r8 0x... [n]            read n bytes (hex dump)
  python tools/pine.py rf 0x... [n]            read n floats
  python tools/pine.py watch 0x... [seconds] [32|8|16|f]   print every change of one value (about 60 Hz poll)
  python tools/pine.py save <slot>             save state to a slot (the game must be running)
  python tools/pine.py load <slot>             load state from a slot
as a module: from pine import Pine; p = Pine(); p.r32(addr)

Protocol (PCSX2 pine.h): request = u32 total_len, u8 opcode, args; reply = u32 total_len, u8 result (0 ok, 0xFF fail), data.
Opcodes: 0 read8, 1 read16, 2 read32, 3 read64, 4 write8, 5 write16, 6 write32, 7 write64, 8 version, 9 save state,
10 load state, 11 title, 12 id, 13 uuid, 14 game version, 15 status (0 running, 1 paused, 2 shutdown).
"""
import socket, struct, sys, time

PORT = 28011


class Pine:
    def __init__(self, host='127.0.0.1', port=PORT, timeout=3.0):
        self.s = socket.create_connection((host, port), timeout=timeout)

    def _call(self, opcode, args=b''):
        body = bytes([opcode]) + args
        self.s.sendall(struct.pack('<I', len(body) + 4) + body)
        hdr = self._recv(4)
        n = struct.unpack('<I', hdr)[0]
        rest = self._recv(n - 4)
        if rest[0] != 0:
            raise RuntimeError('PINE call failed, opcode %d' % opcode)
        return rest[1:]

    def _recv(self, n):
        out = b''
        while len(out) < n:
            c = self.s.recv(n - len(out))
            if not c:
                raise ConnectionError('PINE closed')
            out += c
        return out

    def r8(self, a): return self._call(0, struct.pack('<I', a))[0]
    def r16(self, a): return struct.unpack('<H', self._call(1, struct.pack('<I', a)))[0]
    def r32(self, a): return struct.unpack('<I', self._call(2, struct.pack('<I', a)))[0]
    def r64(self, a): return struct.unpack('<Q', self._call(3, struct.pack('<I', a)))[0]
    def rf(self, a): return struct.unpack('<f', struct.pack('<I', self.r32(a)))[0]
    def w8(self, a, v): self._call(4, struct.pack('<IB', a, v))
    def w16(self, a, v): self._call(5, struct.pack('<IH', a, v))
    def w32(self, a, v): self._call(6, struct.pack('<II', a, v))
    def save(self, slot): self._call(9, bytes([slot]))
    def load(self, slot): self._call(10, bytes([slot]))

    def _str(self, op):
        d = self._call(op)
        n = struct.unpack('<I', d[:4])[0]
        return d[4:4 + n].rstrip(b'\0').decode('utf-8', 'replace')

    def title(self): return self._str(11)
    def game_id(self): return self._str(12)
    def game_version(self): return self._str(14)
    def status(self): return struct.unpack('<I', self._call(15))[0]


def main(argv):
    p = Pine()
    c = argv[1]
    if c == 'status':
        print('status', p.status(), '| title', p.title(), '| id', p.game_id(), '| version', p.game_version())
    elif c == 'r32':
        a = int(argv[2], 0); n = int(argv[3]) if len(argv) > 3 else 1
        for i in range(n): print('%08x: %08x' % (a + 4 * i, p.r32(a + 4 * i)))
    elif c == 'r8':
        a = int(argv[2], 0); n = int(argv[3]) if len(argv) > 3 else 16
        data = bytes(p.r8(a + i) for i in range(n))
        for i in range(0, n, 16): print('%08x: %s' % (a + i, data[i:i + 16].hex(' ')))
    elif c == 'rf':
        a = int(argv[2], 0); n = int(argv[3]) if len(argv) > 3 else 1
        for i in range(n): print('%08x: %g' % (a + 4 * i, p.rf(a + 4 * i)))
    elif c == 'watch':
        a = int(argv[2], 0); secs = float(argv[3]) if len(argv) > 3 else 10; kind = argv[4] if len(argv) > 4 else '32'
        rd = {'32': p.r32, '16': p.r16, '8': p.r8, 'f': p.rf}[kind]
        t0 = time.time(); last = None
        while time.time() - t0 < secs:
            v = rd(a)
            if v != last:
                print('%6.2fs %08x = %s' % (time.time() - t0, a, v if kind == 'f' else hex(v))); last = v
            time.sleep(1 / 60)
    elif c == 'save':
        p.save(int(argv[2])); print('saved slot', argv[2])
    elif c == 'load':
        p.load(int(argv[2])); print('loaded slot', argv[2])
    else:
        print(__doc__)


if __name__ == '__main__':
    main(sys.argv)

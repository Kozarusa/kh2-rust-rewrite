"""mips_scan.py -- search the game's executable instructions directly (also covers functions the decompilation lacks).

Usage (from any checkout):
  python tools/mips_scan.py --jal 0x1a0180            # every call site of FUN_001a0180
  python tools/mips_scan.py --store-off 0x180          # every sb/sh/sw/sd/sq/swc1 with immediate offset 0x180
  python tools/mips_scan.py --load-off 0x238           # every lb/lbu/lh/lhu/lw/ld/lq/lwc1 with offset 0x238
  python tools/mips_scan.py --li 0x1c4                 # every `addiu/ori rX, zero, 0x1c4` (a constant load)
  python tools/mips_scan.py --addr 0x37b540            # every lui+addiu/lw pair that builds this address
  python tools/mips_scan.py --jal 0x1a0180 --jal 0x1a0210   # several at once

Each hit prints the instruction address and the function that contains it: the nearest decompiled function start at
or below it (file names in $KH2_DECOMP/decomp_typed/main/<address>.c, or decomp/main when the typed folder is absent),
marked '?' when the nearest start is far away (> 0x2000 bytes: probably a function the decompilation lacks). The
executable is $KH2_DECOMP/input/SLUS_210.05 (KH2_DECOMP comes from local/machine.ps1, see tools/machine.example.ps1).
Nothing is written anywhere; the output is study material (never commit it).
"""
import argparse, os, struct, bisect, sys

import machine

EXE = machine.path('KH2_DECOMP', 'input', 'SLUS_210.05')
DECOMP = os.path.join(machine.get('KH2_DECOMP'), 'decomp_typed', 'main')
if not os.path.isdir(DECOMP):
    DECOMP = os.path.join(machine.get('KH2_DECOMP'), 'decomp', 'main')

STORES = {0x28: 'sb', 0x29: 'sh', 0x2b: 'sw', 0x3f: 'sd', 0x1f: 'sq', 0x39: 'swc1'}
LOADS = {0x20: 'lb', 0x24: 'lbu', 0x21: 'lh', 0x25: 'lhu', 0x23: 'lw', 0x37: 'ld', 0x1e: 'lq', 0x31: 'lwc1'}


def segments(data):
    if data[:4] != b'\x7fELF':
        sys.exit('not an ELF: ' + EXE)
    phoff, = struct.unpack_from('<I', data, 0x1c)
    phentsize, phnum = struct.unpack_from('<HH', data, 0x2a)
    for i in range(phnum):
        p_type, p_offset, p_vaddr, _, p_filesz, _, p_flags, _ = struct.unpack_from('<8I', data, phoff + i * phentsize)
        if p_type == 1 and p_flags & 1:  # PT_LOAD, executable
            yield p_vaddr, data[p_offset:p_offset + p_filesz]


def functions():
    starts = []
    if os.path.isdir(DECOMP):
        for n in os.listdir(DECOMP):
            if n.endswith('.c'):
                try:
                    starts.append(int(n[:-2], 16))
                except ValueError:
                    pass
    return sorted(starts)


def owner(starts, a):
    i = bisect.bisect_right(starts, a) - 1
    if i < 0:
        return '?'
    s = starts[i]
    return 'FUN_%08x%s' % (s, '' if a - s <= 0x2000 else ' (?)')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--jal', action='append', default=[], type=lambda x: int(x, 0))
    ap.add_argument('--store-off', action='append', default=[], type=lambda x: int(x, 0))
    ap.add_argument('--load-off', action='append', default=[], type=lambda x: int(x, 0))
    ap.add_argument('--li', action='append', default=[], type=lambda x: int(x, 0))
    ap.add_argument('--addr', action='append', default=[], type=lambda x: int(x, 0))
    a = ap.parse_args()
    data = open(EXE, 'rb').read()
    starts = functions()
    hits = 0
    for base, seg in segments(data):
        lui = {}  # register -> (address of lui, upper half)
        for off in range(0, len(seg) - 3, 4):
            w, = struct.unpack_from('<I', seg, off)
            pc = base + off
            op = w >> 26
            rs = (w >> 21) & 31
            rt = (w >> 16) & 31
            imm = w & 0xffff
            simm = imm - 0x10000 if imm & 0x8000 else imm
            out = None
            if op == 3 and a.jal:
                t = ((pc + 4) & 0xf0000000) | ((w & 0x3ffffff) << 2)
                if t in a.jal:
                    out = 'jal 0x%x' % t
            elif op in STORES and (simm & 0xffffffff) in [x & 0xffffffff for x in a.store_off]:
                out = '%s r%d, 0x%x(r%d)' % (STORES[op], rt, imm, rs)
            elif op in LOADS and (simm & 0xffffffff) in [x & 0xffffffff for x in a.load_off]:
                out = '%s r%d, 0x%x(r%d)' % (LOADS[op], rt, imm, rs)
            elif op in (9, 0xd) and rs == 0 and imm in a.li:
                out = '%s r%d, zero, 0x%x' % ('addiu' if op == 9 else 'ori', rt, imm)
            if a.addr:
                if op == 0xf:
                    lui[rt] = (pc, imm << 16)
                elif op in (9, 0x23, 0x2b, 0x31, 0x39, 0x24, 0x20, 0x25, 0x21) and rs in lui and pc - lui[rs][0] < 0x40:
                    full = (lui[rs][1] + simm) & 0xffffffff
                    if full in a.addr:
                        out = 'address 0x%x (lui at 0x%x)' % (full, lui[rs][0])
            if out:
                hits += 1
                print('0x%08x  %-28s  in %s' % (pc, out, owner(starts, pc)))
    print('%d hit(s)' % hits, file=sys.stderr)


if __name__ == '__main__':
    main()

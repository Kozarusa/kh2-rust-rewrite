"""find_fm_iso.py -- check the machine's Kingdom Hearts II disc images and look for the Final Mix one.
The Final Mix image (boot executable SLPM_666.75) is machine path KH2_ISO_FM in local/machine.ps1. The 2026-10-06 note
that it sat in Documents/ChatGPT is stale: since 2026-10-22 it is next to the USA image in the AGENTS Public folder.

Usage:  python tools/find_fm_iso.py [folder ...]
Looks in the folders of KH2_ISO and KH2_ISO_FM (when set) and in the given folders, top level and one folder deep, for
files over 1 GB with an ISO9660 volume, and prints one line per disc image found:
    <path> | boot executable <name> | <USA | FINAL MIX | other>
and, as its last line, either `FM_ISO=<path>` (the first Final Mix image found) or `FM_ISO=none`.
The boot executable comes from SYSTEM.CNF (BOOT2 = cdrom0:<name>;1): SLUS_210.05 is the USA release,
SLPM_666.75 is Final Mix (other SLPM/SLPS discs, such as KH1, are 'other'). Reads only the volume descriptor, the root directory and SYSTEM.CNF.
"""
import os, struct, sys


def bases():
    out = []
    for name in ('KH2_ISO', 'KH2_ISO_FM'):
        v = os.environ.get(name)
        if not v:
            import machine
            v = machine._load()[0].get(name)
        if v:
            out.append(os.path.dirname(v))
    out += sys.argv[1:]
    if not out:
        sys.exit('no folder to search: set KH2_ISO / KH2_ISO_FM in local/machine.ps1 or pass folders')
    return list(dict.fromkeys(out))


SECTOR = 2048


def root_entries(f):
    f.seek(16 * SECTOR)
    pvd = f.read(SECTOR)
    if pvd[1:6] != b'CD001':
        return None
    rec = pvd[156:156 + 34]
    lba, size = struct.unpack_from('<I', rec, 2)[0], struct.unpack_from('<I', rec, 10)[0]
    f.seek(lba * SECTOR)
    data = f.read(size)
    out, i = {}, 0
    while i < len(data):
        n = data[i]
        if n == 0:
            i = (i // SECTOR + 1) * SECTOR
            continue
        elba = struct.unpack_from('<I', data, i + 2)[0]
        esize = struct.unpack_from('<I', data, i + 10)[0]
        nl = data[i + 32]
        name = data[i + 33:i + 33 + nl].decode('ascii', 'replace')
        out[name.split(';')[0].upper()] = (elba, esize)
        i += n
    return out


def boot_name(path):
    try:
        with open(path, 'rb') as f:
            ents = root_entries(f)
            if not ents or 'SYSTEM.CNF' not in ents:
                return None
            lba, size = ents['SYSTEM.CNF']
            f.seek(lba * SECTOR)
            cnf = f.read(min(size, 4096)).decode('ascii', 'replace')
    except OSError:
        return None
    for line in cnf.splitlines():
        if line.strip().upper().startswith('BOOT2'):
            v = line.split('=', 1)[1].strip()
            return v.split('\\')[-1].split(';')[0].upper()
    return None


def main():
    found = None
    cands = []
    for base in bases():
        for root, dirs, files in os.walk(base):
            depth = root[len(base):].count(os.sep)
            if depth > 1:
                dirs[:] = []
                continue
            dirs[:] = [d for d in dirs if d not in ('.git', 'target', 'target-trial', 'decomp', 'ghidra_projects')]
            for n in files:
                p = os.path.join(root, n)
                try:
                    if os.path.getsize(p) > 1 << 30:
                        cands.append(p)
                except OSError:
                    pass
    for p in cands:
        b = boot_name(p)
        if b is None:
            continue
        kind = 'USA' if b == 'SLUS_210.05' else ('FINAL MIX' if b == 'SLPM_666.75' else 'other')
        print('%s | boot executable %s | %s' % (p, b, kind))
        if kind == 'FINAL MIX' and not found:
            found = p
    print('FM_ISO=%s' % (found or 'none'))


if __name__ == '__main__':
    main()

"""machine.py -- machine paths for the Python tools (the twin of tools/env.ps1).

The paths live in the untracked local/machine.env (written by `powershell -File tools/env.ps1` from local/machine.ps1).
A variable already set in the environment wins. The file is looked up in this checkout's local/ first, then in the
main checkout's (found through git's common directory), so the tools work from any worktree.

    import machine
    exe = machine.path('KH2_DECOMP', 'input', 'SLUS_210.05')
"""
import os
import subprocess
import sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..'))


def main_root():
    try:
        out = subprocess.run(['git', '-C', ROOT, 'rev-parse', '--path-format=absolute', '--git-common-dir'],
                             capture_output=True, text=True, check=True).stdout.strip()
        return os.path.normpath(os.path.dirname(out))
    except (OSError, subprocess.CalledProcessError):
        return ROOT


def local(rel):
    """The file local/<rel> of this checkout if it exists, else the main checkout's (which may not exist)."""
    for base in (ROOT, main_root()):
        p = os.path.join(base, 'local', rel)
        if os.path.exists(p):
            return p
    return os.path.join(main_root(), 'local', rel)


def _load():
    vals = {}
    f = local('machine.env')
    if os.path.exists(f):
        with open(f, encoding='utf-8-sig') as fh:
            for line in fh:
                line = line.strip()
                if line and not line.startswith('#') and '=' in line:
                    k, v = line.split('=', 1)
                    vals[k.strip()] = v.strip()
    return vals, f


def get(name):
    """The value of a machine variable; exits with a clear message when it is not set."""
    v = os.environ.get(name)
    if v:
        return v
    vals, f = _load()
    if vals.get(name):
        return vals[name]
    sys.exit('machine path %s is not set: add it to local/machine.ps1 (see tools/machine.example.ps1), then run '
             '`powershell -NoProfile -ExecutionPolicy Bypass -File tools/env.ps1` to write %s' % (name, f))


def path(name, *parts):
    """get(name) joined with parts; exits when the result does not exist."""
    p = os.path.join(get(name), *parts)
    if not os.path.exists(p):
        sys.exit('%s does not exist (from machine path %s)' % (p, name))
    return p

"""Capture the PCSX2 window (the original game) to a png.
usage: python tools/pcsx2_shot.py <out.png> [--pid N] [--list]     (window must be visible, not minimised)
Brings the PCSX2 render window to the front, grabs its client area with PIL.ImageGrab (real pixels: the process is made DPI aware).
--pid N: only windows of that process (the rig passes the PCSX2 it started, `kh2-rig frames` crops and scales the picture to 4:3
and 640x448 afterwards); the largest visible client area of the process wins. Without --pid the first window titled PCSX2 or
Kingdom Hearts II is taken."""
import sys, ctypes, ctypes.wintypes as w, time
from PIL import ImageGrab

args = sys.argv[1:]
pid = None
if '--pid' in args:
    i = args.index('--pid'); pid = int(args[i + 1]); del args[i:i + 2]
args = [a for a in args if a != '--list']
if len(args) != 1:
    sys.exit(__doc__)
out = args[0]

u = ctypes.windll.user32
try:
    ctypes.windll.shcore.SetProcessDpiAwareness(2)
except Exception:
    u.SetProcessDPIAware()

def client_size(h):
    r = w.RECT(); u.GetClientRect(h, ctypes.byref(r)); return r.right, r.bottom

def title(h):
    n = u.GetWindowTextLengthW(h); b = ctypes.create_unicode_buffer(n + 1); u.GetWindowTextW(h, b, n + 1); return b.value

def cls(h):
    b = ctypes.create_unicode_buffer(256); u.GetClassNameW(h, b, 256); return b.value

def descendants(h):
    out = []
    @ctypes.WINFUNCTYPE(ctypes.c_bool, w.HWND, w.LPARAM)
    def cb2(c, l):
        out.append(c); return True
    u.EnumChildWindows(h, cb2, 0)
    return out

found = []
@ctypes.WINFUNCTYPE(ctypes.c_bool, w.HWND, w.LPARAM)
def cb(h, l):
    if not u.IsWindowVisible(h):
        return True
    n = u.GetWindowTextLengthW(h); b = ctypes.create_unicode_buffer(n + 1); u.GetWindowTextW(h, b, n + 1)
    if pid is not None:
        p = w.DWORD(0); u.GetWindowThreadProcessId(h, ctypes.byref(p))
        if p.value == pid and n > 0:
            found.append((h, b.value))
    elif 'Kingdom Hearts II' in b.value or 'PCSX2' in b.value:
        found.append((h, b.value))
    return True
u.EnumWindows(cb, 0)
if not found:
    sys.exit('no PCSX2 window')
if pid is not None:
    found.sort(key=lambda f: -(client_size(f[0])[0] * client_size(f[0])[1]))
top = found[0][0]
u.ShowWindow(top, 9)
# A window that sits partly off the screen is moved to the top left of the primary monitor first (the grab sees only the screen).
r = w.RECT(); u.GetWindowRect(top, ctypes.byref(r))
if r.left < 0 or r.top < 0:
    u.SetWindowPos(top, 0, 0, 0, 0, 0, 0x0001 | 0x0004)   # SWP_NOSIZE | SWP_NOZORDER
u.SetForegroundWindow(top)
time.sleep(0.4)
# The picture is the largest native child window (the emulator's display widget: no menu bar, no status bar); else the client area.
h = top
kids = [c for c in descendants(top) if u.IsWindowVisible(c) and client_size(c)[0] > 0]
if '--list' in sys.argv:
    for c in [top] + kids:
        rr = w.RECT(); u.GetWindowRect(c, ctypes.byref(rr))
        print(hex(c), cls(c), repr(title(c)), client_size(c), (rr.left, rr.top))
if kids:
    big = max(kids, key=lambda c: client_size(c)[0] * client_size(c)[1])
    if client_size(big)[0] * client_size(big)[1] * 2 >= client_size(top)[0] * client_size(top)[1]:
        h = big
cw, ch = client_size(h); pt = w.POINT(0, 0); u.ClientToScreen(h, ctypes.byref(pt))
im = ImageGrab.grab((pt.x, pt.y, pt.x + cw, pt.y + ch), all_screens=True).convert('RGB')
im.save(out); print('saved', out, im.size, found[0][1])

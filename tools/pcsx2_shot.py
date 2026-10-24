"""Capture the PCSX2 window (the original game) to a png.
usage: python tools/pcsx2_shot.py <out.png> [--pid N]     (window must be visible, not minimised)
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
h = found[0][0]
u.ShowWindow(h, 9); u.SetForegroundWindow(h)
time.sleep(0.4)
cw, ch = client_size(h); pt = w.POINT(0, 0); u.ClientToScreen(h, ctypes.byref(pt))
im = ImageGrab.grab((pt.x, pt.y, pt.x + cw, pt.y + ch), all_screens=True).convert('RGB')
im.save(out); print('saved', out, im.size, found[0][1])

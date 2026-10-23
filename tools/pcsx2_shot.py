"""Capture the PCSX2 window (the original game) to a png.
usage: python tools/pcsx2_shot.py <out.png>     (window must be visible, not minimised)
Brings the PCSX2 render window to the front, grabs its client area with PIL.ImageGrab."""
import sys, ctypes, ctypes.wintypes as w
from PIL import ImageGrab
u = ctypes.windll.user32
found = []
@ctypes.WINFUNCTYPE(ctypes.c_bool, w.HWND, w.LPARAM)
def cb(h, l):
    if u.IsWindowVisible(h):
        n = u.GetWindowTextLengthW(h); b = ctypes.create_unicode_buffer(n + 1); u.GetWindowTextW(h, b, n + 1)
        if 'Kingdom Hearts II' in b.value or 'PCSX2' in b.value: found.append((h, b.value))
    return True
u.EnumWindows(cb, 0)
if not found: sys.exit('no PCSX2 window')
h = found[0][0]
u.ShowWindow(h, 9); u.SetForegroundWindow(h)
import time; time.sleep(0.4)
r = w.RECT(); u.GetClientRect(h, ctypes.byref(r)); pt = w.POINT(0, 0); u.ClientToScreen(h, ctypes.byref(pt))
im = ImageGrab.grab((pt.x, pt.y, pt.x + r.right, pt.y + r.bottom))
im.save(sys.argv[1]); print('saved', sys.argv[1], im.size, found[0][1])

"""Extract frames from the user's reference videos (original game footage).
usage: python tools/vidframes.py <video> <t1> [t2 ...]   -> <main checkout>/local/videoframes/<name>_<t>.png (seconds, 60 fps)
       python tools/vidframes.py <video> --sheet [n]      -> contact sheet of n frames (default 12)
<video> is a path, or a bare file name in the folder of machine path KH2_VIDEOS (e.g. 4.mp4).
Needs opencv-python-headless (installed). The output stays untracked (local/)."""
import sys, os, cv2, numpy as np
import machine
v = sys.argv[1]
if not os.path.exists(v):
    v = machine.path('KH2_VIDEOS', v)
name = os.path.splitext(os.path.basename(v))[0]
out = os.path.join(machine.main_root(), 'local', 'videoframes'); os.makedirs(out, exist_ok=True)
c = cv2.VideoCapture(v); fps = c.get(5); n = int(c.get(7))
if sys.argv[2] == '--sheet':
    k = int(sys.argv[3]) if len(sys.argv) > 3 else 12
    fr = []
    for i in range(k):
        c.set(1, int(i * (n - 1) / (k - 1))); ok, im = c.read(); im = cv2.resize(im, (360, 243))
        cv2.putText(im, f'{c.get(1)/fps:.1f}s', (5, 20), cv2.FONT_HERSHEY_SIMPLEX, 0.6, (0, 255, 255), 2); fr.append(im)
    rows = [np.hstack(fr[r*3:(r+1)*3]) for r in range(k // 3)]
    cv2.imwrite(os.path.join(out, f'{name}_sheet.jpg'), np.vstack(rows))
else:
    for t in sys.argv[2:]:
        c.set(1, int(float(t) * fps)); ok, im = c.read()
        cv2.imwrite(os.path.join(out, f'{name}_{t}.png'), im)

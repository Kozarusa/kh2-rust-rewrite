"""Loudness of a 32-bit float stereo wav (the loopback recorder's output, kh2-looprec).
usage: python tools/wavstats.py <file.wav>
Prints the shape, peak, RMS and the count of clipped samples, then the peak of every second (48 kHz assumed)."""
import numpy as np, sys
d = open(sys.argv[1], 'rb').read()
i = d.find(b'data') + 8
a = np.frombuffer(d[i:i + (len(d) - i) // 8 * 8], dtype='<f4').reshape(-1, 2)
print(a.shape, 'peak', abs(a).max(), 'rms', float(np.sqrt((a ** 2).mean())), 'clip', int((abs(a) >= 0.99).sum()))
for k in range(len(a) // 48000):
    print(k, round(float(abs(a[k * 48000:(k + 1) * 48000]).max()), 3))

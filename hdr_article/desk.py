# Renders the ILM "Desk.exr" sample (openexr-images, BSD) into SDR, simulated-HDR and real PQ AVIF versions.
import numpy as np, zlib, struct, subprocess, os, sys
from PIL import Image
here = os.path.dirname(os.path.abspath(__file__))
src = sys.argv[1]  # Desk.npy (float32 HxWx3 linear Rec.709)
a = np.load(src).astype(np.float64)
a = np.maximum(a, 0) * (0.75 / 1.6)  # units of SDR white; the book page sits at ~0.75
def shoulder(x, s, P):
    x = np.maximum(x, 1e-12)
    return np.where(x <= s, x, s + (P - s) * (1 - np.exp(-np.log(np.maximum(x, s) / s) * s / (P - s))))
def mapc(x, s, P):
    L = (x @ np.array([0.2126, 0.7152, 0.0722]))[..., None]
    fL = shoulder(L, s, P) / P; sc = fL / np.maximum(L, 1e-12); t = fL * fL
    return np.minimum(1, x * sc * (1 - t) + shoulder(x, s, P) / P * t) * P
def srgb(x):
    x = np.clip(x, 0, 1); return np.where(x <= 0.0031308, 12.92 * x, 1.055 * x ** (1 / 2.4) - 0.055)
def save_jpg(lin, fn):
    Image.fromarray((srgb(lin) * 255 + 0.5).astype(np.uint8)).save(os.path.join(here, 'desk', fn), quality=88)
H = 4.0
save_jpg(mapc(a, 0.6, 1.0), 'desk_sdr.jpg')
save_jpg(mapc(a, 0.6, 1.0) / H, 'desk_sim_sdr.jpg')
save_jpg(mapc(a, 0.6, H) / H, 'desk_sim_hdr.jpg')
# real HDR: SDR white at 203 nits, 5x headroom -> ~1000 nits peak
WHITE = 203.0
hdr = mapc(a, 0.6, 5.0) * WHITE
M = np.array([[0.6274, 0.3293, 0.0433], [0.0691, 0.9195, 0.0114], [0.0164, 0.0880, 0.8956]])
h = np.maximum(hdr @ M.T, 0)
m1 = 2610 / 16384; m2 = 2523 / 4096 * 128; c1 = 3424 / 4096; c2 = 2413 / 4096 * 32; c3 = 2392 / 4096 * 32
Y = np.clip(h / 10000, 0, 1) ** m1
pq = ((c1 + c2 * Y) / (1 + c3 * Y)) ** m2
Hh, Ww = pq.shape[:2]
arr = (pq * 65535 + 0.5).astype('>u2')
raw = b''.join(b'\x00' + arr[j].tobytes() for j in range(Hh))
def chunk(t, d): return struct.pack('>I', len(d)) + t + d + struct.pack('>I', zlib.crc32(t + d) & 0xffffffff)
png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', Ww, Hh, 16, 2, 0, 0, 0)) + chunk(b'cICP', bytes([9, 16, 0, 1])) + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b'')
p16 = os.path.join(here, 'desk', 'desk_pq16.png')
open(p16, 'wb').write(png)
subprocess.run(['ffmpeg', '-y', '-hide_banner', '-loglevel', 'error', '-i', p16, '-vf', 'scale=out_color_matrix=bt2020:out_range=tv,format=yuv444p10le',
                '-c:v', 'libaom-av1', '-still-picture', '1', '-crf', '14', '-cpu-used', '4', '-color_primaries', 'bt2020', '-color_trc', 'smpte2084',
                '-colorspace', 'bt2020nc', '-color_range', 'tv', os.path.join(here, 'desk', 'desk_hdr.avif')], check=True)
print('peak nits', hdr.max())

import numpy as np, zlib, struct, subprocess
W,H=1600,900
s=np.fromfile('scene.f32',dtype=np.float32).reshape(H,W,3).astype(np.float64)
print('min/max',s.min(),s.max(), 'median', np.median(s))
def rrt(x): return (x*(2.51*x+0.03))/(x*(2.43*x+0.59)+0.14)
def aces(x,peak):
    s=0.2*peak; x=np.maximum(x,1e-9)
    return np.where(x<=s, x, s+(peak-s)*(1-np.exp(-np.log(np.maximum(x,s)/s)*s/(peak-s))))
def srgb_enc(x):
    x=np.clip(x,0,1); return np.where(x<=0.0031308,12.92*x,1.055*x**(1/2.4)-0.055)
EXP=2.5
def png8(a,fn):
    from PIL import Image; Image.fromarray((np.clip(a,0,1)*255+0.5).astype(np.uint8)).save(fn)
# SDR previews (peak 100, same exposure scaled for sdr white ~100)
png8(srgb_enc(aces(s*EXP,100)/100),'prev_sdr_aces.png')
png8(srgb_enc(s*EXP/100),'prev_sdr_clip.png')
# HDR master: ACES to 1000 nits
def mapc(x,P):
    L=(x@np.array([0.2126,0.7152,0.0722]))[...,None]
    fL=aces(L,P)/P; sc=fL/np.maximum(L,1e-9); t=fL*fL
    return np.minimum(1,x*sc*(1-t)+aces(x,P)/P*t)*P
hdr=mapc(s*EXP,1000.0)
M=np.array([[0.6274,0.3293,0.0433],[0.0691,0.9195,0.0114],[0.0164,0.0880,0.8956]])
h2020=hdr@M.T
m1=2610/16384; m2=2523/4096*128; c1=3424/4096; c2=2413/4096*32; c3=2392/4096*32
Y=np.clip(h2020/10000,0,1)**m1
pq=((c1+c2*Y)/(1+c3*Y))**m2
print('pq max',pq.max())
a=(pq*65535+0.5).astype('>u2')
raw=b''.join(b'\x00'+a[j].tobytes() for j in range(H))
def chunk(t,d): return struct.pack('>I',len(d))+t+d+struct.pack('>I',zlib.crc32(t+d)&0xffffffff)
png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',W,H,16,2,0,0,0))+chunk(b'cICP',bytes([9,16,0,1]))+chunk(b'IDAT',zlib.compress(raw,9))+chunk(b'IEND',b'')
open('hdr_pq16.png','wb').write(png)

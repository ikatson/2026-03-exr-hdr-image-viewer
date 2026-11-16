import base64, os
os.chdir(os.path.dirname(os.path.abspath(__file__)))
b64 = lambda p: base64.b64encode(open(p, 'rb').read()).decode()
strip = lambda s: s.split('if (typeof module')[0]
t = open('template.html').read()
t = t.replace('/*__SCENE_JS__*/', strip(open('scene.js').read()) + '\n' + strip(open('night.js').read()))
t = t.replace('__AVIF_B64__', b64('scene_hdr.avif'))
t = t.replace('__DESK_SIM_SDR__', b64('desk/desk_sim_sdr.jpg')).replace('__DESK_SIM_HDR__', b64('desk/desk_sim_hdr.jpg'))
t = t.replace('__DESK_SDR__', b64('desk/desk_sdr.jpg')).replace('__DESK_AVIF__', b64('desk/desk_hdr.avif'))
assert '__' not in ''.join(x for x in t.split('base64,')[0::1] if False) 
open('what_is_hdr.html', 'w').write(t)
print(len(t))

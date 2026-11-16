// A dark "movie" frame graded the way HDR films usually are: almost everything under a few nits,
// a couple of small light sources a few hundred nits. Values are absolute nits (what PQ would carry).
function renderNight(W, H) {
  const out = new Float32Array(W * H * 3);
  const aspect = W / H;
  const lamp = { x: 0.68, y: 0.30 };
  function hash(x, y) { const s = Math.sin(x * 127.1 + y * 311.7) * 43758.5453; return s - Math.floor(s); }
  for (let j = 0; j < H; j++) {
    const v = (j + 0.5) / H;
    for (let i = 0; i < W; i++) {
      const u = (i + 0.5) / W;
      let r, g, b;
      // skyline height
      const bx = Math.floor(u * 9);
      const roof = 0.18 + 0.22 * hash(bx, 1.7);
      const ground = 0.72;
      if (v < ground && v < roof) {
        // night sky
        const L = 0.06 + 0.7 * Math.pow(Math.min(1, v / 0.4), 1.6);
        r = L * 0.45; g = L * 0.6; b = L * 1.0;
        const dx = (u - 0.18) * aspect, dy = v - 0.12;
        const dm = Math.sqrt(dx * dx + dy * dy);
        if (dm < 0.035) { r = 90; g = 92; b = 100; }
        else { const gl = 1.2 * Math.exp(-(dm - 0.035) / 0.05); r += gl * 0.8; g += gl * 0.85; b += gl; }
        if (hash(i * 0.37, j * 0.91) > 0.9993) { r = g = b = 6; }
      } else if (v < ground) {
        // buildings, a few lit windows
        const L = 0.025 + 0.04 * hash(bx, 4.2);
        r = L * 0.7; g = L * 0.75; b = L * 0.95;
        const wx = Math.floor(u * 60), wy = Math.floor(v * 40);
        const fx = u * 60 - wx, fy = v * 40 - wy;
        if (fx > 0.25 && fx < 0.75 && fy > 0.2 && fy < 0.7 && hash(wx, wy) > 0.9 && v > roof + 0.03) {
          const Lw = 6 + 18 * hash(wy, wx);
          r = Lw; g = Lw * 0.72; b = Lw * 0.42;
        }
      } else {
        // wet street
        const L = 0.06 + 0.04 * hash(Math.floor(u * 200), Math.floor(v * 80));
        r = L * 0.8; g = L * 0.85; b = L;
        const px = (u - lamp.x) * aspect, py = (v - 0.80) * 2.2;
        const pool = 6 * Math.exp(-(px * px + py * py) / 0.02);
        r += pool; g += pool * 0.8; b += pool * 0.55;
        // reflection streak of the lamp
        const streak = Math.exp(-Math.pow((u - lamp.x) / 0.008, 2)) * Math.exp(-(v - ground) / 0.12) * 40;
        r += streak; g += streak * 0.8; b += streak * 0.5;
      }
      // lamp post + head + glow (drawn over everything)
      if (Math.abs(u - lamp.x) < 0.004 && v > lamp.y && v < 0.80) { r = 0.05; g = 0.05; b = 0.05; }
      const lx = (u - lamp.x) * aspect, ly = v - lamp.y;
      const dl = Math.sqrt(lx * lx + ly * ly);
      if (dl < 0.018) { r = 500; g = 400; b = 260; }
      else if (v < 0.80) { const gl = 25 * Math.exp(-(dl - 0.018) / 0.03) + 1.5 * Math.exp(-dl / 0.15); r += gl; g += gl * 0.8; b += gl * 0.55; }
      // two people under the lamp, in silhouette with a faint rim light
      for (const [cx, hgt] of [[0.62, 0.17], [0.735, 0.15]]) {
        const top = 0.80 - hgt;
        const head = Math.hypot((u - cx) * aspect, v - (top + 0.02)) < 0.022;
        const body = Math.abs(u - cx) < 0.018 * (1 + (v - top) * 1.5) && v > top + 0.035 && v < 0.80;
        if (head || body) {
          const rim = Math.abs(u - cx) > 0.012 && u < cx === (cx > lamp.x) ? 0 : 0;
          const side = (u - cx) * (lamp.x - cx) > 0 ? 1.2 : 0.15;
          r = 0.12 * side + rim; g = 0.1 * side; b = 0.08 * side;
        }
      }
      const o = (j * W + i) * 3;
      out[o] = r; out[o + 1] = g; out[o + 2] = b;
    }
  }
  return out;
}
if (typeof module !== "undefined") module.exports = { renderNight };

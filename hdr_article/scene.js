// Procedural HDR test scene. Returns linear Rec.709 RGB in absolute nits (cd/m²).
// A dim room, a window onto a sunny landscape, a bare light bulb and a patch of sunlight.
function renderScene(W, H) {
  const out = new Float32Array(W * H * 3);
  const sunX = 0.79, sunY = 0.24;
  const aspect = W / H;
  const win = { x0: 0.40, x1: 0.93, y0: 0.10, y1: 0.70 };
  const bulb = { x: 0.17, y: 0.36 };
  function hash(x, y) {
    const s = Math.sin(x * 127.1 + y * 311.7) * 43758.5453;
    return s - Math.floor(s);
  }
  function vnoise(x, y) {
    const xi = Math.floor(x), yi = Math.floor(y);
    const xf = x - xi, yf = y - yi;
    const u = xf * xf * (3 - 2 * xf), v = yf * yf * (3 - 2 * yf);
    const a = hash(xi, yi), b = hash(xi + 1, yi), c = hash(xi, yi + 1), d = hash(xi + 1, yi + 1);
    return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
  }
  function fbm(x, y) {
    let s = 0, amp = 0.5;
    for (let i = 0; i < 4; i++) { s += amp * vnoise(x, y); x *= 2.03; y *= 2.03; amp *= 0.5; }
    return s;
  }
  for (let j = 0; j < H; j++) {
    const v = (j + 0.5) / H;
    for (let i = 0; i < W; i++) {
      const u = (i + 0.5) / W;
      let r, g, b;
      const inWin = u > win.x0 && u < win.x1 && v > win.y0 && v < win.y1;
      // window mullions
      const mullV = Math.abs(u - (win.x0 + win.x1) / 2) < 0.008;
      const mullH = Math.abs(v - 0.40) < 0.010;
      if (inWin && !mullV && !mullH) {
        const horizon = 0.53 + 0.025 * Math.sin(u * 9.0) + 0.015 * Math.sin(u * 23.0 + 1.0);
        const near = 0.60 + 0.03 * Math.sin(u * 5.0 + 2.0);
        if (v < horizon) {
          // sky
          const t = Math.min(1, Math.max(0, (v - win.y0) / (horizon - win.y0)));
          const L = 2200 + 5200 * t * t;
          r = L * (0.38 + 0.40 * t); g = L * (0.56 + 0.30 * t); b = L * 1.0;
          // clouds
          const cn = fbm(u * 7.0, v * 14.0);
          const band = Math.exp(-Math.pow((v - 0.30) / 0.09, 2));
          const cloud = Math.max(0, (cn - 0.52) * 4.0) * band;
          if (cloud > 0) {
            const c = Math.min(1, cloud);
            const lit = 7000 + 9000 * fbm(u * 11 + 3, v * 11);
            r = r * (1 - c) + lit * c; g = g * (1 - c) + lit * 0.98 * c; b = b * (1 - c) + lit * 0.95 * c;
          }
          // sun + glow
          const dx = (u - sunX) * aspect, dy = v - sunY;
          const d = Math.sqrt(dx * dx + dy * dy);
          if (d < 0.022) { r = 400000; g = 380000; b = 340000; }
          else {
            const glow = 60000 * Math.exp(-(d - 0.022) / 0.012) + 9000 * Math.exp(-d / 0.08);
            r += glow; g += glow * 0.93; b += glow * 0.80;
          }
        } else if (v < near) {
          // distant hills, hazy
          const h = 0.7 + 0.3 * fbm(u * 20, v * 20);
          const L = 1800 * h;
          r = L * 0.62; g = L * 0.75; b = L * 0.95;
        } else {
          // sunlit meadow
          const h = 0.6 + 0.5 * fbm(u * 40, v * 60);
          const L = 3200 * h;
          r = L * 0.55; g = L * 0.85; b = L * 0.30;
        }
      } else if (u > win.x0 - 0.02 && u < win.x1 + 0.02 && v > win.y0 - 0.03 && v < win.y1 + 0.04) {
        // window frame (painted wood, lit from outside a bit)
        const sill = v > win.y1;
        const L = sill ? 140 : 45;
        r = L * 0.95; g = L * 0.92; b = L * 0.86;
      } else if (v > 0.80) {
        // table top, wood, lit by the window
        const grain = 0.75 + 0.25 * fbm(u * 3, v * 90);
        const fall = 10 + 45 * Math.exp(-Math.pow((u - 0.66) / 0.35, 2));
        let L = fall * grain;
        r = L * 0.85; g = L * 0.55; b = L * 0.32;
        // sunlight patch (skewed parallelogram)
        const pu = u - (v - 0.80) * 0.9;
        if (pu > 0.52 && pu < 0.74 && v < 0.97) {
          const soft = Math.min(1, (pu - 0.52) / 0.01, (0.74 - pu) / 0.01, (0.97 - v) / 0.01);
          const S = 9000 * soft * grain;
          r += S * 0.85; g += S * 0.60; b += S * 0.35;
        }
        // sheet of paper in shade
        if (u > 0.12 && u < 0.30 && v > 0.84 && v < 0.95) { r = 55; g = 55; b = 53; }
      } else {
        // wall: lit by the window and by the bulb
        const dw = Math.max(0, win.x0 - u) + 0.35 * Math.max(0, u - win.x1);
        const fromWin = 6 + 45 * Math.exp(-dw / 0.18) * (0.75 + 0.25 * Math.min(1, Math.abs(v - 0.4) / 0.4));
        const bx = (u - bulb.x) * aspect, by = v - bulb.y;
        const db = Math.sqrt(bx * bx + by * by);
        const fromBulb = 120 * Math.exp(-db / 0.12);
        const L = fromWin + fromBulb * 0.7;
        r = L * 0.95 + fromBulb * 0.35; g = L * 0.86 + fromBulb * 0.15; b = L * 0.72;
        // bookshelf with books in deep shade
        if (u > 0.02 && u < 0.30 && v > 0.56 && v < 0.78) {
          const shelfLine = Math.abs(v - 0.67) < 0.006 || v > 0.772;
          if (shelfLine) { r = 9; g = 6; b = 4; }
          else {
            const k = Math.floor(u * 70);
            const hue = hash(k, 3.3);
            const lum = 1.5 + 4 * hash(k, 7.7);
            const gap = hash(k, 9.1) < 0.12;
            if (gap) { r = 0.4; g = 0.3; b = 0.25; }
            else {
              r = lum * (0.3 + 0.7 * Math.abs(Math.sin(hue * 6.28)));
              g = lum * (0.3 + 0.7 * Math.abs(Math.sin(hue * 6.28 + 2.1)));
              b = lum * (0.3 + 0.7 * Math.abs(Math.sin(hue * 6.28 + 4.2)));
            }
          }
        }
        // bare light bulb
        if (db < 0.025) { r = 24000; g = 17000; b = 9000; }
        else if (db < 0.06) {
          const glow = 3000 * Math.exp(-(db - 0.025) / 0.01);
          r += glow; g += glow * 0.72; b += glow * 0.4;
        }
        // cord
        if (Math.abs(u - bulb.x) < 0.002 && v < bulb.y - 0.02) { r = 2; g = 2; b = 2; }
      }
      const o = (j * W + i) * 3;
      out[o] = r; out[o + 1] = g; out[o + 2] = b;
    }
  }
  return out;
}
if (typeof module !== "undefined") module.exports = { renderScene };

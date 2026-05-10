// @ts-check

/**
 * @callback Shader
 * @param {number} f - Horizontal position normalized (0 to 1)
 * @returns {string} - CSS Color string
 */

/**
 * @param {string} id
 * @param {Shader} shader
 */
function drawCanvas(id, shader) {
  const canvas = /** @type {HTMLCanvasElement} */ (document.getElementById(id));
  if (!canvas) return;

  const W = 1024,
    H = 100;
  canvas.width = W;
  canvas.height = H;
  canvas.style.imageRendering = "pixelated";

  const c = canvas.getContext("2d");
  if (!c) return;

  c.imageSmoothingEnabled = false;

  const steps = 256;
  for (let i = 0; i < steps; i++) {
    const f = i / (steps - 1);
    c.fillStyle = shader(f);
    // Draw logic: mapped to the 1024 width
    c.fillRect((i / steps) * W, 0, W / steps + 1, H);
  }
}

// --- Shaders ---

const hslShader = (f) => `hsl(0 0 ${f * 100})`;

const rgbShader = (f) => {
  const color = Math.floor(f * 255);
  return `rgb(${color}, ${color}, ${color})`;
};

const linearQuantizedShader = (f) => {
  // 1. Map to linear brightness
  const linearBrightness = Math.pow(f, 2.2);

  // 2. TRUNCATE in linear space (The "Screw Up" check)
  // This forces the value into one of 256 physical intensity buckets
  const quantizedLinear = Math.floor(linearBrightness * 255) / 255;

  // 3. Convert back to sRGB
  let srgb = Math.pow(quantizedLinear, 1 / 2.2);

  // 4. Final 8-bit quantization for display
  const color = Math.floor(srgb * 255);
  return `rgb(${color}, ${color}, ${color})`;
};

const linearBrightnessGradient = (f) => {
  const color = Math.pow(f, 1 / 2.2);
  return `rgb(${color * 100}%, ${color * 100}%, ${color * 100}%)`;
};

// --- Execution ---

function main() {
  drawCanvas("hsl-gradient", hslShader);
  drawCanvas("rgb-0-255-gradient", rgbShader);
  drawCanvas("simulated-gradient", linearQuantizedShader);
  drawCanvas("linear-brightness-gradient", linearBrightnessGradient);
}

try {
  main();
} catch (e) {
  const errElem = document.getElementById("error");
  if (errElem) errElem.textContent = `error: ${e}`;
  throw e;
}

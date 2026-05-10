// @ts-check

function hslGradient() {
  /** @type {HTMLCanvasElement} */
  // @ts-ignore
  let canvas = document.getElementById("hsl-gradient");
  const W = 1024;
  const H = 100;
  canvas.width = W;
  canvas.height = H;
  /** @type {CanvasRenderingContext2D} */
  // @ts-ignore
  let c = canvas.getContext("2d");
  for (let i = 0; i < 256; i++) {
    let fcolor = i / 255;
    let brightness = `${fcolor * 100}%`;
    console.log(brightness);
    c.fillStyle = `hsl(0 0 ${brightness})`;
    c.fillRect((i / 256) * W, 0, ((i + 1) / 256) * W, H);
  }
}

function rgbFullGradient() {
  /** @type {HTMLCanvasElement} */
  // @ts-ignore
  let canvas = document.getElementById("rgb-0-255-gradient");
  const W = 1024;
  const H = 100;
  canvas.width = W;
  canvas.height = H;
  /** @type {CanvasRenderingContext2D} */
  // @ts-ignore
  let c = canvas.getContext("2d");
  for (let i = 0; i < 256; i++) {
    let fcolor = i / 255;
    let color = Math.floor(fcolor * 255);
    c.fillStyle = `rgb(${color}, ${color}, ${color})`;
    c.fillRect((i / 256) * W, 0, ((i + 1) / 256) * W, H);
  }
}

// If sRGB was linear
function simulatedGradient() {
  /** @type {HTMLCanvasElement} */
  // @ts-ignore
  let canvas = document.getElementById("simulated-gradient");
  const W = 1024;
  const H = 100;
  canvas.width = W;
  canvas.height = H;
  /** @type {CanvasRenderingContext2D} */
  // @ts-ignore
  let c = canvas.getContext("2d");
  for (let i = 0; i < 256; i++) {
    let fcolor = i / 255;
    // srgb color would be Math.floor(fcolor * 255)
    // linear brightness
    let linearBrightness = Math.pow(fcolor, 2.2);
    // truncate linear brightness
    let color = Math.floor(linearBrightness * 255) / 255;
    // convert back to srgb
    color = Math.pow(color, 1 / 2.2);
    // truncate again to u8
    color = Math.floor(color * 255);

    c.fillStyle = `rgb(${color}, ${color}, ${color})`;
    c.fillRect((i / 256) * W, 0, ((i + 1) / 256) * W, H);
  }
}

function main() {
  hslGradient();
  rgbFullGradient();
  simulatedGradient();
}

try {
  main();
} catch (e) {
  document.getElementById("error").textContent = `error: ${e}`;
  throw e;
}

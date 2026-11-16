const {renderScene}=require('./scene.js');
const W=+process.argv[2],H=+process.argv[3];
const t=Date.now();const d=renderScene(W,H);console.error('ms',Date.now()-t);
require('fs').writeFileSync('scene.f32',Buffer.from(d.buffer));

// GARGANTUA — Schwarzschild Black Hole Raytracer
// Real-time null-geodesic raytracing: renderer orchestration, cinematic
// camera, HUD, parameters, quality tiers, audio, and shot mode.

import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
import { ShaderPass } from 'three/addons/postprocessing/ShaderPass.js';
import { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js';
import { RAY_VERT, RAY_FRAG, COMPOSITE_VERT, COMPOSITE_FRAG } from './shaders.js';

/* ================================================================ consts */
const DEG = Math.PI / 180;
const STORE_KEY = 'gargantua.params.v1';

const TIERS = {
  standard:  { name: 'STANDARD',  short: 'STD',  steps: 200, dpr: 1 },
  high:      { name: 'HIGH',      short: 'HIGH', steps: 320, dpr: 1.5 },
  cinematic: { name: 'CINEMATIC', short: 'CINE', steps: 460, dpr: 2 },
};
const TIER_ORDER = ['standard', 'high', 'cinematic'];

const PRESETS = {
  poster: { r: 24, inc: 38, az: 30 },
  edge:   { r: 26, inc: 6,  az: 10 },
  polar:  { r: 28, inc: 82, az: 0 },
  close:  { r: 9,  inc: 14, az: 55 },
};
const PRESET_KEYS = ['poster', 'edge', 'polar', 'close'];

// Closed cinematic loop: 8 spherical keyframes (r, inclination°, azimuth°)
const CINE_R   = [58, 36, 26, 14, 20, 34, 46, 36];
const CINE_INC = [12, 6, 24, 14, 52, 80, 35, 8];
const CINE_AZ  = [-30, 10, 55, 100, 150, 200, 270, 330]; // unwrapped

/* ============================================================ URL params */
const qs = new URLSearchParams(location.search);
const qRaw = (qs.get('q') || '').toLowerCase();
let tierName = qRaw ? (TIERS[qRaw] ? qRaw : 'high') : 'cinematic';
const urlSteps = (() => {
  if (!qs.has('steps')) return null;
  const v = parseFloat(qs.get('steps'));
  if (!Number.isFinite(v)) return null;
  return Math.round(Math.min(600, Math.max(60, v)));
})();
const shotMode = qs.has('shot');
const camRaw = (qs.get('cam') || '').toLowerCase();
const urlCam = PRESETS[camRaw] ? camRaw : null;
const noCine = qs.has('nocine');
const urlCtime = qs.has('ctime') ? Math.max(0, parseFloat(qs.get('ctime')) || 0) : null;
const urlDebug = (() => {
  if (!qs.has('debug')) return null;
  const v = parseFloat(qs.get('debug'));
  if (!Number.isFinite(v)) return null;
  return Math.round(Math.min(9, Math.max(0, v)));
})();
// URL 亮度/氛围覆盖：?bright=1.5（吸积盘亮度）、?star=1.4（星空亮度）、?sky=0.08（天光底色）
const urlBright = (() => {
  if (!qs.has('bright')) return null;
  const v = parseFloat(qs.get('bright'));
  return Number.isFinite(v) ? Math.min(3, Math.max(0.2, v)) : null;
})();
const urlStarBright = (() => {
  if (!qs.has('star')) return null;
  const v = parseFloat(qs.get('star'));
  return Number.isFinite(v) ? Math.min(3, Math.max(0.2, v)) : null;
})();
const urlSkyFloor = (() => {
  if (!qs.has('sky')) return null;
  const v = parseFloat(qs.get('sky'));
  return Number.isFinite(v) ? Math.min(0.15, Math.max(0, v)) : null;
})();
// bg=1：纯背景模式（嵌入空状态/启动页使用）——隐藏 HUD/开场/提示，自动电影镜头，
// 关闭交互与音频，仅保留 WebGL 渲染。
const bgMode = qs.has('bg');

const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
if (reducedMotion) document.body.classList.add('reduced');
if (shotMode) document.body.classList.add('shot', 'ready');
if (bgMode) document.body.classList.add('ready');

/* ============================================================= DOM refs */
const canvas = document.getElementById('view');
const hudEl = document.getElementById('hud');
const paramsEl = document.getElementById('params');
const pRowsEl = document.getElementById('p-rows');
const deckEl = document.getElementById('deck');
const deckModeEl = document.getElementById('deck-mode');
const deckHintEl = document.getElementById('deck-hint');
const hintEl = document.getElementById('hint');
const toastEl = document.getElementById('toast');
const fatalEl = document.getElementById('fatal');
const fatalTitleEl = document.getElementById('fatal-title');
const fatalMsgEl = document.getElementById('fatal-msg');
const btnCine = document.getElementById('btn-cine');
const btnOrbit = document.getElementById('btn-orbit');
const btnQuality = document.getElementById('btn-quality');
const btnParams = document.getElementById('btn-params');
const btnHud = document.getElementById('btn-hud');
const btnSound = document.getElementById('btn-sound');
const btnReset = document.getElementById('btn-reset');
const tDist = document.getElementById('t-dist');
const tInc = document.getElementById('t-inc');
const tSteps = document.getElementById('t-steps');
const tProf = document.getElementById('t-prof');
const tFps = document.getElementById('t-fps');
const clockVal = document.getElementById('clock-val');

/* ============================================================== helpers */
function clamp(v, a, b) { return Math.min(b, Math.max(a, v)); }
function easeCubic(t) { return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2; }

let toastTimer = 0;
function toast(msg, ms = 4200) {
  toastEl.textContent = msg;
  toastEl.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toastEl.classList.remove('show'), ms);
}

function showFatal(title, msg) {
  fatalTitleEl.textContent = title;
  fatalMsgEl.textContent = msg;
  fatalEl.classList.add('show');
}
document.getElementById('fatal-retry').addEventListener('click', () => location.reload());
document.getElementById('fatal-lower').addEventListener('click', () => {
  const idx = TIER_ORDER.indexOf(tierName);
  const lower = TIER_ORDER[Math.max(0, idx - 1)];
  const p = new URLSearchParams(location.search);
  p.set('q', lower);
  location.search = p.toString();
});

/* ============================================================== runtime */
let renderer, fsScene, fsCam, camera, controls, composer, bloomPass, compPass;
let rayUni, compUni;
let cineMode = false;
let cineTime = urlCtime || 0;
let cineBlend = null;
let tShader = 0;
let rafId = 0;
let shotDone = false;
let readyFired = false;
let hintBarShown = false;
let deckHintShown = false;
let contextLost = false;      // WebGL context lost — rendering halted until reload
let contextRestored = false;  // restore handled exactly once (single reload)
let renderFaulted = false;    // composer.render() threw — loop stopped, fatal shown
const clock = new THREE.Clock();
const bufSize = new THREE.Vector2();
const tmpVec = new THREE.Vector3();

const flight = { active: false, t: 0, dur: 2.6, from: new THREE.Vector3(), to: new THREE.Vector3() };

function sphToVec(r, inc, az, out) {
  const i = inc * DEG, a = az * DEG;
  out.set(r * Math.cos(i) * Math.sin(a), r * Math.sin(i), r * Math.cos(i) * Math.cos(a));
  return out;
}

/* ========================================================= three.js init */
function initThree() {
  renderer = new THREE.WebGLRenderer({
    canvas, antialias: false, powerPreference: 'high-performance',
  });
  renderer.outputColorSpace = THREE.LinearSRGBColorSpace; // ACES done manually
  renderer.toneMapping = THREE.NoToneMapping;
  renderer.debug.onShaderError = (gl, program, vs, fs) => {
    const log = (gl.getShaderInfoLog(fs) || '') + '\n' + (gl.getShaderInfoLog(vs) || '');
    showFatal('SHADER COMPILE ERROR', (log.trim() || 'Unknown shader error').slice(0, 900));
  };

  canvas.addEventListener('webglcontextlost', (e) => {
    e.preventDefault();
    contextLost = true;
    if (rafId) { cancelAnimationFrame(rafId); rafId = 0; }
    showFatal('WEBGL CONTEXT LOST',
      'The GPU context was lost and rendering has stopped. Press RETRY to reinitialize, or LOWER QUALITY to reduce the render load first.');
  });
  canvas.addEventListener('webglcontextrestored', () => {
    // Safe, deterministic recovery: rebuild renderer/composer/targets from scratch.
    if (contextRestored) return;
    contextRestored = true;
    location.reload();
  });

  const gl = renderer.getContext();
  const halfOK = renderer.capabilities.isWebGL2 &&
    !!(gl.getExtension('EXT_color_buffer_float') || gl.getExtension('EXT_color_buffer_half_float'));
  if (!halfOK) toast('HDR BUFFER UNAVAILABLE — LDR FALLBACK ACTIVE');

  // fullscreen ray scene: single 2x2 quad, orthographic camera
  fsCam = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  fsScene = new THREE.Scene();
  rayUni = {
    uRes:        { value: new THREE.Vector2(1, 1) },
    uTime:       { value: 0 },
    uCamPos:     { value: new THREE.Vector3(4.49, 2.72, 25.46) },
    uCamTarget:  { value: new THREE.Vector3(0, 0, 0) },
    uFov:        { value: 1 / Math.tan(44 * DEG / 2) },
    uSteps:      { value: 460 },
    uRotSign:    { value: 1 },
    uDebug:      { value: 0 },
    uDin:        { value: 2.75 },
    uDout:       { value: 40 },
    uDopMax:     { value: 1.85 },
    uOpNear:     { value: 0.90 },
    uOpFar:      { value: 0.80 },
    uDiskBright: { value: 1 },
    uStarBright: { value: 1 },
    uSkyFloor:   { value: 0.04 },
    uRotSpeed:   { value: 1 },
  };
  const rayMat = new THREE.ShaderMaterial({
    vertexShader: RAY_VERT,
    fragmentShader: RAY_FRAG,
    uniforms: rayUni,
    depthTest: false,
    depthWrite: false,
  });
  fsScene.add(new THREE.Mesh(new THREE.PlaneGeometry(2, 2), rayMat));

  // observer camera: only provides position / FOV, never renders geometry
  camera = new THREE.PerspectiveCamera(44, innerWidth / innerHeight, 0.01, 200);
  camera.position.set(4.49, 2.72, 25.46);

  controls = new OrbitControls(camera, canvas);
  controls.target.set(0, 0, 0);
  controls.enableDamping = true;
  controls.dampingFactor = 0.06;
  controls.minDistance = 1.62;
  controls.maxDistance = 150;
  controls.rotateSpeed = 0.55;
  controls.zoomSpeed = 0.7;
  controls.enablePan = false;
  controls.autoRotate = true;
  controls.autoRotateSpeed = 0.12;

  renderer.getDrawingBufferSize(bufSize);
  const rt = new THREE.WebGLRenderTarget(bufSize.x || 2, bufSize.y || 2, {
    type: halfOK ? THREE.HalfFloatType : THREE.UnsignedByteType,
  });
  composer = new EffectComposer(renderer, rt);
  composer.addPass(new RenderPass(fsScene, fsCam));
  bloomPass = new UnrealBloomPass(new THREE.Vector2(bufSize.x || 2, bufSize.y || 2), 0.55, 0.35, 0.55);
  composer.addPass(bloomPass);
  compUni = {
    tDiffuse:  { value: null },
    uRes:      { value: new THREE.Vector2(1, 1) },
    uTime:     { value: 0 },
    uVignette: { value: 1 },
    uGrain:    { value: 0.045 },
    uCA:       { value: 0.0028 },
  };
  compPass = new ShaderPass({
    uniforms: compUni,
    vertexShader: COMPOSITE_VERT,
    fragmentShader: COMPOSITE_FRAG,
  });
  composer.addPass(compPass);
}

/* ================================================================ resize */
function currentDpr() {
  return Math.min(window.devicePixelRatio || 1, TIERS[tierName].dpr);
}

function onResize() {
  // 容器隐藏（iframe/窗口尺寸为 0）时钳制到 1×1，避免零尺寸 framebuffer
  // 触发 GL_INVALID_FRAMEBUFFER_OPERATION 刷屏
  const w = Math.max(1, innerWidth), h = Math.max(1, innerHeight);
  const dpr = currentDpr();
  renderer.setPixelRatio(dpr);
  renderer.setSize(w, h);
  composer.setPixelRatio(dpr);
  composer.setSize(w, h);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
  renderer.getDrawingBufferSize(bufSize);
  rayUni.uRes.value.copy(bufSize);
  compUni.uRes.value.copy(bufSize);
  updatePanelMaxHeight();
}

function updatePanelMaxHeight() {
  const deckTop = deckEl.getBoundingClientRect().top;
  const maxH = Math.max(180, deckTop - 88 - 12);
  paramsEl.style.maxHeight = maxH + 'px';
}

/* ============================================================ parameters */
const P = {}; // runtime parameter values
const urlOverrideKeys = new Set(); // fields overridden by URL for this run — never persisted
/** URL 氛围参数覆盖（bright/star/sky），优先于本地存储且不落盘 */
function applyUrlAmbient() {
  if (urlBright !== null) { P.diskBright = urlBright; urlOverrideKeys.add('diskBright'); }
  if (urlStarBright !== null) { P.starBright = urlStarBright; urlOverrideKeys.add('starBright'); }
  if (urlSkyFloor !== null) { P.skyFloor = urlSkyFloor; urlOverrideKeys.add('skyFloor'); }
}

const PARAM_DEFS = [
  { key: 'steps', label: 'GEODESIC STEPS', min: 60, max: 600, step: 10, def: null,
    fmt: v => v.toFixed(0), apply: v => { rayUni.uSteps.value = v; } },
  { key: 'din', label: 'DISK INNER EDGE', min: 2.0, max: 4.0, step: 0.05, def: 2.75,
    fmt: v => v.toFixed(2), apply: v => { rayUni.uDin.value = v; } },
  { key: 'dout', label: 'DISK OUTER EDGE', min: 10, max: 80, step: 1, def: 40,
    fmt: v => v.toFixed(0), apply: v => { rayUni.uDout.value = v; } },
  { key: 'dopMax', label: 'DOPPLER BOOST', min: 1, max: 3, step: 0.05, def: 1.85,
    fmt: v => v.toFixed(2), apply: v => { rayUni.uDopMax.value = v; } },
  { key: 'opNear', label: 'DISK OPACITY·INNER', min: 0.50, max: 1, step: 0.01, def: 0.90,
    fmt: v => v.toFixed(2), apply: v => { rayUni.uOpNear.value = v; } },
  { key: 'opFar', label: 'DISK OPACITY·OUTER', min: 0.30, max: 1, step: 0.01, def: 0.80,
    fmt: v => v.toFixed(2), apply: v => { rayUni.uOpFar.value = v; } },
  { key: 'diskBright', label: 'DISK BRIGHTNESS', min: 0.2, max: 3, step: 0.05, def: 1,
    fmt: v => v.toFixed(2), apply: v => { rayUni.uDiskBright.value = v; } },
  { key: 'starBright', label: 'STARFIELD BRIGHTNESS', min: 0.2, max: 3, step: 0.05, def: 1,
    fmt: v => v.toFixed(2), apply: v => { rayUni.uStarBright.value = v; } },
  { key: 'skyFloor', label: 'SKY FLOOR GLOW', min: 0, max: 0.15, step: 0.005, def: 0.04,
    fmt: v => v.toFixed(3), apply: v => { rayUni.uSkyFloor.value = v; } },
  { key: 'rotSpeed', label: 'DISK ROTATION', min: 0, max: 3, step: 0.05, def: 1,
    fmt: v => v.toFixed(2), apply: v => { rayUni.uRotSpeed.value = v; } },
  { key: 'bloomStrength', label: 'BLOOM STRENGTH', min: 0, max: 1.5, step: 0.05, def: 0.55,
    fmt: v => v.toFixed(2), apply: v => { bloomPass.strength = v; } },
  { key: 'bloomRadius', label: 'BLOOM RADIUS', min: 0, max: 1, step: 0.05, def: 0.35,
    fmt: v => v.toFixed(2), apply: v => { bloomPass.radius = v; } },
  { key: 'bloomThreshold', label: 'BLOOM THRESHOLD', min: 0, max: 1, step: 0.05, def: 0.55,
    fmt: v => v.toFixed(2), apply: v => { bloomPass.threshold = v; } },
  { key: 'vignette', label: 'VIGNETTE', min: 0, max: 1.5, step: 0.05, def: 1,
    fmt: v => v.toFixed(2), apply: v => { compUni.uVignette.value = v; } },
  { key: 'grain', label: 'FILM GRAIN', min: 0, max: 0.15, step: 0.005, def: 0.045,
    fmt: v => v.toFixed(3), apply: v => { compUni.uGrain.value = v; } },
  { key: 'ca', label: 'CHROMATIC ABERRATION', min: 0, max: 0.01, step: 0.0005, def: 0.0028,
    fmt: v => v.toFixed(4), apply: v => { compUni.uCA.value = v; } },
  { key: 'fov', label: 'LENS FOV', min: 25, max: 80, step: 1, def: 44,
    fmt: v => v.toFixed(0) + '°',
    apply: v => {
      camera.fov = v;
      camera.updateProjectionMatrix();
      rayUni.uFov.value = 1 / Math.tan(v * DEG / 2);
    } },
  { key: 'maxDist', label: 'MAX DISTANCE', min: 40, max: 300, step: 5, def: 150,
    fmt: v => v.toFixed(0) + ' RS', apply: v => { controls.maxDistance = v; } },
  { key: 'orbitSpeed', label: 'AUTO-ORBIT SPEED', min: 0, max: 1, step: 0.02, def: 0.12,
    fmt: v => v.toFixed(2), apply: v => { controls.autoRotateSpeed = v; } },
  { key: 'cineSeg', label: 'CINE SEGMENT', min: 4, max: 30, step: 1, def: 11,
    fmt: v => v.toFixed(0) + 's', apply: () => {} },
  { key: 'debug', label: 'DEBUG VIEW', min: 0, max: 9, step: 1, def: 0,
    fmt: v => v.toFixed(0),
    apply: v => {
      rayUni.uDebug.value = v;
      // data-inspection views must not be polluted by bloom
      bloomPass.enabled = Math.round(v) <= 2;
    } },
];

const rowEls = {}; // key -> { input, valEl }

function paramDefault(def) {
  return def.key === 'steps' ? TIERS[tierName].steps : def.def;
}

function loadStorage() {
  let data = null;
  try { data = JSON.parse(localStorage.getItem(STORE_KEY) || 'null'); } catch (e) { data = null; }
  for (const def of PARAM_DEFS) {
    let v = paramDefault(def);
    if (data && Number.isFinite(data[def.key])) v = clamp(data[def.key], def.min, def.max);
    P[def.key] = v;
  }
}

function saveStorage() {
  let data = {};
  try {
    data = JSON.parse(localStorage.getItem(STORE_KEY) || '{}') || {};
    if (typeof data !== 'object') data = {};
  } catch (e) { data = {}; }
  for (const def of PARAM_DEFS) {
    if (urlOverrideKeys.has(def.key)) continue; // URL overrides stay run-scoped
    data[def.key] = P[def.key];
  }
  try { localStorage.setItem(STORE_KEY, JSON.stringify(data)); } catch (e) { /* private mode */ }
}

function applyParam(def, updateRow = true) {
  def.apply(P[def.key]);
  if (updateRow && rowEls[def.key]) {
    rowEls[def.key].input.value = P[def.key];
    rowEls[def.key].valEl.textContent = def.fmt(P[def.key]);
  }
}

function buildParamsPanel() {
  for (const def of PARAM_DEFS) {
    const row = document.createElement('div');
    row.className = 'p-row';
    const meta = document.createElement('div');
    meta.className = 'p-meta';
    const label = document.createElement('label');
    label.textContent = def.label;
    label.htmlFor = 'p-in-' + def.key;
    const val = document.createElement('span');
    val.className = 'p-val';
    val.id = 'p-val-' + def.key;
    meta.append(label, val);
    const input = document.createElement('input');
    input.type = 'range';
    input.id = 'p-in-' + def.key;
    input.min = def.min; input.max = def.max; input.step = def.step;
    input.setAttribute('aria-label', def.label);
    input.addEventListener('input', () => {
      P[def.key] = clamp(parseFloat(input.value), def.min, def.max);
      urlOverrideKeys.delete(def.key); // manual control takes over from any URL override
      applyParam(def, false);
      val.textContent = def.fmt(P[def.key]);
      if (def.key === 'steps') updateHudTelemetry();
      saveStorage();
    });
    row.append(meta, input);
    pRowsEl.appendChild(row);
    rowEls[def.key] = { input, valEl: val };
  }
}

function applyAllParams() {
  for (const def of PARAM_DEFS) applyParam(def);
}

btnReset.addEventListener('click', () => {
  try { localStorage.removeItem(STORE_KEY); } catch (e) { /* ignore */ }
  urlOverrideKeys.clear();
  for (const def of PARAM_DEFS) P[def.key] = paramDefault(def);
  // URL overrides remain the top initial priority for this run, even after RESET
  if (urlSteps !== null) { P.steps = urlSteps; urlOverrideKeys.add('steps'); }
  if (urlDebug !== null) { P.debug = urlDebug; urlOverrideKeys.add('debug'); }
  applyAllParams();
  updateHudTelemetry();
  toast('PARAMETERS RESET');
});

/* ========================================================== quality tier */
function setTier(name, announce = true) {
  tierName = name;
  const tier = TIERS[name];
  urlOverrideKeys.delete('steps'); // tier button owns steps from now on — persistable
  P.steps = tier.steps;
  applyParam(PARAM_DEFS[0]);
  btnQuality.textContent = tier.short;
  btnQuality.setAttribute('aria-label', 'Render quality: ' + tier.name + '. Activate to cycle.');
  onResize();
  updateHudTelemetry();
  saveStorage();
  if (announce) toast('RENDER PROFILE — ' + tier.name + ' · ' + tier.steps + ' STEPS');
}

btnQuality.addEventListener('click', () => {
  const idx = TIER_ORDER.indexOf(tierName);
  setTier(TIER_ORDER[(idx + 1) % TIER_ORDER.length]);
});

/* ================================================================== HUD */
let fpsSmooth = 0, fpsFrames = 0, fpsTime = 0, lowFpsSeconds = 0, lowFpsToasted = false;

function updateHudTelemetry() {
  const d = camera.position.length();
  tDist.textContent = d.toFixed(2) + ' RS';
  tInc.textContent = (Math.asin(clamp(camera.position.y / d, -1, 1)) / DEG).toFixed(1) + '°';
  tSteps.textContent = String(Math.round(P.steps));
  tProf.textContent = TIERS[tierName].name;
  tFps.textContent = fpsSmooth > 0 ? Math.round(fpsSmooth) + ' FPS' : '—';
}
setInterval(updateHudTelemetry, 250);

const bootTime = performance.now();
setInterval(() => {
  const s = Math.floor((performance.now() - bootTime) / 1000) % 86400;
  const hh = String(Math.floor(s / 3600)).padStart(2, '0');
  const mm = String(Math.floor((s % 3600) / 60)).padStart(2, '0');
  const ss = String(s % 60).padStart(2, '0');
  clockVal.textContent = hh + ':' + mm + ':' + ss;
}, 1000);

function trackFps(dt) {
  fpsFrames++; fpsTime += dt;
  if (fpsTime >= 1) {
    const fps = fpsFrames / fpsTime;
    fpsSmooth = fpsSmooth > 0 ? fpsSmooth * 0.6 + fps * 0.4 : fps;
    if (fpsSmooth < 24) {
      lowFpsSeconds++;
      if (lowFpsSeconds >= 5 && !lowFpsToasted && !shotMode) {
        lowFpsToasted = true;
        toast('LOW FRAME RATE — CONSIDER LOWER QUALITY');
      }
    } else lowFpsSeconds = 0;
    fpsFrames = 0; fpsTime = 0;
  }
}

/* ======================================================= cinematic path */
function cr(p0, p1, p2, p3, t) {
  const t2 = t * t, t3 = t2 * t;
  return 0.5 * (2 * p1 + (-p0 + p2) * t +
    (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2 +
    (-p0 + 3 * p1 - 3 * p2 + p3) * t3);
}
function arrAt(arr, j) { const n = arr.length; return arr[((j % n) + n) % n]; }
function unwrapNear(a, ref) {
  while (a - ref > 180) a -= 360;
  while (a - ref < -180) a += 360;
  return a;
}

function cinePosAt(time, out) {
  const seg = Math.max(1, P.cineSeg);
  const s = ((time / seg) % 8 + 8) % 8;
  const i = Math.floor(s), t = s - i;
  const r = cr(arrAt(CINE_R, i - 1), arrAt(CINE_R, i), arrAt(CINE_R, i + 1), arrAt(CINE_R, i + 2), t);
  const inc = cr(arrAt(CINE_INC, i - 1), arrAt(CINE_INC, i), arrAt(CINE_INC, i + 1), arrAt(CINE_INC, i + 2), t);
  const a1 = arrAt(CINE_AZ, i);
  const a0 = unwrapNear(arrAt(CINE_AZ, i - 1), a1);
  const a2 = unwrapNear(arrAt(CINE_AZ, i + 1), a1);
  const a3 = unwrapNear(arrAt(CINE_AZ, i + 2), a2);
  const az = cr(a0, a1, a2, a3, t);
  return sphToVec(r, inc, az, out);
}

function updateCine(dt) {
  const pos = cinePosAt(cineTime, tmpVec);
  if (cineBlend) {
    cineBlend.t += dt;
    const k = easeCubic(Math.min(cineBlend.t / 2, 1));
    pos.lerpVectors(cineBlend.from, pos, k);
    if (cineBlend.t >= 2) cineBlend = null;
  }
  camera.position.copy(pos);
  camera.lookAt(0, 0, 0);
}

/* ======================================================= flights/presets */
function flyToPreset(name) {
  const pr = PRESETS[name];
  if (!pr) return;
  if (cineMode) breakCine();
  flight.active = true;
  flight.t = 0;
  flight.from.copy(camera.position);
  sphToVec(pr.r, pr.inc, pr.az, flight.to);
  controls.enabled = false;
}

function updateFlight(dt) {
  flight.t += dt;
  const k = easeCubic(Math.min(flight.t / flight.dur, 1));
  camera.position.lerpVectors(flight.from, flight.to, k);
  camera.lookAt(0, 0, 0);
  if (flight.t >= flight.dur) {
    flight.active = false;
    controls.enabled = true;
  }
}

/* =========================================================== cine toggle */
function syncDeck() {
  deckModeEl.textContent = cineMode ? 'CINEMATIC SEQUENCE' : 'NAVIGATION';
  btnCine.classList.toggle('active', cineMode);
  btnCine.setAttribute('aria-pressed', String(cineMode));
  btnOrbit.classList.toggle('active', controls.autoRotate && !cineMode);
  btnOrbit.setAttribute('aria-pressed', String(controls.autoRotate && !cineMode));
}

function breakCine() {
  if (!cineMode) return;
  cineMode = false;
  cineBlend = null;
  controls.enabled = true;
  syncDeck();
  if (!deckHintShown) {
    deckHintShown = true;
    deckHintEl.classList.add('show');
    setTimeout(() => deckHintEl.classList.remove('show'), 6000);
  }
}

function toggleCine() {
  if (cineMode) { breakCine(); return; }
  flight.active = false;
  cineMode = true;
  controls.enabled = false;
  controls.autoRotate = false;
  cineBlend = { t: 0, from: camera.position.clone() };
  syncDeck();
  syncMusicToCine();
}

function toggleOrbit() {
  if (cineMode) {
    // leave cinematic first so OrbitControls are live, then start orbiting
    breakCine();
    controls.autoRotate = true;
  } else {
    controls.autoRotate = !controls.autoRotate;
  }
  syncDeck();
}

document.getElementById('btn-poster').addEventListener('click', () => flyToPreset('poster'));
document.getElementById('btn-edge').addEventListener('click', () => flyToPreset('edge'));
document.getElementById('btn-polar').addEventListener('click', () => flyToPreset('polar'));
document.getElementById('btn-close').addEventListener('click', () => flyToPreset('close'));
btnCine.addEventListener('click', toggleCine);
btnOrbit.addEventListener('click', toggleOrbit);

function toggleParams() {
  const hidden = paramsEl.classList.toggle('hidden');
  btnParams.classList.toggle('active', !hidden);
  btnParams.setAttribute('aria-pressed', String(!hidden));
  if (!hidden) updatePanelMaxHeight();
}
btnParams.addEventListener('click', toggleParams);

function toggleHud() {
  const off = hudEl.classList.toggle('off');
  btnHud.classList.toggle('active', !off);
  btnHud.setAttribute('aria-pressed', String(!off));
}
btnHud.addEventListener('click', toggleHud);

/* ============================================================ audio */
const introAud = document.createElement('audio');
const mainAud = document.createElement('audio');
if (!bgMode) {
  introAud.preload = 'auto';
  introAud.src = 'audio/gargantua-intro.mp3';
  mainAud.preload = 'auto';
  mainAud.loop = true;
  mainAud.src = (() => {
    const a = document.createElement('audio');
    if (a.canPlayType('audio/ogg; codecs="opus"')) return 'audio/gargantua-main.opus';
    if (a.canPlayType('audio/mpeg')) return 'audio/gargantua-main.mp3';
    if (a.canPlayType('audio/ogg; codecs="vorbis"')) return 'audio/gargantua-main.ogg';
    return 'audio/gargantua-main.mp3';
  })();
  document.body.append(introAud, mainAud);
}

let soundOn = false;
let stingPlayed = false;
let stingBroken = false;
let volTimer = 0;
let blockedTimer = 0;

function fadeVolume(aud, target, dur) {
  clearInterval(volTimer);
  const start = aud.volume;
  const t0 = performance.now();
  volTimer = setInterval(() => {
    const k = Math.min(1, (performance.now() - t0) / (dur * 1000));
    aud.volume = start + (target - start) * k;
    if (k >= 1) clearInterval(volTimer);
  }, 50);
}

function soundBlocked() {
  soundOn = false;
  clearInterval(volTimer);
  introAud.pause();
  mainAud.pause();
  btnSound.textContent = '⚠ SOUND: BLOCKED';
  btnSound.classList.remove('active');
  btnSound.setAttribute('aria-pressed', 'false');
  clearTimeout(blockedTimer);
  blockedTimer = setTimeout(() => { btnSound.textContent = '🔇 SOUND: OFF'; }, 2500);
}

// Position the main loop: cinematic → cineTime % 176; first hand-off from the
// intro sting outside cinematic → 0; otherwise keep the paused position.
// Metadata-safe: defers the seek to loadedmetadata when necessary.
let metaListenPending = false;
function alignMainToCine(fromSting = false) {
  if (!cineMode && !fromSting) return;
  const doSeek = () => {
    metaListenPending = false;
    try { mainAud.currentTime = cineMode ? (cineTime % 176) : 0; } catch (e) { /* seek unavailable */ }
  };
  if (mainAud.readyState >= 1) doSeek();
  else if (!metaListenPending) {
    metaListenPending = true;
    mainAud.addEventListener('loadedmetadata', doSeek, { once: true });
  }
}

function syncMusicToCine() {
  if (soundOn && cineMode && !mainAud.paused) alignMainToCine(false);
}

introAud.addEventListener('ended', () => {
  if (!soundOn) return;
  alignMainToCine(true);
  mainAud.volume = 0.6;
  mainAud.play().then(() => fadeVolume(mainAud, 0.85, 0.15)).catch(soundBlocked);
});
introAud.addEventListener('error', () => { stingBroken = true; });
mainAud.addEventListener('error', () => { if (soundOn) soundBlocked(); });

async function setSound(on) {
  if (on) {
    soundOn = true;
    clearTimeout(blockedTimer);
    btnSound.textContent = '🔊 SOUND: ON';
    btnSound.classList.add('active');
    btnSound.setAttribute('aria-pressed', 'true');
    const introVisible = !document.body.classList.contains('ready');
    try {
      if (introVisible && !stingPlayed && !stingBroken) {
        stingPlayed = true;
        introAud.currentTime = 0;
        await introAud.play();
      } else {
        alignMainToCine(false); // cinematic: re-align; navigation: resume in place
        mainAud.volume = 0.35;
        await mainAud.play();
        fadeVolume(mainAud, 0.85, 0.8);
      }
    } catch (e) {
      soundBlocked();
    }
  } else {
    soundOn = false;
    clearInterval(volTimer);
    introAud.pause();
    mainAud.pause();
    btnSound.textContent = '🔇 SOUND: OFF';
    btnSound.classList.remove('active');
    btnSound.setAttribute('aria-pressed', 'false');
  }
}
btnSound.addEventListener('click', () => setSound(!soundOn));

/* =========================================================== interaction */
function manualTakeover() {
  if (flight.active) { flight.active = false; controls.enabled = true; }
  breakCine();
}
if (!bgMode) {
  canvas.addEventListener('pointerdown', manualTakeover, { capture: true });
  canvas.addEventListener('wheel', manualTakeover, { capture: true, passive: true });
  window.addEventListener('keydown', (e) => {
    const t = e.target;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable)) return;
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const k = e.key.toLowerCase();
    if (k >= '1' && k <= '4') flyToPreset(PRESET_KEYS[+k - 1]);
    else if (k === 'c') toggleCine();
    else if (k === 'r') toggleOrbit();
    else if (k === 'p') toggleParams();
    else if (k === 'm') setSound(!soundOn);
    else if (k === 'h') toggleHud();
  });
}

document.addEventListener('visibilitychange', () => {
  if (document.hidden) {
    if (rafId) { cancelAnimationFrame(rafId); rafId = 0; }
  } else if (!rafId && !shotDone && !contextLost && !renderFaulted) {
    clock.getDelta(); // discard time accumulated in the background
    rafId = requestAnimationFrame(animate);
  }
});

// 宿主（父页面）控制暂停/恢复：面板隐藏时停止 RAF，节能并释放 GPU
let pausedByHost = false;
function pauseByHost() {
  if (pausedByHost) return;
  pausedByHost = true;
  if (rafId) {
    cancelAnimationFrame(rafId);
    rafId = 0;
  }
}
function resumeByHost() {
  if (!pausedByHost) return;
  pausedByHost = false;
  if (!rafId && !shotDone && !contextLost && !renderFaulted) {
    clock.getDelta();
    rafId = requestAnimationFrame(animate);
  }
}
window.addEventListener('message', (e) => {
  const d = e.data;
  if (!d || typeof d !== 'object') return;
  if (d.type === 'gargantua-pause') pauseByHost();
  else if (d.type === 'gargantua-resume') resumeByHost();
});

/* ============================================================ main loop */
let shotFrames = 0;

function renderFault(err) {
  if (renderFaulted) return; // never rethrow or respam the overlay per frame
  renderFaulted = true;
  if (!fatalEl.classList.contains('show')) {
    showFatal('RENDER FAULT',
      'The renderer reported an unrecoverable error: ' +
      String((err && err.message) || err).slice(0, 400) +
      ' — Press RETRY to reinitialize, or LOWER QUALITY to reduce the render load first.');
  }
}

function animate() {
  rafId = requestAnimationFrame(animate);
  // 容器尺寸为 0（iframe 隐藏/窗口最小化）时跳过本帧渲染，
  // 避免零尺寸 framebuffer 的 GL 错误刷屏
  if (innerWidth === 0 || innerHeight === 0) return;
  const dt = Math.min(clock.getDelta(), 0.1);
  tShader += dt;

  if (flight.active) updateFlight(dt);
  else if (cineMode) { cineTime += dt; updateCine(dt); }
  controls.update();

  rayUni.uTime.value = tShader;
  rayUni.uCamPos.value.copy(camera.position);
  rayUni.uCamTarget.value.copy(controls.target);
  compUni.uTime.value = tShader;
  try {
    composer.render();
  } catch (err) {
    cancelAnimationFrame(rafId); // the next frame was already queued above
    rafId = 0;
    renderFault(err);
    return;
  }

  trackFps(dt);

  if (!readyFired) {
    readyFired = true;
    document.body.classList.add('ready');
    scheduleHintBar();
  }

  if (shotMode) {
    shotFrames++;
    if (shotFrames >= 4) {
      shotDone = true;
      cancelAnimationFrame(rafId);
      rafId = 0;
      updateHudTelemetry();
      document.title = 'SHOT_OK';
    }
  }
}

function scheduleHintBar() {
  if (hintBarShown || shotMode || bgMode) return;
  hintBarShown = true;
  setTimeout(() => {
    hintEl.classList.add('show');
    setTimeout(() => hintEl.classList.remove('show'), 10000);
  }, 2500);
}

/* ================================================================= boot */
function boot() {
  if (bgMode) {
    hudEl.classList.add('off');
    deckEl.style.display = 'none';
    hintEl.style.display = 'none';
    toastEl.style.display = 'none';
  }
  try {
    initThree();
  } catch (e) {
    showFatal('WEBGL UNAVAILABLE', String(e && e.message || e));
    return;
  }

  buildParamsPanel();
  loadStorage();
  // URL overrides win over storage for this run (run-scoped, not persisted)
  if (urlSteps !== null) { P.steps = urlSteps; urlOverrideKeys.add('steps'); }
  if (urlDebug !== null) { P.debug = urlDebug; urlOverrideKeys.add('debug'); }
  applyUrlAmbient();
  applyAllParams(); // syncs uniforms, panel rows, values, FOV, limits, bloom state

  btnQuality.textContent = TIERS[tierName].short;

  // camera start: cinematic by default unless nocine / cam / reduced motion
  cineMode = !noCine && !urlCam && !reducedMotion;
  if (urlCam) {
    sphToVec(PRESETS[urlCam].r, PRESETS[urlCam].inc, PRESETS[urlCam].az, camera.position);
    camera.lookAt(0, 0, 0);
  }
  if (cineMode) {
    controls.enabled = false;
    controls.autoRotate = false;
    updateCine(0);
  }
  if (reducedMotion) controls.autoRotate = false;
  syncDeck();

  onResize();
  window.addEventListener('resize', onResize);
  window.addEventListener('orientationchange', onResize);

  // manual takeover must win over cinematic mode on the very first gesture
  if (!bgMode) controls.addEventListener('start', breakCine);

  updateHudTelemetry();
  rafId = requestAnimationFrame(animate);

  // safety net: never leave the user on a black intro
  setTimeout(() => {
    if (!readyFired) {
      readyFired = true;
      document.body.classList.add('ready');
      scheduleHintBar();
    }
  }, 9000);
}

boot();

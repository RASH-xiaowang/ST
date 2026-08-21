// GARGANTUA — Schwarzschild Black Hole Raytracer
// Fullscreen null-geodesic raymarcher + final composite shader.
// Geometric units: c = G = 1, RS = 1.0 (Schwarzschild radius).

export const RAY_VERT = /* glsl */`
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = vec4(position.xy, 0.0, 1.0);
}
`;

export const RAY_FRAG = /* glsl */`
precision highp float;

varying vec2 vUv;

uniform vec2  uRes;
uniform float uTime;
uniform vec3  uCamPos;
uniform vec3  uCamTarget;
uniform float uFov;
uniform float uSteps;
uniform float uRotSign;
uniform float uDebug;
uniform float uDin;
uniform float uDout;
uniform float uDopMax;
uniform float uOpNear;
uniform float uOpFar;
uniform float uDiskBright;
uniform float uStarBright;
uniform float uSkyFloor;
uniform float uRotSpeed;

#define RS 1.0

// ---------------------------------------------------------------- hashes
float hash13(vec3 p) {
  p = fract(p * 0.1031);
  p += dot(p, p.zyx + 31.32);
  return fract((p.x + p.y) * p.z);
}
vec3 hash33(vec3 p) {
  p = fract(p * vec3(0.1031, 0.1030, 0.0973));
  p += dot(p, p.yxz + 33.33);
  return fract((p.xxy + p.yxx) * p.zyx);
}

// ------------------------------------------------------- value noise / fbm
float vnoise(vec3 p) {
  vec3 i = floor(p);
  vec3 f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  float n000 = hash13(i);
  float n100 = hash13(i + vec3(1.0, 0.0, 0.0));
  float n010 = hash13(i + vec3(0.0, 1.0, 0.0));
  float n110 = hash13(i + vec3(1.0, 1.0, 0.0));
  float n001 = hash13(i + vec3(0.0, 0.0, 1.0));
  float n101 = hash13(i + vec3(1.0, 0.0, 1.0));
  float n011 = hash13(i + vec3(0.0, 1.0, 1.0));
  float n111 = hash13(i + vec3(1.0, 1.0, 1.0));
  return mix(
    mix(mix(n000, n100, f.x), mix(n010, n110, f.x), f.y),
    mix(mix(n001, n101, f.x), mix(n011, n111, f.x), f.y),
    f.z);
}

// 5-layer FBM: frequency x2.03, offset 11.3, amplitude .5 halving per layer
float fbm(vec3 p) {
  float a = 0.5;
  float s = 0.0;
  for (int k = 0; k < 5; k++) {
    s += a * vnoise(p);
    p = p * 2.03 + 11.3;
    a *= 0.5;
  }
  return s;
}

// ----------------------------------------------------------- color helpers
vec3 blackbody(float t) {
  vec3 c = mix(vec3(0.55, 0.06, 0.01), vec3(1.0, 0.42, 0.10), smoothstep(0.0, 0.55, t));
  c = mix(c, vec3(1.0, 0.86, 0.55), smoothstep(0.50, 1.05, t));
  c = mix(c, vec3(0.85, 0.92, 1.25), smoothstep(1.05, 1.90, t));
  return c;
}

mat3 rotAxis(vec3 a, float t) {
  a = normalize(a);
  float c = cos(t), s = sin(t), ic = 1.0 - c;
  return mat3(
    ic*a.x*a.x + c,     ic*a.x*a.y + s*a.z, ic*a.x*a.z - s*a.y,
    ic*a.x*a.y - s*a.z, ic*a.y*a.y + c,     ic*a.y*a.z + s*a.x,
    ic*a.x*a.z + s*a.y, ic*a.y*a.z - s*a.x, ic*a.z*a.z + c);
}

// ----------------------------------------------------------- galaxy & stars
vec3 galaxy(vec3 dir) {
  vec3 n  = normalize(vec3(0.25, 1.0, 0.15));
  vec3 t1 = normalize(cross(n, vec3(0.0, 0.0, 1.0)));
  vec3 t2 = cross(n, t1);
  float w = dot(dir, n);
  float band = exp(-w * w * 7.0);
  vec2 uv = vec2(dot(dir, t1), dot(dir, t2));
  float cloud  = fbm(vec3(uv * 2.6, 7.0));
  float cloud2 = fbm(vec3(uv * 5.4 + cloud * 1.8, 13.0));
  float dust   = fbm(vec3(uv * 4.2 + 4.7, 21.0));
  float dustMask = smoothstep(0.42, 0.78, dust);
  vec3 col = mix(vec3(0.04, 0.07, 0.20), vec3(0.42, 0.24, 0.52),
                 smoothstep(0.30, 0.92, cloud2));
  float inten = band * (0.30 + 0.90 * cloud) * (1.0 - 0.62 * dustMask) * 1.15;
  return col * inten;
}

vec3 starLayer(vec3 dir, mat3 rot, float scale, float thresh, float soft) {
  vec3 p = rot * dir * scale;
  vec3 id = floor(p);
  vec3 f = fract(p);
  float h = hash13(id + 17.17);
  if (h < thresh) return vec3(0.0);
  vec3 sp = vec3(0.5) + 0.62 * (hash33(id + 3.71) - 0.5);
  float d2 = dot(f - sp, f - sp);
  float core = exp(-d2 * soft);
  float halo = exp(-d2 * soft * 0.10) * 0.22;
  float bright = 0.30 + 1.6 * pow(hash13(id + 9.3), 6.0);
  vec3 tint = mix(vec3(0.72, 0.84, 1.25), vec3(1.20, 0.95, 0.72), hash13(id + 5.5));
  return tint * (core + halo) * bright * smoothstep(thresh, thresh + 0.015, h);
}

vec3 starField(vec3 dir) {
  vec3 s = vec3(0.0);
  s += starLayer(dir, rotAxis(vec3(0.2, 1.0, 0.1), 0.0),  9.0, 0.952, 230.0);
  s += starLayer(dir, rotAxis(vec3(0.5, 0.8, 0.3), 1.9),  13.0, 0.952, 270.0);
  s += starLayer(dir, rotAxis(vec3(0.9, 0.3, 0.6), 3.7),  17.0, 0.953, 310.0);
  s += starLayer(dir, rotAxis(vec3(0.1, 0.6, 0.9), 5.1),  23.0, 0.968, 350.0) * 0.8;
  // rare hero stars: softer, larger warm/blue-white glow
  vec3 p = rotAxis(vec3(0.4, 1.0, 0.2), 0.7) * dir * 4.0;
  vec3 id = floor(p);
  vec3 f = fract(p);
  float h = hash13(id + 41.3);
  if (h > 0.9975) {
    vec3 sp = vec3(0.5) + 0.5 * (hash33(id + 11.1) - 0.5);
    float d2 = dot(f - sp, f - sp);
    vec3 tint = mix(vec3(0.80, 0.90, 1.30), vec3(1.25, 1.00, 0.80), hash13(id + 2.2));
    s += tint * (exp(-d2 * 150.0) * 3.2 + exp(-d2 * 20.0) * 0.85);
  }
  return s;
}

vec3 background(vec3 dir) {
  vec3 col = uSkyFloor * vec3(0.10, 0.13, 0.28);
  col += galaxy(dir);
  col += starField(dir);
  return col * uStarBright;
}

// -------------------------------------------------------------- accretion
// Novikov-Thorne-style flux (ISCO = 3 RS)
float ntFlux(float r) {
  float x = max(r, 3.001);
  return pow(x / 3.0, -3.0) * (1.0 - sqrt(3.0 / x));
}

// Turbulence pattern: warped FBM clouds, tangential streaks, dark lanes.
// Uses normalized rotated coordinate (no atan) => no branch-cut seam.
float diskPattern(vec3 q, float qr, out float turbOut) {
  vec2 n2 = q.xz / qr;
  float omega = uRotSign * 1.1 * uRotSpeed * pow(3.0 / qr, 1.5);
  float ph = omega * uTime;
  float cs = cos(ph), sn = sin(ph);
  vec2 rn = vec2(n2.x * cs - n2.y * sn, n2.x * sn + n2.y * cs);
  float det = 1.0 - smoothstep(4.0, 18.0, qr);
  float warp = fbm(vec3(rn * 1.5, 3.0));
  float rad = qr * 0.55;
  float turb = fbm(vec3(rn * 2.3 + (warp - 0.5) * 1.4 * det, rad * 0.4));
  turbOut = turb;
  turb = 0.55 + 0.45 * smoothstep(0.22, 0.88, turb);
  float arcA = fbm(vec3(rn * 3.1 + (warp - 0.5) * 2.2 * det, rad * 3.4 + 5.0));
  float arcB = fbm(vec3(rn * 22.0 + (warp - 0.5) * 3.0 * det, rad * 6.0 + 9.0));
  float streak = mix(arcA, arcA * 0.55 + arcB * 0.80, det);
  streak = 0.42 + 0.58 * smoothstep(0.20, 0.86, streak);
  float lane = fbm(vec3(rn * 5.2 + 7.3, rad * 1.15 + 2.0));
  float laneMask = 0.58 + 0.42 * smoothstep(0.30, 0.82, lane);
  return turb * streak * laneMask;
}

vec3 diskEmission(vec3 q, float qr, vec3 rayDir, out float patOut) {
  float flux = ntFlux(qr);
  float temp = pow(flux * 10.0, 0.25);
  float pat = diskPattern(q, qr, patOut);
  float fade = 1.0 - smoothstep(uDout - 14.0, uDout, qr);
  float I = flux * 11.0 * pat;
  I += exp(-pow((qr - 3.1) * 3.0, 2.0)) * 2.8;
  I *= fade;
  // relativistic effects
  float ang = atan(q.z, q.x);
  vec3 tdir = normalize(vec3(-sin(ang), 0.0, cos(ang))) * uRotSign;
  float beta = sqrt(0.5 / qr);
  float gamma = 1.0 / sqrt(max(1.0 - beta * beta, 1e-4));
  float D = 1.0 / (gamma * (1.0 - dot(tdir * beta, rayDir)));
  D = clamp(D, 0.50, uDopMax);
  float g = sqrt(max(1.0 - RS / qr, 0.0));
  return blackbody(temp * D * g) * I * (D * D * D * g);
}

// cheap hot haze, no turbulence
vec3 diskGlow(float r) {
  float flux = ntFlux(r);
  float temp = pow(flux * 10.0, 0.25);
  float g = sqrt(max(1.0 - RS / r, 0.0));
  float fade = 1.0 - smoothstep(uDout - 14.0, uDout, r);
  float I = flux * 7.0 + exp(-pow((r - 3.1) * 3.0, 2.0)) * 1.4;
  return blackbody(temp * g) * I * g * fade;
}

// ------------------------------------------------------------------- main
void main() {
  vec2 p = (gl_FragCoord.xy - 0.5 * uRes) / uRes.y;
  vec3 ro = uCamPos;
  vec3 ww = normalize(uCamTarget - ro);
  vec3 uu = normalize(cross(ww, vec3(0.0, 1.0, 0.0)));
  vec3 vv = cross(uu, ww);
  vec3 rd = normalize(p.x * uu + p.y * vv + uFov * ww);

  vec3 pos = ro;
  vec3 vel = rd;
  vec3 col = vec3(0.0);
  float trans = 1.0;
  float minR = 1e5;
  float lastR = length(ro);
  float stepsUsed = 0.0;
  float crossCount = 0.0;
  float validCount = 0.0;
  float firstAng = 0.0;
  float firstRad = 0.0;
  float firstPat = 0.0;
  int dbg = int(uDebug + 0.5);

  for (int i = 0; i < 600; i++) {
    if (float(i) >= uSteps) break;
    stepsUsed += 1.0;
    float r = length(pos);
    if (r < 1.03 * RS) { trans = 0.0; lastR = r; break; }
    if (r > 45.0 && dot(pos, vel) > 0.0) { lastR = r; break; }
    minR = min(minR, r);

    vec3 h = cross(pos, vel);
    float h2 = dot(h, h);
    float r2 = r * r;
    vec3 acc = -1.5 * RS * h2 / (r2 * r2 * r) * pos;
    float dt = max(0.012, r * mix(0.02, 0.06, smoothstep(6.0, 20.0, r)));

    // thin volume disk haze
    if (dbg != 2) {
      float absY = abs(pos.y);
      if (absY < 0.45 && r > uDin && r < uDout) {
        float density = exp(-absY * 30.0) * 0.03 *
                        (1.0 - smoothstep(10.0, max(uDout - 1.0, 11.0), r));
        col += trans * diskGlow(r) * density * dt * uDiskBright;
      }
    }

    vel = normalize(vel + acc * dt);
    vec3 npos = pos + vel * dt;

    // accretion disk plane crossing (y = 0)
    if (pos.y * npos.y <= 0.0) {
      crossCount += 1.0;
      float t = abs(pos.y) / (abs(pos.y) + abs(npos.y) + 1e-5);
      vec3 q = mix(pos, npos, t);
      float qr = length(q.xz);
      if (qr > uDin && qr < uDout) {
        validCount += 1.0;
        float ang = atan(q.z, q.x);
        float pat = 0.0;
        vec3 em = vec3(0.0);
        if (dbg != 2) em = diskEmission(q, qr, vel, pat);
        if (validCount < 1.5) { firstAng = ang; firstRad = qr; firstPat = pat; }
        if (dbg != 2) {
          float op = mix(uOpFar, uOpNear, 1.0 - smoothstep(4.0, 13.0, qr));
          op *= 1.0 - smoothstep(uDout - 14.0, uDout, qr);
          col += trans * op * em * uDiskBright;
          trans *= 1.0 - op;
        }
      }
    }

    pos = npos;
    lastR = r;
    if (trans < 0.02) break;
  }

  // thin photon ring near 1.55 RS (critical curve)
  if (dbg == 0 || dbg == 1) {
    float ring = exp(-pow((minR - 1.55) * 4.0, 2.0));
    col += trans * ring * vec3(1.0, 0.92, 0.80) * 0.05;
  }

  // lensed background sampled along final escape direction
  if (dbg == 0 || dbg == 2) {
    if (trans > 0.0) {
      float dim = clamp((lastR - 1.03) * 0.45, 0.45, 1.0);
      col += trans * background(normalize(vel)) * dim;
    }
  }

  // ------------------------------------------------------- debug outputs
  if (dbg == 3) {
    float t = clamp(stepsUsed / uSteps, 0.0, 1.0);
    vec3 hc = mix(vec3(0.02, 0.04, 0.18), vec3(0.0, 0.75, 0.9), smoothstep(0.0, 0.55, t));
    col = mix(hc, vec3(1.0, 0.35, 0.05), smoothstep(0.55, 1.0, t));
  } else if (dbg == 4) {
    float g = clamp((firstRad - uDin) / (uDout - uDin), 0.0, 1.0);
    col = validCount > 0.5 ? vec3(g) : vec3(0.0);
  } else if (dbg == 5) {
    col = validCount > 0.5 ? vec3(firstPat) : vec3(0.0);
  } else if (dbg == 6) {
    col = vec3(clamp(minR / 12.0, 0.0, 1.0), clamp(crossCount / 4.0, 0.0, 1.0), 0.0);
  } else if (dbg == 7) {
    if (validCount < 0.5) col = vec3(0.0);
    else if (validCount < 1.5) col = vec3(0.10, 0.30, 1.0);
    else if (validCount < 2.5) col = vec3(0.10, 0.90, 0.30);
    else col = vec3(1.0, 0.15, 0.10);
  } else if (dbg == 8) {
    col = validCount > 0.5
      ? 0.5 + 0.5 * sin(firstAng + vec3(0.0, 2.094, 4.188))
      : vec3(0.0);
  } else if (dbg == 9) {
    col = validCount > 0.5 ? vec3(0.5 + 0.5 * sin(firstRad * 2.2)) : vec3(0.0);
  }

  gl_FragColor = vec4(col, 1.0);
}
`;

// Final composite: chromatic aberration, manual ACES, vignette, film grain.
export const COMPOSITE_VERT = /* glsl */`
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}
`;

export const COMPOSITE_FRAG = /* glsl */`
precision highp float;

varying vec2 vUv;
uniform sampler2D tDiffuse;
uniform vec2  uRes;
uniform float uTime;
uniform float uVignette;
uniform float uGrain;
uniform float uCA;

vec3 aces(vec3 x) {
  x *= 0.95;
  return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0.0, 1.0);
}

void main() {
  vec2 uv = vUv;
  vec2 dir = uv - 0.5;
  float ca = uCA * dot(dir, dir);
  vec3 col;
  col.r = texture2D(tDiffuse, uv + dir * ca).r;
  col.g = texture2D(tDiffuse, uv).g;
  col.b = texture2D(tDiffuse, uv - dir * ca).b;

  col = aces(col);

  float aspect = uRes.x / uRes.y;
  float vig = 1.0 - smoothstep(0.30, 1.30, length(dir * vec2(aspect, 1.0)) * 1.15);
  col *= mix(1.0, vig, uVignette);

  float gr = fract(sin(dot(gl_FragCoord.xy + fract(uTime * 13.7) * 97.0,
                           vec2(12.9898, 78.233))) * 43758.5453) - 0.5;
  col += gr * uGrain * (1.0 - 0.5 * col);

  gl_FragColor = vec4(col, 1.0);
}
`;

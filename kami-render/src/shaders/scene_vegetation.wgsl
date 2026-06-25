// GPU-instanced vegetation with wind-driven vertex sway (grass/fern/palm/conifer/bush).
struct U { viewProj: mat4x4f, camPos: vec3f, time: f32, sunDir: vec3f, windSpeed: f32, fogColor: vec3f, fogDensity: f32, windDir: vec2f, gustMul: f32, biomeDry: f32 };
@group(0) @binding(0) var<uniform> u: U;

struct VIn { @location(0) vpos: vec3f, @location(1) vuv: vec2f, @location(2) ipos: vec3f, @location(3) isc: f32, @location(4) irot: f32, @location(5) isp: f32, @location(6) iph: f32, @location(7) itn: f32 };
struct VO { @builtin(position) cp: vec4f, @location(0) wp: vec3f, @location(1) uv: vec2f, @location(2) tn: f32, @location(3) sp: f32, @location(4) hf: f32 };

fn sH(s: f32) -> f32 { if (s < 0.5) { return 0.7; } else if (s < 1.5) { return 1.3; } else if (s < 2.5) { return 7.5; } else if (s < 3.5) { return 9.0; } else { return 1.6; } }
fn sW(s: f32) -> f32 { if (s < 0.5) { return 0.5; } else if (s < 1.5) { return 1.4; } else if (s < 2.5) { return 3.0; } else if (s < 3.5) { return 2.2; } else { return 1.5; } }
fn sSw(s: f32) -> f32 { if (s < 0.5) { return 0.45; } else if (s < 1.5) { return 0.3; } else if (s < 2.5) { return 0.6; } else if (s < 3.5) { return 0.2; } else { return 0.35; } }

// ── Wind field (matches kami_atmosphere::wind_field Rust formula) ──
fn wf_hash(p: vec2f) -> f32 {
  return fract((sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453));
}
fn wf_noise(p: vec2f) -> f32 {
  let i = floor(p);
  let f = fract(p);
  let s = (f * f * (3.0 - (2.0 * f)));
  return mix(mix(wf_hash(i), wf_hash((i + vec2f(1, 0))), s.x), mix(wf_hash((i + vec2f(0, 1))), wf_hash((i + vec2f(1, 1))), s.x), s.y);
}
/// Returns (local_wind_dir_xz, local_gust_mul) at world position xz + time t.
fn wind_field_sample(xz: vec2f, t: f32, base_dir: vec2f, gust: f32) -> vec3f {
  let freq = 0.012;
  // ~83m ripple wavelength;
  let tfreq = 0.25;
  let var_amp = 0.5;
  let n1 = ((wf_noise(vec2f(((xz.x * freq) + (t * tfreq)), ((xz.y * freq) + (t * tfreq * 0.7)))) * 2.0) - 1.0);
  let n2 = ((wf_noise(vec2f(((xz.x * freq * 2.0) + (t * tfreq * 1.5) + 13.0), ((xz.y * freq * 2.0) + (t * tfreq * 1.1) + 7.0))) * 2.0) - 1.0);
  let magnitude_mod = (1.0 + (((n1 * 0.7) + (n2 * 0.3)) * var_amp));
  let dir_shift = (((n1 * 0.3) + (n2 * 0.2)) * var_amp);
  let perp = vec2f((-base_dir.y), base_dir.x);
  let local_dir = normalize((base_dir + (perp * dir_shift)));
  return vec3f(local_dir.x, local_dir.y, (max(magnitude_mod, 0.0) * gust));
}

@vertex
fn vs(v: VIn) -> VO {
  let h = (sH(v.isp) * v.isc);
  let w = (sW(v.isp) * v.isc);
  var lp = vec3f((v.vpos.x * w), (v.vpos.y * h), (v.vpos.z * w));
  let c = cos(v.irot);
  let s = sin(v.irot);
  let rp = vec3f(((c * lp.x) - (s * lp.z)), lp.y, ((s * lp.x) + (c * lp.z)));
  let hf = v.vpos.y;
  let sw = sSw(v.isp);
  // Sample wind field at instance position — gives spatial ripple;
  let wf = wind_field_sample(v.ipos.xz, u.time, u.windDir, u.gustMul);
  let local_dir = wf.xy;
  let local_gust = wf.z;
  let wm = (u.windSpeed * local_gust * 0.1);
  let ph = ((u.time * 2.2) + v.iph + dot(v.ipos.xz, vec2f(0.03)));
  let amt = (hf * hf * sw * wm);
  let bendX = (local_dir.x * amt * sin(ph));
  let bendZ = (local_dir.y * amt * sin((ph + 0.7)));
  let wp = (v.ipos + rp + vec3f(bendX, 0.0, bendZ));
  var o: VO;
  o.cp = (u.viewProj * vec4f(wp, 1.0));
  o.wp = wp;
  o.uv = v.vuv;
  o.tn = v.itn;
  o.sp = v.isp;
  o.hf = hf;
  return o;
}

fn hash21(p: vec2f) -> f32 {
  return fract((sin(dot(p, vec2f(17.3, 91.7))) * 43758.5453));
}

@fragment
fn fs(v: VO) -> @location(0) vec4f {
  let isGr = (v.sp < 1.5);
  var a = 1.0;
  if (isGr) {
    // Blade edge fade + center vein cutout for grass;
    let edge = (1.0 - (abs((v.uv.x - 0.5)) * 2.0));
    a = smoothstep(0.0, 0.3, edge);
    if ((a < 0.35)) {
      discard;
    }
  }
  // Base palette (green vs dry tan);
  let gb = vec3f(0.18, 0.42, 0.08);
  let gt = vec3f(0.42, 0.68, 0.15);
  let db = vec3f(0.55, 0.48, 0.26);
  let dt = vec3f(0.78, 0.68, 0.38);
  let b = mix(gb, db, u.biomeDry);
  let t = mix(gt, dt, u.biomeDry);
  let isT = ((v.sp > 1.5) && (v.sp < 3.5));
  var col = mix(b, t, v.hf);
  if (isT) {
    // Tree: brown trunk → green foliage at heightFrac 0.5-0.7;
    let tr = vec3f(0.32, 0.22, 0.14);
    let fo = mix(vec3f(0.2, 0.35, 0.1), vec3f(0.42, 0.40, 0.22), u.biomeDry);
    col = mix(tr, fo, smoothstep(0.5, 0.7, v.hf));
    // Leaf-like speckle on foliage portion;
    if ((v.hf > 0.55)) {
      let speckle = hash21(floor((v.uv * 12.0)));
      col *= 0.85 + speckle * 0.3;
    }
  } else {
    // Grass/fern/bush: vertical vein darker, horizontal noise;
    let vein = (smoothstep(0.42, 0.5, abs((v.uv.x - 0.5))) * 0.25);
    col *= 1.0 - vein;
    // Horizontal noise bands for leaf texture;
    let band = ((sin((v.uv.y * 14.0)) * 0.06) + (hash21(floor((v.uv * 8.0))) * 0.15));
    col += vec3f(band * 0.3, band * 0.25, band * 0.1);
  }
  // Per-instance color tint (±15% variation);
  col += vec3f((v.tn * 0.4));
  // Back-lit subsurface: when viewer faces away from sun, leaves glow slightly;
  let sun_back = max((-u.sunDir.z), 0.0);
  let translucent = (smoothstep(0.3, 1.0, v.hf) * sun_back * 0.15);
  col += vec3f((translucent * 0.8), (translucent * 1.0), (translucent * 0.4));
  // Stage 4: ground AO — darken the base of the plant (simulates;
  // occlusion from surrounding terrain + foliage density at root level).;
  let ao = (1.0 - exp(((-v.hf) * 3.0)));
  // 0 at base, ~1 at top;
  col *= mix(0.55, 1.0, ao);
  // Ambient + directional light;
  col *= 0.7 + max(u.sunDir.y, 0.0) * 0.3;
  // Fog;
  let d = length((v.wp - u.camPos));
  let f = (1.0 - exp(((-d) * u.fogDensity * 2.0)));
  col = mix(col, u.fogColor, clamp(f, 0.0, 0.92));
  return vec4f(col, a);
}

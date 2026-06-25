struct U { invVP: mat4x4f, camPos: vec3f, _p0: f32, sunDir: vec3f, _p1: f32, fogColor: vec3f, overcast: f32, scrollX: f32, scrollZ: f32, altitude: f32, _p2: f32 };
@group(0) @binding(0) var<uniform> u: U;
struct VO { @builtin(position) pos: vec4f, @location(0) ndc: vec2f };
@vertex
fn vs(@builtin(vertex_index) vi: u32) -> VO {
  let x = (f32(((vi & 1u) << 2u)) - 1.0);
  let y = (f32(((vi & 2u) << 1u)) - 1.0);
  var o: VO;
  o.pos = vec4f(x, y, 1.0, 1.0);
  o.ndc = vec2f(x, y);
  return o;
}
fn h2(p: vec2f) -> f32 {
  return fract((sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453));
}
fn n2(p: vec2f) -> f32 {
  let i = floor(p);
  let f = fract(p);
  let u = (f * f * (3.0 - (2.0 * f)));
  return mix(mix(h2(i), h2((i + vec2f(1, 0))), u.x), mix(h2((i + vec2f(0, 1))), h2((i + vec2f(1, 1))), u.x), u.y);
}
fn fbm(p: vec2f) -> f32 {
  var v = 0.0;
  var a = 0.5;
  var f = 1.0;
  for (var i = 0; (i < 5); i = (i + 1)) {
    v += (a * n2((p * f)));
    a *= 0.5;
    f *= 2.0;
  }
  return v;
}
@fragment
fn fs(v: VO) -> @location(0) vec4f {
  let wp4 = (u.invVP * vec4f(v.ndc.x, v.ndc.y, 1.0, 1.0));
  let vd = normalize(((wp4.xyz / wp4.w) - u.camPos));
  let elev = max(vd.y, 0.0);
  let horizon = vec3f(0.72, 0.73, 0.75);
  let zenith = vec3f(0.60, 0.63, 0.67);
  var sky = mix(horizon, zenith, pow(elev, 0.7));
  if ((vd.y > 0.005)) {
    let t = ((u.altitude - u.camPos.y) / vd.y);
    if (((t > 0.0) && (t < 6000.0))) {
      let hit = (u.camPos + (vd * t));
      let cuv = ((hit.xz * 0.0015) + (vec2f(u.scrollX, u.scrollZ) * 0.001));
      let cn = ((fbm(cuv) * 0.7) + (fbm(((cuv * 2.3) + vec2f(3.1))) * 0.3));
      let thr = (1.0 - u.overcast);
      let mask = smoothstep((thr - 0.05), (thr + 0.2), cn);
      let ccol = mix(vec3f(0.40, 0.42, 0.46), vec3f(0.82, 0.82, 0.83), cn);
      let fade = smoothstep(5500.0, 400.0, t);
      sky = mix(sky, ccol, (mask * fade));
    }
  }
  return vec4f(sky, 1.0);
}

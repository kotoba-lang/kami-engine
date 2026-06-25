struct U { viewProj: mat4x4f, camPos: vec3f, _p0: f32, sunDir: vec3f, _p1: f32, sunColor: vec3f, fogDensity: f32, fogColor: vec3f, _p2: f32, baseCol: array<vec4f, 4>, tipCol: array<vec4f, 4> };
@group(0) @binding(0) var<uniform> u: U;
struct VIn { @location(0) pos: vec3f, @location(1) normal: vec3f, @location(2) uv: vec2f, @location(3) splat: vec4f };
struct VOut { @builtin(position) clipPos: vec4f, @location(0) wp: vec3f, @location(1) n: vec3f, @location(2) sp: vec4f };
@vertex
fn vs(v: VIn) -> VOut {
  var o: VOut;
  o.clipPos = (u.viewProj * vec4f(v.pos, 1.0));
  o.wp = v.pos;
  o.n = normalize(v.normal);
  o.sp = v.splat;
  return o;
}
@fragment
fn fs(v: VOut) -> @location(0) vec4f {
  let slope = (1.0 - v.n.y);
  var col = vec3f(0.0);
  for (var i = 0; (i < 4); i = (i + 1)) {
    let r = clamp(((slope * 0.6) + 0.2), 0.0, 1.0);
    col += (mix((u.baseCol[i]).xyz, (u.tipCol[i]).xyz, r) * v.sp[i]);
  }
  let NdotL = max(dot(v.n, u.sunDir), 0.0);
  let ambient = vec3f(0.40, 0.41, 0.44);
  col = (col * (ambient + (u.sunColor * NdotL * 0.45)));
  let d = length((v.wp - u.camPos));
  let fog = (1.0 - exp(((-d) * u.fogDensity * 2.0)));
  col = mix(col, u.fogColor, clamp(fog, 0.0, 0.92));
  return vec4f(col, 1.0);
}

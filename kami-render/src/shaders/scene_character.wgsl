struct U { viewProj: mat4x4f, model: mat4x4f, camPos: vec3f, _p0: f32, sunDir: vec3f, _p1: f32, sunColor: vec3f, fogDensity: f32, fogColor: vec3f, _p2: f32 };
@group(0) @binding(0) var<uniform> u: U;
struct VIn { @location(0) pos: vec3f, @location(1) normal: vec3f, @location(2) color: vec3f };
struct VO { @builtin(position) cp: vec4f, @location(0) wp: vec3f, @location(1) n: vec3f, @location(2) col: vec3f };
@vertex
fn vs(v: VIn) -> VO {
  var o: VO;
  let wp = ((u.model * vec4f(v.pos, 1.0))).xyz;
  o.cp = (u.viewProj * vec4f(wp, 1.0));
  o.wp = wp;
  o.n = normalize(((u.model * vec4f(v.normal, 0.0))).xyz);
  o.col = v.color;
  return o;
}
@fragment
fn fs(v: VO) -> @location(0) vec4f {
  let NdotL = max(dot(v.n, u.sunDir), 0.0);
  let ambient = vec3f(0.45, 0.46, 0.50);
  var col = (v.col * (ambient + (u.sunColor * NdotL * 0.5)));
  let d = length((v.wp - u.camPos));
  let f = (1.0 - exp(((-d) * u.fogDensity * 2.0)));
  col = mix(col, u.fogColor, clamp(f, 0.0, 0.9));
  return vec4f(col, 1.0);
}

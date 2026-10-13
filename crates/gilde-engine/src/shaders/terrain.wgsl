struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    ambient: vec4<f32>,
    fog: vec4<f32>,
    camera_pos: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var layers: texture_2d_array<f32>;
@group(1) @binding(1) var samp: sampler;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) w0: vec4<f32>,
    @location(3) w1: vec4<f32>,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) w0: vec4<f32>,
    @location(2) w1: vec4<f32>,
    @location(3) world: vec3<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.normal = v.normal;
    o.w0 = v.w0;
    o.w1 = v.w1;
    o.world = v.pos;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    let uv = i.world.xz / 400.0;
    var c = vec3<f32>(0.0);
    c += textureSample(layers, samp, uv, 0).rgb * i.w0.x;
    c += textureSample(layers, samp, uv, 1).rgb * i.w0.y;
    c += textureSample(layers, samp, uv, 2).rgb * i.w0.z;
    c += textureSample(layers, samp, uv, 3).rgb * i.w0.w;
    c += textureSample(layers, samp, uv, 4).rgb * i.w1.x;
    c += textureSample(layers, samp, uv, 5).rgb * i.w1.y;
    c += textureSample(layers, samp, uv, 6).rgb * i.w1.z;
    c += textureSample(layers, samp, uv, 7).rgb * i.w1.w;
    let wsum = dot(i.w0, vec4<f32>(1.0)) + dot(i.w1, vec4<f32>(1.0));
    c = c / max(wsum, 0.0001);
    let n = normalize(i.normal);
    let diff = max(dot(n, -g.light_dir.xyz), 0.0);
    let lit = c * (g.ambient.rgb + diff * vec3<f32>(0.85, 0.82, 0.75));
    let d = clamp(distance(i.world, g.camera_pos.xyz) * g.fog.w, 0.0, 1.0);
    return vec4<f32>(mix(lit, g.fog.rgb, d * d), 1.0);
}

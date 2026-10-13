struct Globals {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    ambient: vec4<f32>,
    fog: vec4<f32>,       // rgb colour
    camera_pos: vec4<f32>,
    sun: vec4<f32>,       // rgb sun colour
    fog_range: vec4<f32>, // near, far
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) m0: vec4<f32>,
    @location(4) m1: vec4<f32>,
    @location(5) m2: vec4<f32>,
    @location(6) m3: vec4<f32>,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    let world = model * vec4<f32>(v.pos, 1.0);
    var o: VsOut;
    o.clip = g.view_proj * world;
    o.normal = normalize((model * vec4<f32>(v.normal, 0.0)).xyz);
    o.uv = v.uv;
    o.world = world.xyz;
    return o;
}

fn shade(base: vec3<f32>, n: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let diff = abs(dot(n, -g.light_dir.xyz));
    let lit = base * (g.ambient.rgb + diff * g.sun.rgb);
    let d = clamp((distance(world, g.camera_pos.xyz) - g.fog_range.x) / max(g.fog_range.y - g.fog_range.x, 1.0), 0.0, 1.0);
    return mix(lit, g.fog.rgb, d);
}

@fragment
fn fs_opaque(i: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv);
    return vec4<f32>(shade(c.rgb, normalize(i.normal), i.world), 1.0);
}

@fragment
fn fs_cutout(i: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, i.uv);
    if (c.a < 0.5) { discard; }
    return vec4<f32>(shade(c.rgb, normalize(i.normal), i.world), 1.0);
}

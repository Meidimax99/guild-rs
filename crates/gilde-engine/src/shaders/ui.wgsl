struct Xf { s: vec4<f32> }; // scale.xy, offset.xy (virtual px -> NDC)
@group(0) @binding(0) var<uniform> xf: Xf;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VsIn { @location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32> };
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec4<f32> };

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = vec4<f32>(v.pos.x * xf.s.x + xf.s.z, v.pos.y * xf.s.y + xf.s.w, 0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    return o;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    // D3D modulate: the engine draws normal UI at 200/255, hover at 255/255.
    return vec4<f32>(t.rgb * i.color.rgb, t.a * i.color.a);
}

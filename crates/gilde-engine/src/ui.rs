//! Rasteriser for `gilde_ui::DrawList` (2D sprites in 800×600 UI space).

use gilde_ui::{DrawCmd, DrawList, SpriteRef, UiAssets, UI_HEIGHT, UI_WIDTH};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct V {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [u8; 4],
}

pub struct UiRenderer {
    pipeline: wgpu::RenderPipeline,
    xf: wgpu::Buffer,
    xf_bg: wgpu::BindGroup,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::BindGroup,
    textures: HashMap<SpriteRef, Option<wgpu::BindGroup>>,
}

/// Uniform scale + centring of the 800×600 UI inside the framebuffer.
pub fn ui_viewport(width: u32, height: u32) -> (f32, f32, f32) {
    let s = (width as f32 / UI_WIDTH).min(height as f32 / UI_HEIGHT);
    let ox = (width as f32 - UI_WIDTH * s) / 2.0;
    let oy = (height as f32 - UI_HEIGHT * s) / 2.0;
    (s, ox, oy)
}

/// Convert framebuffer pixel coordinates to UI coordinates.
pub fn to_ui(width: u32, height: u32, px: f32, py: f32) -> (f32, f32) {
    let (s, ox, oy) = ui_viewport(width, height);
    ((px - ox) / s, (py - oy) / s)
}

impl UiRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/ui.wgsl"));
        let xf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui xf"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let xf_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let xf_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &xf_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: xf.as_entire_binding() }],
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui tex"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui"),
            bind_group_layouts: &[&xf_layout, &layout],
            push_constant_ranges: &[],
        });
        const ATTRS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<V>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &ATTRS,
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white = Self::make_bg(device, &layout, &sampler, None, 1, 1, &[255, 255, 255, 255]);
        Self { pipeline, xf, xf_bg, layout, sampler, white, textures: HashMap::new() }
    }

    fn make_bg(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        queue: Option<&wgpu::Queue>,
        w: u32,
        h: u32,
        rgba: &[u8],
    ) -> wgpu::BindGroup {
        let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        let desc = wgpu::TextureDescriptor {
            label: Some("ui sprite"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        let tex = match queue {
            Some(q) => device.create_texture_with_data(q, &desc, wgpu::util::TextureDataOrder::LayerMajor, rgba),
            None => {
                // 1×1 white: create with mapped data via a tiny staging upload.
                let t = device.create_texture(&desc);
                let _ = rgba;
                t
            }
        };
        let view = tex.create_view(&Default::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        })
    }

    fn texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, assets: &UiAssets, s: &SpriteRef) -> Option<&wgpu::BindGroup> {
        if !self.textures.contains_key(s) {
            let bg = assets.bank(&s.bank).and_then(|b| {
                let f = b.frames.get(s.frame as usize)?;
                (f.width > 0 && f.height > 0)
                    .then(|| Self::make_bg(device, &self.layout, &self.sampler, Some(queue), f.width, f.height, &f.rgba))
            });
            self.textures.insert(s.clone(), bg);
        }
        self.textures.get(s).and_then(|o| o.as_ref())
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        assets: &UiAssets,
        list: &DrawList,
    ) {
        if self.textures.is_empty() {
            // Initialise the white texture lazily (needs the queue).
            self.white = Self::make_bg(device, &self.layout, &self.sampler, Some(queue), 1, 1, &[255, 255, 255, 255]);
        }
        let (s, ox, oy) = ui_viewport(width, height);
        let sx = 2.0 * s / width as f32;
        let sy = -2.0 * s / height as f32;
        let xf = [sx, sy, -1.0 + 2.0 * ox / width as f32, 1.0 - 2.0 * oy / height as f32];
        queue.write_buffer(&self.xf, 0, bytemuck::cast_slice(&xf));

        // Build batches: (texture key or None for white, scissor, vertex range).
        let mut verts: Vec<V> = Vec::new();
        let mut batches: Vec<(Option<SpriteRef>, [u32; 4], u32, u32)> = Vec::new();
        let full = [0, 0, width, height];
        let mut scissor = full;
        let to_px = |r: [f32; 4]| -> [u32; 4] {
            let x0 = (r[0] * s + ox).floor().clamp(0.0, width as f32);
            let y0 = (r[1] * s + oy).floor().clamp(0.0, height as f32);
            let x1 = ((r[0] + r[2]) * s + ox).ceil().clamp(0.0, width as f32);
            let y1 = ((r[1] + r[3]) * s + oy).ceil().clamp(0.0, height as f32);
            [x0 as u32, y0 as u32, (x1 - x0).max(0.0) as u32, (y1 - y0).max(0.0) as u32]
        };
        let mut push = |key: Option<SpriteRef>, sc: [u32; 4], x: f32, y: f32, w: f32, h: f32, c: [u8; 4], verts: &mut Vec<V>| {
            let start = verts.len() as u32;
            let q = [([x, y], [0.0, 0.0]), ([x + w, y], [1.0, 0.0]), ([x + w, y + h], [1.0, 1.0]), ([x, y + h], [0.0, 1.0])];
            for i in [0, 1, 2, 0, 2, 3] {
                verts.push(V { pos: q[i].0, uv: q[i].1, color: c });
            }
            match batches.last_mut() {
                Some(b) if b.0 == key && b.1 == sc => b.3 += 6,
                _ => batches.push((key, sc, start, 6)),
            }
        };
        for cmd in &list.cmds {
            match cmd {
                DrawCmd::Clip(r) => scissor = r.map(to_px).unwrap_or(full),
                DrawCmd::Rect { x, y, w, h, rgba } => push(None, scissor, *x, *y, *w, *h, *rgba, &mut verts),
                DrawCmd::Sprite { sprite, x, y, w, h, tint } => {
                    if self.texture(device, queue, assets, sprite).is_some() {
                        push(Some(sprite.clone()), scissor, *x, *y, *w, *h, *tint, &mut verts);
                    }
                }
            }
        }
        if verts.is_empty() {
            return;
        }
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ui verts"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.xf_bg, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            for (key, sc, start, count) in &batches {
                if sc[2] == 0 || sc[3] == 0 {
                    continue;
                }
                pass.set_scissor_rect(sc[0], sc[1], sc[2], sc[3]);
                let bg = match key {
                    Some(k) => self.textures.get(k).and_then(|o| o.as_ref()).unwrap_or(&self.white),
                    None => &self.white,
                };
                pass.set_bind_group(1, bg, &[]);
                pass.draw(*start..start + count, 0..1);
            }
        }
        queue.submit([enc.finish()]);
    }
}

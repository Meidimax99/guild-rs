//! wgpu renderer for worlds assembled by [`crate::world::World`].

use crate::assets::{Assets, MeshData, TextureData, Vertex};
use crate::camera::Camera;
use crate::terrain::TerrainVertex;
use crate::world::World;
use anyhow::{Context, Result};
use glam::{Mat4, Vec3, Vec4};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const TERRAIN_LAYER_SIZE: u32 = 256;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
    ambient: [f32; 4],
    fog: [f32; 4],
    camera_pos: [f32; 4],
}

struct GpuSubMesh {
    bind_group: Arc<wgpu::BindGroup>,
    cutout: bool,
    start: u32,
    count: u32,
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    submeshes: Vec<GpuSubMesh>,
    instances: wgpu::Buffer,
    instance_count: u32,
}

struct GpuTerrain {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    bind_group: wgpu::BindGroup,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub color_format: wgpu::TextureFormat,
    globals: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    mesh_opaque: wgpu::RenderPipeline,
    mesh_cutout: wgpu::RenderPipeline,
    terrain_pipeline: wgpu::RenderPipeline,
    textures: HashMap<(String, bool), Arc<wgpu::BindGroup>>,
    meshes: Vec<GpuMesh>,
    terrain: Option<GpuTerrain>,
    depth: Option<(wgpu::TextureView, u32, u32)>,
    pub clear_color: wgpu::Color,
    pub ambient: Vec3,
}

pub async fn create_device(
    compatible_surface: Option<&wgpu::Surface<'_>>,
    instance: &wgpu::Instance,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface,
            force_fallback_adapter: false,
        })
        .await
        .context("no suitable GPU adapter")?;
    log::info!("adapter: {:?}", adapter.get_info().name);
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("gilde"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        )
        .await?;
    Ok((adapter, device, queue))
}

fn vertex_layouts() -> [wgpu::VertexBufferLayout<'static>; 2] {
    const MESH_ATTRS: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
    const INST_ATTRS: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4];
    [
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &MESH_ATTRS,
        },
        wgpu::VertexBufferLayout {
            array_stride: 64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INST_ATTRS,
        },
    ]
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, color_format: wgpu::TextureFormat) -> Self {
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        let tex_entry = |dim| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: dim,
                multisampled: false,
            },
            count: None,
        };
        let samp_entry = wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture"),
            entries: &[tex_entry(wgpu::TextureViewDimension::D2), samp_entry],
        });
        let terrain_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain"),
            entries: &[tex_entry(wgpu::TextureViewDimension::D2Array), samp_entry],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let mesh_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/mesh.wgsl"));
        let terrain_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/terrain.wgsl"));
        let mesh_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[&globals_layout, &tex_layout],
            push_constant_ranges: &[],
        });
        let depth_state = || wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        };
        let layouts = vertex_layouts();
        let mesh_pipeline = |fs: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&mesh_layout),
                vertex: wgpu::VertexState {
                    module: &mesh_shader,
                    entry_point: Some("vs_main"),
                    buffers: &layouts,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &mesh_shader,
                    entry_point: Some(fs),
                    targets: &[Some(color_format.into())],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: Some(depth_state()),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let mesh_opaque = mesh_pipeline("fs_opaque");
        let mesh_cutout = mesh_pipeline("fs_cutout");

        let terrain_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain"),
            bind_group_layouts: &[&globals_layout, &terrain_layout],
            push_constant_ranges: &[],
        });
        const TERRAIN_ATTRS: [wgpu::VertexAttribute; 4] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x4];
        let terrain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain"),
            layout: Some(&terrain_pl),
            vertex: wgpu::VertexState {
                module: &terrain_shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TerrainVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &TERRAIN_ATTRS,
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &terrain_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(color_format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: Some(depth_state()),
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        // Keep the terrain layout alive through the bind group created later.
        let _ = &terrain_layout;

        Self {
            device,
            queue,
            color_format,
            globals,
            globals_bg,
            tex_layout,
            sampler,
            mesh_opaque,
            mesh_cutout,
            terrain_pipeline,
            textures: HashMap::new(),
            meshes: Vec::new(),
            terrain: None,
            depth: None,
            clear_color: wgpu::Color { r: 0.55, g: 0.68, b: 0.85, a: 1.0 },
            ambient: Vec3::splat(0.45),
        }
    }

    fn upload_texture(&self, data: &TextureData, layers: u32) -> wgpu::Texture {
        let mip_count = data.width.min(data.height).max(1).ilog2() + 1;
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: data.width, height: data.height / layers, depth_or_array_layers: layers },
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // CPU-generated mip chain (box filter, alpha-preserving for cutouts).
        let (mut w, mut h) = (data.width, data.height / layers);
        let mut levels: Vec<Vec<u8>> = vec![data.rgba.clone()];
        for _ in 1..mip_count {
            let prev = levels.last().unwrap();
            let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
            let mut next = vec![0u8; (nw * nh * 4 * layers) as usize];
            for l in 0..layers {
                for y in 0..nh {
                    for x in 0..nw {
                        for c in 0..4 {
                            let mut s = 0u32;
                            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                                let sx = (x * 2 + dx).min(w - 1);
                                let sy = (y * 2 + dy).min(h - 1);
                                s += prev[(((l * h + sy) * w + sx) * 4 + c) as usize] as u32;
                            }
                            next[(((l * nh + y) * nw + x) * 4 + c) as usize] = (s / 4) as u8;
                        }
                    }
                }
            }
            levels.push(next);
            (w, h) = (nw, nh);
        }
        let (mut w, mut h) = (data.width, data.height / layers);
        for (mip, level) in levels.iter().enumerate() {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: mip as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                level,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: layers },
            );
            (w, h) = ((w / 2).max(1), (h / 2).max(1));
        }
        tex
    }

    fn texture_bind_group(&mut self, assets: &mut Assets, name: &str, color_key: bool) -> Arc<wgpu::BindGroup> {
        let key = (name.to_lowercase(), color_key);
        if let Some(bg) = self.textures.get(&key) {
            return bg.clone();
        }
        let data = if name.is_empty() {
            TextureData::solid([200, 200, 200, 255])
        } else {
            match assets.texture(name) {
                Some(t) if color_key => (*t).clone().with_color_key(),
                Some(t) => (*t).clone(),
                // Missing in the shipped data (e.g. vg_nm_big_laub_a_s03): hide cutouts.
                None if color_key => TextureData::solid([0, 0, 0, 0]),
                None => TextureData::solid([160, 160, 160, 255]),
            }
        };
        let tex = self.upload_texture(&data, 1);
        let view = tex.create_view(&Default::default());
        let bg = Arc::new(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(name),
            layout: &self.tex_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        }));
        self.textures.insert(key, bg.clone());
        bg
    }

    /// Upload everything needed to draw `world`, replacing the previous world.
    pub fn load_world(&mut self, world: &World, assets: &mut Assets) {
        self.meshes.clear();
        self.terrain = None;
        self.ambient = (world.ambient / 100.0).clamp(Vec3::splat(0.25), Vec3::splat(0.8));

        let mut groups: Vec<(Arc<MeshData>, Vec<Mat4>)> = Vec::new();
        let mut index: HashMap<*const MeshData, usize> = HashMap::new();
        for inst in &world.instances {
            let k = Arc::as_ptr(&inst.mesh);
            let i = *index.entry(k).or_insert_with(|| {
                groups.push((inst.mesh.clone(), Vec::new()));
                groups.len() - 1
            });
            groups[i].1.push(inst.transform);
        }
        for (mesh, transforms) in groups {
            if mesh.vertices.is_empty() {
                continue;
            }
            let vertices = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&mesh.name),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let inst: Vec<[f32; 16]> = transforms.iter().map(|m| m.to_cols_array()).collect();
            let instances = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("instances"),
                contents: bytemuck::cast_slice(&inst),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let submeshes = mesh
                .submeshes
                .iter()
                .map(|s| GpuSubMesh {
                    bind_group: self.texture_bind_group(assets, &s.material.texture, s.material.color_key),
                    cutout: s.material.color_key,
                    start: s.start,
                    count: s.count,
                })
                .collect();
            self.meshes.push(GpuMesh { vertices, submeshes, instances, instance_count: inst.len() as u32 });
        }

        if let Some(t) = &world.terrain {
            let (verts, idx) = t.build_mesh();
            let mut layer_data = Vec::new();
            for l in 0..8 {
                let name = t.layer_textures.get(l).cloned().unwrap_or_default();
                let tex = if name.is_empty() { None } else { assets.texture(&name) };
                let tex = tex.map(|t| (*t).clone()).unwrap_or_else(|| TextureData::solid([90, 120, 60, 255]));
                layer_data.extend(tex.resized(TERRAIN_LAYER_SIZE, TERRAIN_LAYER_SIZE).rgba);
            }
            let arr = self.upload_texture(
                &TextureData { width: TERRAIN_LAYER_SIZE, height: TERRAIN_LAYER_SIZE * 8, rgba: layer_data },
                8,
            );
            let view = arr.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
            let layout = self.terrain_pipeline.get_bind_group_layout(1);
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("terrain"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            });
            self.terrain = Some(GpuTerrain {
                vertices: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("terrain"),
                    contents: bytemuck::cast_slice(&verts),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                indices: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("terrain idx"),
                    contents: bytemuck::cast_slice(&idx),
                    usage: wgpu::BufferUsages::INDEX,
                }),
                index_count: idx.len() as u32,
                bind_group,
            });
        }
    }

    fn depth_view(&mut self, w: u32, h: u32) -> wgpu::TextureView {
        if let Some((v, dw, dh)) = &self.depth {
            if (*dw, *dh) == (w, h) {
                return v.clone();
            }
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let v = tex.create_view(&Default::default());
        self.depth = Some((v.clone(), w, h));
        v
    }

    pub fn render(&mut self, target: &wgpu::TextureView, width: u32, height: u32, camera: &Camera) {
        let c = self.clear_color;
        let globals = Globals {
            view_proj: camera.view_proj(width as f32 / height as f32).to_cols_array_2d(),
            light_dir: Vec3::new(0.4, -0.8, 0.45).normalize().extend(0.0).to_array(),
            ambient: self.ambient.extend(1.0).to_array(),
            fog: Vec4::new(c.r as f32, c.g as f32, c.b as f32, 1.0 / camera.far).to_array(),
            camera_pos: camera.position.extend(1.0).to_array(),
        };
        self.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));
        let depth = self.depth_view(width, height);
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(c), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            if let Some(t) = &self.terrain {
                pass.set_pipeline(&self.terrain_pipeline);
                pass.set_bind_group(1, &t.bind_group, &[]);
                pass.set_vertex_buffer(0, t.vertices.slice(..));
                pass.set_index_buffer(t.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..t.index_count, 0, 0..1);
            }
            for cutout in [false, true] {
                pass.set_pipeline(if cutout { &self.mesh_cutout } else { &self.mesh_opaque });
                for m in &self.meshes {
                    pass.set_vertex_buffer(0, m.vertices.slice(..));
                    pass.set_vertex_buffer(1, m.instances.slice(..));
                    for s in m.submeshes.iter().filter(|s| s.cutout == cutout) {
                        pass.set_bind_group(1, &*s.bind_group, &[]);
                        pass.draw(s.start..s.start + s.count, 0..m.instance_count);
                    }
                }
            }
        }
        self.queue.submit([enc.finish()]);
    }

    /// Render to an offscreen texture and return tightly packed RGBA8 pixels.
    pub fn render_to_rgba(&mut self, width: u32, height: u32, camera: &Camera) -> Vec<u8> {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.color_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        self.render(&view, width, height, camera);
        let row = (width * 4).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(height) },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::Maintain::Wait);
        let data = buf.slice(..).get_mapped_range();
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            out.extend_from_slice(&data[(y * row) as usize..(y * row + width * 4) as usize]);
        }
        out
    }
}

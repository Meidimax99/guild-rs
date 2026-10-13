//! Interactive free-camera viewer.
//!
//! WASD/QE move, right mouse drag looks around, shift = fast, scroll = speed.

use crate::{Args, load};
use anyhow::Result;
use gilde_engine::Renderer;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
}

struct App {
    args: Args,
    loaded: Option<crate::Loaded>,
    gfx: Option<Gfx>,
    keys: HashSet<KeyCode>,
    looking: bool,
    speed: f32,
    last: Instant,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let window = Arc::new(
            el.create_window(
                Window::default_attributes()
                    .with_title("Die Gilde (gilde-rs)")
                    .with_inner_size(winit::dpi::PhysicalSize::new(self.args.width, self.args.height)),
            )
            .expect("window"),
        );
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone()).expect("surface");
        let (adapter, device, queue) =
            pollster::block_on(gilde_engine::render::create_device(Some(&surface), &instance)).expect("device");
        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| f.is_srgb()).unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let mut renderer = Renderer::new(device, queue, format);
        let l = self.loaded.as_mut().unwrap();
        renderer.load_world(&l.world, &mut l.assets);
        self.gfx = Some(Gfx { window, surface, config, renderer });
        self.last = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gfx) = self.gfx.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                gfx.config.width = s.width.max(1);
                gfx.config.height = s.height.max(1);
                gfx.surface.configure(&gfx.renderer.device, &gfx.config);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(k) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        if k == KeyCode::Escape {
                            el.exit();
                        }
                        self.keys.insert(k);
                    } else {
                        self.keys.remove(&k);
                    }
                }
            }
            WindowEvent::MouseInput { button: MouseButton::Right, state, .. } => {
                self.looking = state == ElementState::Pressed;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 50.0,
                };
                self.speed = (self.speed * 1.2f32.powf(d)).clamp(50.0, 50000.0);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last).as_secs_f32().min(0.1);
                self.last = now;
                let cam = &mut self.loaded.as_mut().unwrap().camera;
                let fast = if self.keys.contains(&KeyCode::ShiftLeft) { 4.0 } else { 1.0 };
                let step = self.speed * fast * dt;
                let (f, r) = (cam.forward(), cam.right());
                for (k, d) in [
                    (KeyCode::KeyW, f),
                    (KeyCode::KeyS, -f),
                    (KeyCode::KeyD, r),
                    (KeyCode::KeyA, -r),
                    (KeyCode::KeyE, glam::Vec3::Y),
                    (KeyCode::KeyQ, -glam::Vec3::Y),
                ] {
                    if self.keys.contains(&k) {
                        cam.position += d * step;
                    }
                }
                let cam = *cam;
                match gfx.surface.get_current_texture() {
                    Ok(frame) => {
                        let view = frame.texture.create_view(&Default::default());
                        gfx.renderer.render(&view, gfx.config.width, gfx.config.height, &cam);
                        frame.present();
                    }
                    Err(_) => gfx.surface.configure(&gfx.renderer.device, &gfx.config),
                }
                gfx.window.request_redraw();
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: winit::event::DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if self.looking {
                let cam = &mut self.loaded.as_mut().unwrap().camera;
                cam.yaw += delta.0 as f32 * 0.004;
                cam.pitch = (cam.pitch + delta.1 as f32 * 0.004).clamp(-1.5, 1.5);
            }
        }
    }
}

pub fn run(args: Args) -> Result<()> {
    let loaded = load(&args)?;
    let el = EventLoop::new()?;
    let mut app = App { args, loaded: Some(loaded), gfx: None, keys: HashSet::new(), looking: false, speed: 1500.0, last: Instant::now() };
    el.run_app(&mut app)?;
    Ok(())
}

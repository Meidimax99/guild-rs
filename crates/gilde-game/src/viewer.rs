//! Windowed application: main menu → city view.
//!
//! City view controls: WASD/QE move, right mouse drag looks around, shift =
//! fast, scroll = speed, Escape returns to the menu.

use crate::menu::{MainMenu, MenuAction};
use crate::{Args, Loaded, load};
use anyhow::Result;
use gilde_engine::Renderer;
use gilde_engine::ui::{UiRenderer, to_ui};
use gilde_formats::vfs::Vfs;
use gilde_ui::{DrawList, Event, UiAssets};
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
    ui: UiRenderer,
}

enum Mode {
    Menu(MainMenu),
    City(Loaded),
}

struct App {
    args: Args,
    vfs: Arc<Vfs>,
    assets: UiAssets,
    mode: Mode,
    pending_city: bool,
    gfx: Option<Gfx>,
    keys: HashSet<KeyCode>,
    looking: bool,
    mouse: (f32, f32),
    speed: f32,
    last: Instant,
}

impl App {
    fn enter_city(&mut self) {
        match load(&self.args) {
            Ok(mut l) => {
                if let Some(g) = self.gfx.as_mut() {
                    g.renderer.load_world(&l.world, &mut l.assets);
                }
                self.mode = Mode::City(l);
            }
            Err(e) => log::error!("loading city: {e:#}"),
        }
    }

    fn back_to_menu(&mut self) {
        match MainMenu::new(&self.vfs, &self.assets) {
            Ok(m) => self.mode = Mode::Menu(m),
            Err(e) => log::error!("menu: {e:#}"),
        }
    }
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
        let ui = UiRenderer::new(&device, format);
        let renderer = Renderer::new(device, queue, format);
        self.gfx = Some(Gfx { window, surface, config, renderer, ui });
        if self.pending_city {
            self.pending_city = false;
            self.enter_city();
        }
        self.last = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if self.gfx.is_none() {
            return;
        }
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(s) => {
                let gfx = self.gfx.as_mut().unwrap();
                gfx.config.width = s.width.max(1);
                gfx.config.height = s.height.max(1);
                gfx.surface.configure(&gfx.renderer.device, &gfx.config);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(k) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        if k == KeyCode::Escape {
                            match self.mode {
                                Mode::City(_) => self.back_to_menu(),
                                Mode::Menu(_) => el.exit(),
                            }
                        }
                        self.keys.insert(k);
                    } else {
                        self.keys.remove(&k);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let g = self.gfx.as_ref().unwrap();
                self.mouse = to_ui(g.config.width, g.config.height, position.x as f32, position.y as f32);
                if let Mode::Menu(m) = &mut self.mode {
                    m.screen.mouse(&self.assets, self.mouse.0, self.mouse.1, false, false);
                }
            }
            WindowEvent::MouseInput { button, state, .. } => match &mut self.mode {
                Mode::Menu(m) if button == MouseButton::Left => {
                    let ev = m.screen.mouse(
                        &self.assets,
                        self.mouse.0,
                        self.mouse.1,
                        state == ElementState::Pressed,
                        state == ElementState::Released,
                    );
                    if let Some(Event::Clicked(id)) = ev {
                        match m.action(id) {
                            Some(MenuAction::NewGame) => self.enter_city(),
                            Some(MenuAction::Quit) => el.exit(),
                            Some(a) => log::info!("menu action {a:?} not implemented yet"),
                            None => {}
                        }
                    }
                }
                Mode::City(_) if button == MouseButton::Right => self.looking = state == ElementState::Pressed,
                _ => {}
            },
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
                let gfx = self.gfx.as_mut().unwrap();
                let frame = match gfx.surface.get_current_texture() {
                    Ok(f) => f,
                    Err(_) => {
                        gfx.surface.configure(&gfx.renderer.device, &gfx.config);
                        return;
                    }
                };
                let view = frame.texture.create_view(&Default::default());
                let (w, h) = (gfx.config.width, gfx.config.height);
                match &mut self.mode {
                    Mode::City(l) => {
                        let cam = &mut l.camera;
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
                        // Day advances while viewing (rate is a placeholder until the
                        // engine's clock rate is recovered): 1 game minute per second.
                        l.date.add(0, 0, (dt * 60.0) as i32);
                        if let Some(light) = l.lighting() {
                            gfx.renderer.lighting = light;
                        }
                        gfx.renderer.render(&view, w, h, &cam);
                    }
                    Mode::Menu(m) => {
                        let mut enc = gfx.renderer.device.create_command_encoder(&Default::default());
                        enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("clear"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                resolve_target: None,
                                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                            })],
                            ..Default::default()
                        });
                        gfx.renderer.queue.submit([enc.finish()]);
                        let mut list = DrawList::default();
                        m.screen.draw(&self.assets, &mut list);
                        let (d, q) = (gfx.renderer.device.clone(), gfx.renderer.queue.clone());
                        gfx.ui.render(&d, &q, &view, w, h, &self.assets, &list);
                    }
                }
                frame.present();
                gfx.window.request_redraw();
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: winit::event::DeviceId, event: DeviceEvent) {
        if let (DeviceEvent::MouseMotion { delta }, Mode::City(l)) = (event, &mut self.mode) {
            if self.looking {
                l.camera.yaw += delta.0 as f32 * 0.004;
                l.camera.pitch = (l.camera.pitch + delta.1 as f32 * 0.004).clamp(-1.5, 1.5);
            }
        }
    }
}

pub fn run(args: Args) -> Result<()> {
    let vfs = Arc::new(Vfs::open_game_dir(&args.game)?);
    let assets = crate::open_ui_assets(&args.game, &vfs)?;
    let menu = MainMenu::new(&vfs, &assets)?;
    let pending_city = args.scene.is_some();
    let el = EventLoop::new()?;
    let mut app = App {
        args,
        vfs,
        assets,
        mode: Mode::Menu(menu),
        pending_city,
        gfx: None,
        keys: HashSet::new(),
        looking: false,
        mouse: (0.0, 0.0),
        speed: 1500.0,
        last: Instant::now(),
    };
    el.run_app(&mut app)?;
    Ok(())
}

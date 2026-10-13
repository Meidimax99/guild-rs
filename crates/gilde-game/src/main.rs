//! Die Gilde reimplementation — viewer / game entry point.

use anyhow::{Context, Result};
use clap::Parser;
use gilde_engine::{Assets, Camera, Renderer, World};
use gilde_formats::scene::Scene;
use gilde_formats::vfs::Vfs;
use glam::Vec3;
use std::path::PathBuf;
use std::sync::Arc;

mod menu;
mod viewer;

#[derive(Parser, Clone)]
#[command(name = "gilde-game")]
pub struct Args {
    /// Game installation directory (contains Resources/). Defaults to $GILDE_DIR,
    /// then a search of the current directory, its parents and their subfolders.
    #[arg(long, env = "GILDE_DIR", default_value_os_t = default_game_dir())]
    game: PathBuf,
    /// Scene or city to open directly, skipping the menu (VFS path, e.g.
    /// "scenes/*stadt_augsburg.ed3" or "resources/gamedata/cities/koeln.cty").
    #[arg(long)]
    scene: Option<String>,
    /// City for "Neues Spiel" (game.ini `Stadt`).
    #[arg(long, default_value = "Augsburg")]
    city: String,
    /// Render one frame offscreen to this PNG and exit.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// Camera position "x,y,z" (defaults to the scene's MegaCam).
    #[arg(long, value_parser = parse_vec3, allow_hyphen_values = true)]
    cam: Option<Vec3>,
    /// Camera target "x,y,z".
    #[arg(long, value_parser = parse_vec3, allow_hyphen_values = true)]
    look: Option<Vec3>,
    #[arg(long, default_value_t = 1280)]
    width: u32,
    #[arg(long, default_value_t = 720)]
    height: u32,
    /// Time of day "HH:MM" for lighting (game days start at 06:00).
    #[arg(long, default_value = "12:00")]
    time: String,
    /// Round (day) number; the season is round % 4.
    #[arg(long, default_value_t = 0)]
    round: i32,
    /// Spawn a few animated characters at the camera target (debug).
    #[arg(long)]
    demo_actors: bool,
    /// Animation time in seconds for --demo-actors screenshots.
    #[arg(long, default_value_t = 0.0)]
    anim_time: f32,
    /// Render the main menu offscreen to this PNG and exit.
    #[arg(long)]
    menu_screenshot: Option<PathBuf>,
}

fn default_game_dir() -> PathBuf {
    gilde_formats::vfs::find_game_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn parse_vec3(s: &str) -> Result<Vec3, String> {
    let v: Vec<f32> = s.split(',').map(|p| p.trim().parse::<f32>().map_err(|e| e.to_string())).collect::<Result<_, _>>()?;
    match v[..] {
        [x, y, z] => Ok(Vec3::new(x, y, z)),
        _ => Err("expected x,y,z".into()),
    }
}

pub struct Loaded {
    pub world: World,
    pub assets: Assets,
    pub camera: Camera,
    pub date: gilde_sim::Date,
}

impl Loaded {
    pub fn lighting(&self) -> Option<gilde_engine::render::Lighting> {
        let p = gilde_sim::DayPhase::at(&self.date);
        gilde_engine::world::lighting(&self.world.light_sets, p.set, p.blend)
    }
}

impl Args {
    pub fn scene_path(&self) -> String {
        self.scene.clone().unwrap_or_else(|| format!("resources/gamedata/cities/{}.cty", self.city.to_lowercase()))
    }
}

pub fn load(args: &Args) -> Result<Loaded> {
    let vfs = Arc::new(Vfs::open_game_dir(&args.game)?);
    let mut assets = Assets::new(vfs.clone());
    let path = args.scene_path();
    let data = vfs.read(&path).with_context(|| format!("reading {path}"))?;
    let scene = if path.to_lowercase().ends_with(".cty") {
        let w = gilde_formats::world::WorldFile::parse(&data)?;
        log::info!("city {:?}: {} buildings, {} characters", w.header.name, w.buildings.len(), w.characters.list.len());
        w.scene.context("city file without scene")?
    } else {
        Scene::parse(&data)?
    };
    let world = World::from_scene(&scene, &mut assets);
    log::info!("{} instances, {} markers", world.instances.len(), world.markers.len());

    let mut camera = Camera::default();
    if let Some(m) = world.camera {
        camera.position = m.w_axis.truncate();
        camera.look_at(Vec3::ZERO.with_y(camera.position.y * 0.2));
    }
    if let Some(p) = args.cam {
        camera.position = p;
    }
    if let Some(t) = args.look {
        camera.look_at(t);
    }
    let mut date = gilde_sim::Date::start_of_round(args.round);
    if let Some((h, m)) = args.time.split_once(':') {
        date.hour = h.parse().unwrap_or(12);
        date.minute = m.parse().unwrap_or(0);
    }
    Ok(Loaded { world, assets, camera, date })
}

pub fn open_ui_assets(game: &std::path::Path, vfs: &Vfs) -> Result<gilde_ui::UiAssets> {
    let dir = gilde_formats::vfs::find_ci(game, "gfx").context("gfx directory not found")?;
    let file = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gfx")))
        .context("no .gfx archive in gfx/")?;
    let gfx = gilde_formats::gfx::GfxArchive::open(file)?;
    let text = gilde_formats::text::TextDb::load(vfs, "german")?;
    Ok(gilde_ui::UiAssets::new(gfx, text))
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"))
        .init();
    let args = Args::parse();
    if let Some(out) = &args.menu_screenshot {
        let vfs = Vfs::open_game_dir(&args.game)?;
        let assets = open_ui_assets(&args.game, &vfs)?;
        let menu = menu::MainMenu::new(&vfs, &assets)?;
        let mut list = gilde_ui::DrawList::default();
        menu.screen.draw(&assets, &mut list);
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let (_adapter, device, queue) = pollster::block_on(gilde_engine::render::create_device(None, &instance))?;
        let fmt = wgpu::TextureFormat::Rgba8UnormSrgb;
        let mut ui = gilde_engine::ui::UiRenderer::new(&device, fmt);
        let px = gilde_engine::render::capture(&device, &queue, fmt, args.width, args.height, |view| {
            let mut enc = device.create_command_encoder(&Default::default());
            enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            queue.submit([enc.finish()]);
            ui.render(&device, &queue, view, args.width, args.height, &assets, &list);
        });
        image::save_buffer(out, &px, args.width, args.height, image::ExtendedColorType::Rgba8)?;
        log::info!("wrote {}", out.display());
        return Ok(());
    }
    if let Some(out) = &args.screenshot {
        let mut l = load(&args)?;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let (_adapter, device, queue) = pollster::block_on(gilde_engine::render::create_device(None, &instance))?;
        let mut r = Renderer::new(device, queue, wgpu::TextureFormat::Rgba8UnormSrgb);
        r.load_world(&l.world, &mut l.assets);
        if let Some(light) = l.lighting() {
            r.lighting = light;
        }
        if args.demo_actors {
            let center = args.look.unwrap_or(glam::Vec3::ZERO);
            let demo = demo_actors(&mut l, center);
            let frames: Vec<gilde_engine::render::ActorFrame> = demo
                .iter()
                .enumerate()
                .map(|(i, (mesh, clip, xf))| gilde_engine::render::ActorFrame {
                    mesh,
                    clip: clip.as_deref(),
                    time: clip.as_ref().map_or(0.0, |c| c.looped_time(args.anim_time + i as f32 * 0.37)),
                    transform: *xf,
                })
                .collect();
            r.update_actors(&mut l.assets, &frames);
        }
        let px = r.render_to_rgba(args.width, args.height, &l.camera);
        image::save_buffer(out, &px, args.width, args.height, image::ExtendedColorType::Rgba8)?;
        log::info!("wrote {}", out.display());
        return Ok(());
    }
    viewer::run(args)
}

type DemoActor = (std::sync::Arc<gilde_engine::assets::MeshData>, Option<std::sync::Arc<gilde_engine::anim::AnimClip>>, glam::Mat4);

/// Debug: a row of townsfolk playing their body type's walk cycle.
pub fn demo_actors(l: &mut Loaded, center: Vec3) -> Vec<DemoActor> {
    let people = [
        ("arbeiter_mann", "mann"),
        ("buergerin_frau", "frau"),
        ("priester_kutte", "kutte"),
        ("hellebardentraeger_soldat", "soldat"),
        ("wirt_dicker", "dicker"),
        ("patrizier_mann2", "mann2"),
        ("magd_frau", "frau"),
        ("bettler_kutte", "kutte"),
    ];
    let mut out = Vec::new();
    for (i, (mesh, body)) in people.iter().enumerate() {
        let Some(m) = l.assets.mesh(mesh) else { continue };
        let clip = l.assets.anim(&format!("animations/character/{body}/bewegung/gehen_{body}.baf"));
        let x = center.x + (i as f32 - people.len() as f32 / 2.0) * 40.0;
        let z = center.z;
        let y = l.world.terrain.as_ref().and_then(|t| t.height_at(x, z)).unwrap_or(center.y);
        let xf = glam::Mat4::from_translation(Vec3::new(x, y, z)) * glam::Mat4::from_rotation_y(i as f32 * 0.6);
        out.push((m, clip, xf));
    }
    out
}

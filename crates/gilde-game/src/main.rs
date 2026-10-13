//! Die Gilde reimplementation — viewer / game entry point.

use anyhow::{Context, Result};
use clap::Parser;
use gilde_engine::{Assets, Camera, Renderer, World};
use gilde_formats::scene::Scene;
use gilde_formats::vfs::Vfs;
use glam::Vec3;
use std::path::PathBuf;
use std::sync::Arc;

mod viewer;

#[derive(Parser)]
#[command(name = "gilde-game")]
pub struct Args {
    /// Game installation directory (contains Resources/). Defaults to $GILDE_DIR.
    #[arg(long, env = "GILDE_DIR")]
    game: PathBuf,
    /// Scene to load (VFS path, e.g. "scenes/*stadt_augsburg.ed3").
    #[arg(long, default_value = "scenes/*stadt_augsburg.ed3")]
    scene: String,
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
}

pub fn load(args: &Args) -> Result<Loaded> {
    let vfs = Arc::new(Vfs::open_game_dir(&args.game)?);
    let mut assets = Assets::new(vfs.clone());
    let data = vfs.read(&args.scene).with_context(|| format!("reading {}", args.scene))?;
    let scene = if args.scene.to_lowercase().ends_with(".cty") {
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
    Ok(Loaded { world, assets, camera })
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,wgpu_core=warn,wgpu_hal=warn,naga=warn"))
        .init();
    let args = Args::parse();
    if let Some(out) = &args.screenshot {
        let mut l = load(&args)?;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let (_adapter, device, queue) = pollster::block_on(gilde_engine::render::create_device(None, &instance))?;
        let mut r = Renderer::new(device, queue, wgpu::TextureFormat::Rgba8UnormSrgb);
        r.load_world(&l.world, &mut l.assets);
        let px = r.render_to_rgba(args.width, args.height, &l.camera);
        image::save_buffer(out, &px, args.width, args.height, image::ExtendedColorType::Rgba8)?;
        log::info!("wrote {}", out.display());
        return Ok(());
    }
    viewer::run(args)
}

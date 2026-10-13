//! `gilde` — command-line tools for inspecting and converting Die Gilde game data.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use gilde_formats::vfs::Vfs;
use std::path::PathBuf;

mod check;
mod export;

#[derive(Parser)]
#[command(name = "gilde", about = "Die Gilde Gold data tools")]
struct Cli {
    /// Game installation directory (contains Resources/). Defaults to $GILDE_DIR.
    #[arg(long, global = true, env = "GILDE_DIR")]
    game: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse every file of every supported format and report failures.
    Check {
        /// Only check this extension (e.g. bgf).
        #[arg(long)]
        ext: Option<String>,
        /// Print each failing path.
        #[arg(short, long)]
        verbose: bool,
    },
    /// List VFS paths, optionally filtered by substring.
    Ls { filter: Option<String> },
    /// Extract a file from the VFS to stdout or a path.
    Cat { path: String, out: Option<PathBuf> },
    /// Dump a parsed file in debug form.
    Dump { path: String },
    /// Print the object tree of an .ed3 scene or .ogr group.
    Tree { path: String },
    /// Decode every SHAPBANK in the .gfx archive and report failures.
    GfxCheck,
    /// Export a .gfx sprite (e.g. "_MENUE_BACKGROUND" or "_FONT+1") to PNG.
    GfxPng { name: String, out: PathBuf },
    /// Export all frames of a .gfx bank side by side to one PNG.
    GfxSheet { name: String, out: PathBuf },
    /// Print strings from the global text table by id or key.
    Text { ids: Vec<String> },
    /// Compare a mesh's vertex count with an animation's frame size.
    AnimInfo { mesh: String, anim: String },
    /// Export a BGF mesh to Wavefront OBJ (+MTL, textures copied as BMP).
    Obj { path: String, out_dir: PathBuf },
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let game = cli.game.or_else(gilde_formats::vfs::find_game_dir).context("game directory not found: pass --game <dir> or set GILDE_DIR")?;
    let vfs = Vfs::open_game_dir(&game)?;
    match cli.cmd {
        Cmd::Check { ext, verbose } => check::run(&vfs, ext.as_deref(), verbose),
        Cmd::Ls { filter } => {
            for p in vfs.list() {
                if filter.as_deref().is_none_or(|f| p.contains(&f.to_lowercase())) {
                    println!("{p}");
                }
            }
            Ok(())
        }
        Cmd::Cat { path, out } => {
            let data = vfs.read(&path)?;
            match out {
                Some(o) => std::fs::write(o, data)?,
                None => std::io::Write::write_all(&mut std::io::stdout(), &data)?,
            }
            Ok(())
        }
        Cmd::Dump { path } => check::dump(&vfs, &path),
        Cmd::GfxCheck => {
            let gfx = open_gfx(&game)?;
            let (mut ok, mut bad) = (0, 0);
            for e in gfx.entries.iter().filter(|e| e.size > 0) {
                match gfx.raw(e).map_err(anyhow::Error::from).and_then(|b| Ok(gilde_formats::gfx::ShapBank::parse(&b)?)) {
                    Ok(_) => ok += 1,
                    Err(err) => {
                        bad += 1;
                        println!("{}: {err}", e.name);
                    }
                }
            }
            println!("gfx: {ok}/{} banks ok", ok + bad);
            Ok(())
        }
        Cmd::GfxPng { name, out } => {
            let s = open_gfx(&game)?.sprite(&name)?;
            image::save_buffer(&out, &s.rgba, s.width, s.height, image::ExtendedColorType::Rgba8)?;
            Ok(())
        }
        Cmd::GfxSheet { name, out } => {
            let bank = open_gfx(&game)?.bank(&name)?;
            let w: u32 = bank.frames.iter().map(|f| f.width + 2).sum::<u32>().max(1);
            let h = bank.frames.iter().map(|f| f.height).max().unwrap_or(1).max(1);
            let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba([40, 40, 80, 255]));
            let mut x = 0;
            for f in &bank.frames {
                let fi = image::RgbaImage::from_raw(f.width, f.height, f.rgba.clone()).unwrap();
                image::imageops::overlay(&mut img, &fi, x as i64, 0);
                x += f.width + 2;
            }
            println!("{} frames: {:?}", bank.frames.len(), bank.frames.iter().map(|f| (f.width, f.height)).collect::<Vec<_>>());
            img.save(&out)?;
            Ok(())
        }
        Cmd::Text { ids } => {
            let db = gilde_formats::text::TextDb::load(&vfs, "german")?;
            println!("{} strings", db.len());
            for i in ids {
                let id = i.parse::<u32>().ok().or_else(|| db.id(&i));
                println!("{i} -> {id:?}: {:?}", id.and_then(|id| db.get(id)));
            }
            Ok(())
        }
        Cmd::AnimInfo { mesh, anim } => {
            let m = gilde_formats::bgf::Bgf::parse(&vfs.read(&mesh)?)?;
            let a = gilde_formats::baf::Baf::parse(&vfs.read(&anim)?)?;
            let parts: Vec<usize> = m.objects.iter().flat_map(|o| o.parts.iter().map(|p| p.vertices.len())).collect();
            println!("mesh parts {parts:?} total {}", parts.iter().sum::<usize>());
            println!(
                "anim frames {} verts/frame {:?} parts {} loop {}..{} times {:?}",
                a.frames.len(),
                a.frames.iter().map(|f| f.vertices.len()).collect::<std::collections::BTreeSet<_>>(),
                a.part_count,
                a.loop_in,
                a.loop_out,
                a.frames.iter().map(|f| f.time).collect::<Vec<_>>()
            );
            if let Some(f) = a.frames.first() {
                println!("root {:?} {:?} dummies {:?}", f.root_position, f.root_rotation, f.dummies.iter().map(|d| &d.name).collect::<Vec<_>>());
            }
            Ok(())
        }
        Cmd::Tree { path } => check::tree(&vfs, &path),
        Cmd::Obj { path, out_dir } => export::obj(&vfs, &path, &out_dir),
    }
}

fn open_gfx(game: &std::path::Path) -> Result<gilde_formats::gfx::GfxArchive> {
    let dir = gilde_formats::vfs::find_ci(game, "gfx").context("gfx directory not found")?;
    let file = std::fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gfx")))
        .context("no .gfx archive")?;
    Ok(gilde_formats::gfx::GfxArchive::open(file)?)
}

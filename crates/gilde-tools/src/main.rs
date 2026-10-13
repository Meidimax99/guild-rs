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
    /// Export a BGF mesh to Wavefront OBJ (+MTL, textures copied as BMP).
    Obj { path: String, out_dir: PathBuf },
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();
    let game = cli.game.context("pass --game <dir> or set GILDE_DIR")?;
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
        Cmd::Tree { path } => check::tree(&vfs, &path),
        Cmd::Obj { path, out_dir } => export::obj(&vfs, &path, &out_dir),
    }
}

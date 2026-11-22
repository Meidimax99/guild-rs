# gilde-rs

A Rust reimplementation of **Die Gilde Gold** (*The Guild Gold*, JoWooD / 4HEAD
Studios, 2002–2003), reverse engineered from `GildeGold_TL.exe` (v2.05) with
Ghidra. Like OpenMW for Morrowind, it is an engine replacement: it needs the
original game data, which is not included here.

## Status

| Area | State |
|---|---|
| Resource VFS (`.bin` ZIP archives, `*name` lookups) | done |
| Text tables `.res` | done |
| Meshes `.bgf`, vertex animations `.baf` | done (all shipped files) |
| Scenes `.ed3`, object groups `.ogr` | done (all shipped files) |
| City/world state `.cty` (+ savegame prefix) | parsed; field semantics in progress |
| 2D graphics archive `.gfx` (SHAPBANK sprites, fonts) | done |
| UI forms `.form` (FRM2) | layout decoded |
| ESC event-script interpreter | done (parser, VM, scheduler; all 461 scripts parse) |
| 3D renderer (wgpu): terrain, meshes, cutout textures | working |
| Characters, animation playback, navigation | todo |
| UI system (windows, controls, fonts) | todo |
| Audio (Miles → native) | todo |
| Game clock, replicated commands, game server core | started (`gilde-sim`, `gilde-net`) |
| Game simulation (economy, dynasties, politics, AI) | todo |

## Layout

```
crates/
  gilde-formats   parsers for every file format, VFS
  gilde-esc       ESC script language: parser, VM, scheduler
  gilde-engine    wgpu renderer, world assembly, camera
  gilde-sim       game simulation: date, clock, commands, drives, AI scheduling, notices
  gilde-net       game server core and TCP transport (server.dll replacement)
  gilde-game      the executable (viewer for now)
  gilde-tools     `gilde` CLI: check/dump/convert game data
docs/             reverse-engineering notes (formats, ESC, addresses)
re/               Ghidra scripts and helpers used for the analysis
```

## Usage

The game directory is found automatically if it sits in the current directory,
one of its parents, or a sibling folder; otherwise pass `--game <dir>` or set
`GILDE_DIR` (fish: `set -x GILDE_DIR "/path/to/The Guild Gold (German)"`).

```sh

cargo run --release --bin gilde -- check          # parse every shipped file
cargo run --release --bin gilde -- tree 'scenes/*stadt_augsburg.ed3'
cargo run --release --bin gilde -- gfx-png _MENUE_BACKGROUND menu.png
cargo run --release --bin gilde -- obj 'objects/*ub_PFOSTEN_LAGERHAUS.bgf' out/
cargo run --release --bin gilde -- sim path/to/save.SAV --minutes 600   # headless simulation
cargo run --release --bin gilde -- server                               # game server on TCP 7531

cargo run --release --bin gilde-game -- --scene resources/gamedata/cities/augsburg.cty
cargo run --release --bin gilde-game -- --screenshot shot.png --cam 0,9000,9000 --look 0,0,0
```

Viewer controls: WASD/QE move, right mouse drag to look, shift = fast, wheel = speed.

## Documentation

See [`docs/`](docs/README.md): repository structure, game and server
architecture, reverse-engineering progress, file and savegame formats, and the
ESC script language.

## Reverse engineering

`re/decompile.sh` reproduces the full decompile of the executable and
`server.dll` with headless Ghidra: `FindFunctions.java` recovers functions the
auto-analysis misses, `ExportAll.java` decompiles every function and dumps
strings with cross references, and `DecompileAt.java` decompiles code that
Ghidra only reaches through data tables. `re/scripts/difftest.py` runs original
functions in an x86 emulator (conda env `guild`) and records test vectors that
the Rust port is checked against. Function addresses in comments and docs refer
to `GildeGold_TL.exe` (SHA-256 `cd40c06e…f6d0`, base `0x400000`).

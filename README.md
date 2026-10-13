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
| Game simulation (economy, dynasties, politics, AI) | todo |

## Layout

```
crates/
  gilde-formats   parsers for every file format, VFS
  gilde-esc       ESC script language: parser, VM, scheduler
  gilde-engine    wgpu renderer, world assembly, camera
  gilde-game      the executable (viewer for now)
  gilde-tools     `gilde` CLI: check/dump/convert game data
docs/             reverse-engineering notes (formats, ESC, addresses)
re/               Ghidra scripts and helpers used for the analysis
```

## Usage

```sh
export GILDE_DIR="/path/to/The Guild Gold (German)"

cargo run --release --bin gilde -- check          # parse every shipped file
cargo run --release --bin gilde -- tree 'scenes/*stadt_augsburg.ed3'
cargo run --release --bin gilde -- gfx-png _MENUE_BACKGROUND menu.png
cargo run --release --bin gilde -- obj 'objects/*ub_PFOSTEN_LAGERHAUS.bgf' out/

cargo run --release --bin gilde-game -- --scene resources/gamedata/cities/augsburg.cty
cargo run --release --bin gilde-game -- --screenshot shot.png --cam 0,9000,9000 --look 0,0,0
```

Viewer controls: WASD/QE move, right mouse drag to look, shift = fast, wheel = speed.

## Reverse engineering

`re/ghidra_scripts/ExportAll.java` decompiles every function and dumps strings
with cross references (headless Ghidra); `DecompileAt.java` decompiles code
that Ghidra only reaches through data tables. Function addresses in comments
and docs refer to `GildeGold_TL.exe` (SHA-256 `cd40c06e…f6d0`, base `0x400000`).

# Repository structure

```
gilde-rs/
├── Cargo.toml            workspace (edition 2024, shared dependency versions)
├── README.md             overview, status, usage
├── crates/
│   ├── gilde-formats/    file formats and VFS (no GPU, no game logic)
│   ├── gilde-esc/        ESC script language
│   ├── gilde-sim/        game simulation (calendar so far)
│   ├── gilde-ui/         2D UI model, renderer-independent
│   ├── gilde-engine/     wgpu renderer and world assembly
│   ├── gilde-game/       the game executable
│   └── gilde-tools/      the `gilde` data CLI
├── docs/                 reverse-engineering documentation
└── re/                   reverse-engineering tooling
```

## Crate dependency graph

```
gilde-formats ◄── gilde-ui ◄── gilde-engine ◄── gilde-game
      ▲              ▲              ▲              │
      │              └──────────────┼──────────────┤
      ├── gilde-esc                 │              │
      ├── gilde-sim ◄───────────────┼──────────────┘
      └── gilde-tools ──────────────┘ (formats + esc)
```

Rules of thumb: `gilde-formats` knows nothing about rendering or game rules;
`gilde-ui` produces draw lists but never touches the GPU; `gilde-engine` owns
all wgpu code; `gilde-sim` must stay renderer-free so it can run headless (tests,
future server); `gilde-game` wires everything together.

## `gilde-formats`

| Module | Contents |
|---|---|
| `reader` | little-endian cursor, Windows-1252 string helpers |
| `error` | shared `Error`/`Result` |
| `vfs` | mounts `Resources/*.bin` ZIP archives + loose files, `*name` subtree search, game-directory auto-detection |
| `res` | `.res` string tables |
| `text` | global text database (all `.res` files merged by id, lookup by key) |
| `bgf` | static meshes, `to_engine_space` coordinate conversion |
| `baf` | vertex animations |
| `scene` | `.ed3` scenes and `.ogr` groups (object trees, floor, light sets) |
| `world` | `.cty` city / savegame prefix (world state + embedded scene) |
| `gfx` | `.gfx` archive and SHAPBANK sprite decoding |
| `form` | `.form` UI layouts |

Each parser documents its layout and the original loader's address in its
module doc comment; [`formats.md`](formats.md) is the prose overview.

## `gilde-esc`

| Module | Contents |
|---|---|
| `ast` | types, expressions in the engine's evaluation shape, statements |
| `parser` | tokenizer + lenient parser mirroring the engine's tolerance |
| `vm` | compilation to resumable instructions, `Scheduler` (ticks, step modes, latent host calls, `exit()`), `Host` trait for engine functions |

`tests/vm.rs` pins the reconstructed semantics. Language reference:
[`esc.md`](esc.md).

## `gilde-sim`

| Module | Contents |
|---|---|
| `date` | `Date` (round/hour/minute/second, `add`, ordering, season, calendar), `DayPhase` (light set + blend from the seasonal table) |

## `gilde-ui`

| Module | Contents |
|---|---|
| `assets` | `UiAssets` (gfx archive + text DB + decoded-bank cache), `Font` metrics |
| `draw` | `DrawList`/`DrawCmd` in 800×600 UI space, original tint constants |
| `text` | layout and drawing (wrapping, soft hyphens, alignment, kerning) |
| `widgets` | `Window` (from form records), `Widget` (menu button, label, image), `Screen` (z-order, hit testing, mouse events) |

## `gilde-engine`

| Module | Contents |
|---|---|
| `assets` | `Assets`: VFS-backed cache of meshes (`MeshData`), textures, animation clips |
| `anim` | `AnimClip` (BAF sampling, loop handling, 30 fps timeline) |
| `terrain` | heightfield mesh, height queries, layer weights |
| `world` | `World` built from a scene: mesh instances, markers, terrain, light sets; `object_rotation`; `lighting` (light-set blending) |
| `camera` | left-handed perspective camera |
| `render` | `Renderer` (terrain, instanced meshes, animated actors, cutout pass, fog), `Lighting`, `create_device`, offscreen `capture` |
| `ui` | `UiRenderer` rasterising `gilde_ui::DrawList`, `to_ui` coordinate mapping |
| `shaders/` | `mesh.wgsl`, `terrain.wgsl`, `ui.wgsl` |

## `gilde-game`

| Module | Contents |
|---|---|
| `main` | CLI (`--game`, `--scene`, `--city`, `--time`, `--round`, screenshot modes, `--demo-actors`), loading a city/scene |
| `menu` | main menu screen (layout and actions from `menu_Main`) |
| `viewer` | winit application: menu ↔ city state machine, input, frame loop |

## `gilde-tools` (`gilde` binary)

| Command | Purpose |
|---|---|
| `check [--ext X] [-v]` | parse every shipped file of every supported format |
| `ls`, `cat`, `dump`, `tree` | browse and inspect VFS contents |
| `text <id or key>…` | look up strings |
| `gfx-check`, `gfx-png`, `gfx-sheet` | validate and export 2D graphics |
| `obj` | export a mesh to Wavefront OBJ |
| `anim-info` | compare a mesh with an animation |

## `docs/`

| File | Contents |
|---|---|
| [`reverse-engineering.md`](reverse-engineering.md) | progress, address map, decoded records, open questions |
| [`formats.md`](formats.md) | file format reference |
| [`esc.md`](esc.md) | ESC language and interpreter |
| [`esc_api.tsv`](esc_api.tsv) | the 129 script host functions with signatures |
| `repository.md` | this file |

## `re/`

| Path | Purpose |
|---|---|
| `ghidra_scripts/ExportAll.java` | decompile all functions, dump strings with xrefs |
| `ghidra_scripts/DecompileAt.java` | create + decompile functions at given addresses |
| `ghidra_scripts/ApplyNames.java` | apply `names.tsv` to the Ghidra project |
| `decompile.sh` | reproduce the full decompiled source outside the repo (import, apply names, export, split by module) |
| `names.tsv` | identified functions and globals (address, name, comment) |
| `scripts/pe.py` | minimal PE reader (virtual address → bytes) |
| `scripts/bgf_grammar.py` | dump the BGF chunk grammar tables |
| `scripts/esc_api.py` | extract script host-function registrations |
| `scripts/gfx_dump.py` | reference SHAPBANK → PNG decoder |
| `scripts/split_decomp.py` | split `decomp.c` into per-module files + `index.tsv` |

## Outside the repository

The analysis uses a few working directories next to the repo that are not
committed (they contain copyrighted game data or derived code):

| Path | Contents |
|---|---|
| `../The Guild Gold (German)/` | the game installation (auto-detected) |
| `../unpacked/` | extracted `.bin` archives for manual inspection |
| `../re-out/` | headless Ghidra project, `src/` (decompiled source split by module, see below), `decomp.c`, `functions.tsv`, `strings.tsv`, helper scripts `bin/fn` (print a function), `bin/dec` (decompile at addresses), `bin/reexport` (apply names + re-export) |

## Conventions

* Comments cite the original routine (`FUN_xxxxxxxx` or its name from
  `names.tsv`) wherever behaviour is reproduced.
* Deliberate deviations from the original are marked "Deviation:" in comments;
  unverified assumptions are marked `TODO(re)`.
* New formats must be added to `gilde check` and pass on all shipped files.

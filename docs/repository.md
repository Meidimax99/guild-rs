# Repository structure

```
gilde-rs/
├── Cargo.toml            workspace (edition 2024, shared dependency versions)
├── README.md             overview, status, usage
├── crates/
│   ├── gilde-formats/    file formats and VFS (no GPU, no game logic)
│   ├── gilde-esc/        ESC script language
│   ├── gilde-sim/        game simulation: calendar, clock, commands
│   ├── gilde-net/        networking: server core, wire framing
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
| `ai` | AI action catalogue `ai_data_2.dfn` |
| `savegame` | savegame dynamic state: sections 1–5 up to the pending notices |
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
| `date` | `Date` (round/hour/minute/second, `add`, ordering, season, calendar, 14-byte encoding), `DayPhase` (light set + blend from the seasonal table) |
| `clock` | `LocalClock` (single-player `clk_Advance`), `ServerClock` (server tick, speed table, end of round) |
| `command` | `Command` (0x99-byte record, known types and system sub-types), `Executor` (handler table, transactions, sequence gaps) |
| `drives` | `Drives`: the 13 drives per character and the activity choice (`chr_UpdateDrives`) |
| `ai` | `AiScheduler`: which characters get their hourly update and an AI decision each minute (`ai_Schedule`) |
| `actions` | AI actions: action table (code registration + `ai_data_2.dfn`), scoring, selection, execution, drive-delta command `0x18`, `Planner` trait for the action-specific functions |
| `notices` | `Notices`: pending notices and their deadlines (`msg_ProcessNotices`, handlers not ported) |
| `world` | `World`: runtime world built from a city/save, per-minute step (notices, drives, AI scheduling) |

## `gilde-net`

| Module | Contents |
|---|---|
| `server` | `Server`: transport-independent game server (handshake, lobby, client indices, validation through an `Authority`, echo and broadcast, game clock, timeouts) |
| `frame` | wire framing and stream `Decoder` |
| `tcp` | `ServerHandle` (server thread over TCP, UDP lobby announcement), `Client` |

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
| `server [--port] [--players] [--speed]` | run the game server standalone (no game directory needed) |
| `sim <city or save> [--minutes]` | run the ported simulation headless and report activities and AI visits per hour |

## `docs/`

| File | Contents |
|---|---|
| [`reverse-engineering.md`](reverse-engineering.md) | progress, address map, decoded records, open questions |
| [`formats.md`](formats.md) | file format reference |
| [`savegame.md`](savegame.md) | savegame dynamic state, header, network games |
| [`architecture.md`](architecture.md) | game architecture: frame loop, commands, clock, networking |
| [`commands.tsv`](commands.tsv) | the 106 network command types with client and server handlers |
| [`ai_actions.tsv`](ai_actions.tsv) | the 72 AI actions with their score/target/execute functions and flags |
| [`notices.tsv`](notices.tsv) | the 167 notice (HE event) types with handlers and message titles |
| [`esc.md`](esc.md) | ESC language and interpreter |
| [`esc_api.tsv`](esc_api.tsv) | the 129 script host functions with signatures |
| `repository.md` | this file |

## `re/`

| Path | Purpose |
|---|---|
| `ghidra_scripts/ExportAll.java` | decompile all functions, dump strings with xrefs |
| `ghidra_scripts/DecompileAt.java` | create + decompile functions at given addresses |
| `ghidra_scripts/ApplyNames.java` | apply `names.tsv` to the Ghidra project |
| `ghidra_scripts/FindFunctions.java` | create functions in code that auto-analysis missed |
| `names_server.tsv` | identified functions in `server/server.dll` |
| `decompile.sh` | reproduce the full decompiled source outside the repo (import, apply names, export, split by module) |
| `names.tsv` | identified functions and globals (address, name, comment) |
| `scripts/pe.py` | minimal PE reader (virtual address → bytes) |
| `scripts/bgf_grammar.py` | dump the BGF chunk grammar tables |
| `scripts/esc_api.py` | extract script host-function registrations |
| `scripts/gfx_dump.py` | reference SHAPBANK → PNG decoder |
| `scripts/split_decomp.py` | split `decomp.c` into per-module files + `index.tsv` |
| `scripts/difftest.py` | run original functions in an x86 emulator and record test vectors for `gilde-sim` |
| `scripts/ai_methods.py` | dump the AI action registration table (functions, flags) with the emulator |

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

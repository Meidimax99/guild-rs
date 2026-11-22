# Reverse-engineering progress

State of the analysis of `GildeGold_TL.exe` (Die Gilde Gold 2.05, German, SHA-256
`cd40c06ee82db8e240a32d50f24481dfa980594a6faebd5c55989ef08cb2f6d0`, image base
`0x400000`, 32-bit x86, MSVC, Direct3D 8, Miles Sound System). All addresses in
this repository refer to this binary.

Status legend: **done** (understood and implemented in Rust, verified against
shipped data), **mapped** (understood well enough to describe, not yet ported),
**located** (entry points known, internals not analysed), **open** (not started).

## Coverage

The executable has 6496 functions after a function search over the uncovered
code (`re/ghidra_scripts/FindFunctions.java`), covering 97.9 % of `.text`
(77 % before). `server/server.dll` (729 functions) is decompiled into
`re-out/server/`.

## Decompiled source

The complete decompiled C (Ghidra output, all ~4800 functions, with every name
from `re/names.tsv` applied) is generated locally and deliberately **not
committed**, because it is derived from the copyrighted executable:

```sh
re/decompile.sh "/path/to/The Guild Gold (German)/GildeGold_TL.exe"   # → ../re-out/src/
```

`../re-out/src/` then holds one file per module of the address map below
(`d3_engine.c`, `d2_ui.c`, `game_logic.c`, …) and `index.tsv` (address, name,
module). Re-run the script after adding names to see them throughout.

## Method and tooling

* **Static analysis** with Ghidra 12.1 (headless) and the REA MCP tools. No
  debugger or runtime tracing has been used yet.
* `re/ghidra_scripts/ExportAll.java` decompiles all ~4800 functions into one
  `decomp.c` and dumps every defined string with its cross references
  (`strings.tsv`), so the code can be searched with ordinary text tools.
* `re/ghidra_scripts/DecompileAt.java` creates and decompiles functions that
  Ghidra never discovered because they are only reachable through pointer
  tables (BGF chunk handlers, message handlers, script commands).
* `re/ghidra_scripts/ApplyNames.java` applies `re/names.tsv` (address, name,
  comment) to the project. Re-exporting after renaming takes ~45 s and makes the
  whole decompile progressively more readable; **every newly identified
  function or global should be added to `re/names.tsv`**.
* `re/scripts/`: PE reader (`pe.py`), BGF grammar dumper, ESC host-API extractor,
  SHAPBANK-to-PNG decoder used to validate the gfx format.
* Verification: every parser is run over every shipped file by `gilde check` and
  must consume each file exactly. Rendering is checked with offscreen
  screenshots (`gilde-game --screenshot`, `--menu-screenshot`).

Useful anchors in the binary: debug strings name many routines and allocation
tags (`d3_io_b:d3_LoadBinaryAnimation`, `f3_gm_lpGebaeudePROT`,
`evt_LoadScript`, `d2_Open`, `amt_fio_LoadAemter`, …). String prefixes give a
rough module map: `d3_*` 3D engine, `d2_*` UI, `evt`/`ecmd` scripting,
`cm`/`ch` characters, `ai`/`aip` AI, `hst` history, `gm`/`f3_gm` game manager,
`sv`/`ls` save/load, `nt` network, `gv` graveyard, `amt` offices, `txt` text.

## Address map (approximate)

| Range | Contents |
|---|---|
| `0x401000`–`0x410000` | ESC host commands (`ecmd`), character/object helpers for scripts |
| `0x410000`–`0x420000` | camera, input, sound wrappers |
| `0x420000`–`0x424a00` | ESC interpreter (`evt_*`) |
| `0x424a00`–`0x427000` | Miles sound wrappers (`sfx_*`) |
| `0x427000`–`0x42b000` | text tables, timer, VFS |
| `0x42b000`–`0x459000` | 3D engine: objects, animation, scene/mesh I/O, floor, particles, sky |
| `0x459000`–`0x46c000` | 2D UI (`d2_*`, 227 functions) |
| `0x46c000`–`0x4f0000` | game logic: characters, buildings, AI, offices, events |
| `0x4f0000`–`0x52c000` | game manager, save/load, cities, main/session loop |
| `0x52c000`–`0x5c0000` | timed message (action) handlers and gameplay screens |
| `0x5c0000`–`0x605000` | menus, options, network |
| `0x605000`–`0x615000` | MSVC runtime |
| `0x620000`–`0x1523304` | `.data`/BSS: global game state arrays |

## Subsystem status

### Program flow — mapped

* `WinMain` (`0x52b320`) reads `game.ini`, initialises D3D and the archives
  (`gfx_Init` `0x520050`), then alternates between `menu_Main` (`0x521c00`) and
  `game_Session` (`0x52a150`).
* `master_Step` (`0x51ed60`, param = subsystem flag mask: `0x200` scripts,
  `0x40` 3D, `0x1` messages, `0x20000` periodic logic every 7 ticks, …) runs
  one frame. Called in loops by every screen with the screen's own function as
  re-entrancy key.
* Timer: multimedia timer, 14 ms period, increments `g_tick` (`0x6612cc`).

### Resource loading — done

VFS (`vfs_Open` `0x429760`, lookup `0x428be0`), `.res` text tables
(`0x427650`), global text table at `0x144a280` (pointer per text id). See
[`formats.md`](formats.md).

### 3D engine — formats done, behaviour partly mapped

* Meshes `.bgf` (`d3_LoadObject` `0x441920`, table-driven parser `0x443530`),
  animations `.baf` (`0x443ed0`), scenes `.ed3` (`0x43fae0`), groups `.ogr`
  (`0x440060`), floor (`0x43f5b0`/`0x438580`), texture sets `.txs`
  (`0x4417e0`): **done**.
* Coordinate conventions: BGF file space is Z-up and converted with
  `(x,y,z) → (−x,z,−y)`; BAF data is already in engine space. Object rotation
  `Rx(−x)·Ry(−y)·Rz(−z)` (`0x4474e0`, applied transposed). **Done**, verified
  visually and against terrain heights.
* Materials: flag `0x0b` bit 0 = black colour key, bit 1 = double-sided.
  **Done.** Seasonal texture variants (`_f/_s/_h/_w`): the engine's own
  substitution was not found; a name-based fallback is used. **Open.**
* Animation playback: per-object 3 slots (`anim_Attach` `0x42df30`), next-frame
  rule `0x42b7d0`, blend weight `0x42ecd0`, root motion `0x42b480`. Keyframe
  times treated as 30 fps (inferred from walk speed, not confirmed in code).
  **Mapped / ported approximately.**
* Day/night: `sky_UpdateLightByTime` (`0x513b70`) maps the time of day onto 7
  scene light sets using per-season transition times (`0x618c40`); blending in
  `0x43daf0`. Light-set fields partly understood (ambient, fog colour/range,
  sun colour). **Ported approximately**; light objects (`0x446620`) not yet.
* Particles (`d3_par`), sky layers (`d3_sky`), water, shadows: **located.**

### 2D UI — mapped, main menu ported

* gfx archive and SHAPBANK sprites (`d2_Open` `0x4635e0`, converter
  `0x469ce0`, blitter `0x469700`): **done.**
* Forms `.form` (`d2_LoadForm` `0x463d60`): layout **done**; forms only
  describe windows, code fills in controls.
* Windows (`d2_CreateWindow` `0x461f70`, drawing `0x461670`/`0x4613e0`): style
  bits 1 fill, 4 border, 8/0x20 icons, 0x10 text area. **Mapped.**
* Object record: 0x2f4 bytes at `*0xfbb300`; type at `+0x20` (1 image,
  5/8 sprite button, 9 menu button, 0x40 window, 0x41 input, 0x43 text,
  0x45 list, 0x47 paintbox). Draw dispatch `d2_DrawAll` `0x45d900`. **Mapped.**
* Text (`d2_DrawText` `0x45f3a0`): word wrap, `~` soft hyphen, alignment flags,
  word spacing 8, letter spacing 2, kerning from the glyph ink box. Text
  formatting codes (`$0>`, `$A`, `%T`, `%1N4`, …) used by `FUN_00503c00` /
  `FUN_00505970`: **open.**
* Blit tints: normal 200, selected 128, hover 255. Fades: `d2_Fade`
  `0x4660d0`. Radio groups / keyboard navigation `0x45cd30`/`0x45cf50`.
* Main menu (`menu_Main`): background gfx `0x797`, form `MENU/MAIN_MENU`, nine
  buttons via `menu_AddButton` `0x521ba0`; also loads the `ChooseCity` and
  `spielerauswahl` 3D scenes and plays random menu music. **Ported** (without
  the 3D backdrop and music). All other screens: **open.**

### ESC scripting — done

Interpreter semantics, scheduling and leniency are documented in
[`esc.md`](esc.md); the 129 host functions in [`esc_api.tsv`](esc_api.tsv).
Host-function implementations (walking, animation, camera flights, particles,
sound) are **located** (`0x40b5d0`, `0x40cfa0`, `0x40f670`, `0x40ff30`
register them) but not ported.

### World state and game data — partly mapped

* Prototypes: `gm_LoadPrototypes` (`0x4ec750`) reads `Data/A_Geb.dat` (88 of 192
  records × 589 bytes) and `Data/A_Obj.dat` (732 × 65). **Partly done**
  (`gilde_formats::proto`, `gilde buildings`). Building record:

  | Offset | Meaning | Evidence |
  |---|---|---|
  | `+0x00` | class (34; e.g. 18 smithy, 29 graveyard, 30 residence) | compared throughout |
  | `+0x01` | internal name (32 bytes) | data |
  | `+0x21` | count of installed workshop objects | incremented in `gm_LoadPrototypes` |
  | `+0x23` | 64 × u16 object slots, 0-terminated; bit 15 = installed when bought | `FUN_004f7f00` |
  | `+0x223` | profession running the building (text `0x130 + n`) | `FUN_00586…` UI, AI |
  | `+0x22f`, `+0x230` | arguments to `FUN_004b3950` | open |
  | `+0x243` | build time in rounds ("Bauzeit") | building tooltip `0x576680` |
  | `+0x247` | tier I–IV | tooltip, graveyard allocation |

  Texts: prototype `i` owns 14 strings from id `0x48e + 14·i` (display name,
  description, 12 upgrade names). Price = sum of the prices of the installed
  objects (`FUN_004f7f00` → `FUN_004f69c0(object category, 100)`). Object record:
  byte 0 item category, name at +1 (32 bytes), values at +0x21… (open).
* City/savegame loader `sv_LoadWorld` (`0x50abf0`): all city sections parsed
  and verified (see [`formats.md`](formats.md)); the savegame-only dynamic
  state (17 loaders, `0x4e0d00`…`0x4ae3c0`) is transcribed in
  [`savegame.md`](savegame.md) §5; sections 1–5 verified on real saves.
* Instance types with extra data: `0x1e` forest (64-cell plant map), `H/I/J`
  graveyards (graves and zombies, counts per tier at `0x6181f0`/`0x618200`),
  `K/L/M` residences (`ahm` info).

### Simulation — mapped at the top level, mostly open

* **Date** `g_date` (`0x79cca0`): `u32 round` (one in-game day, displayed as year
  1400 + round, `round & 3` = season), `u16 hour @4`, `u32 minute @6`,
  `u32 second @10`. `date_Add` `0x4ec320`, compare `0x4ec3f0`, calendar
  `0x4ec4a0`. Days start at 06:00. **Ported** (`gilde-sim`).
* **Clock**: driven by network command `0x1e` (one simulation step per game
  minute). In network games the server sends it (770 ms per game minute at
  default speed); in single player `clk_Advance` adds
  `(speed × 0.00625 + 0.5) × 100` game seconds per real second and sends the
  command to itself. **Mapped**, see [`architecture.md`](architecture.md) §5.
  `game.ini` `speed` also sets the movement time scale `0.25 + speed/160`
  (`0x5c3a70`).
* **Notices**: events with a deadline and optional message box (0x14c
  bytes at `0x726d00`, deadline `+0x52`), 167 types registered with
  `msg_RegisterHandler(type, request, execute)` (`0x52bc30`, handlers in
  `0x530000`–`0x570000`); expired ones run every game minute
  (`msg_ProcessNotices`), open boxes every frame (`msg_DispatchQueue`).
  **Mapped.** Replicated changes are network commands (0x99-byte records,
  106 types), see [`architecture.md`](architecture.md) §4.
* **Drives**: every character has 13 drives (action points, health,
  residence, money, profession, pleasure, reputation, power, education,
  righteousness, villainy, security, family — named from the AI action
  catalogue `ai_data_2.dfn`, see [`formats.md`](formats.md)) (`+0x88` + 12·n: rate, decay,
  value 0–1000) updated each game minute as `value + rate − decay·value`;
  the strongest becomes the current activity `+0x130` (`chr_UpdateDrives`).
  Saved values look as expected (rates ≈ 2–3, decays ≈ 0.0025, equilibrium
  ≈ 1000; employees have no drives). **Ported** (`gilde_sim::drives`).
* **Session loop** (`game_Session`): start of game sets 06:00 and grants starting
  money ((5 − difficulty) × 250, cheat `MO_MONEY` 750 000); per round
  `FUN_00526790` (`RundenBeginn`: per-player round-start events) and
  `FUN_00526b30`; end of round `FUN_005019d0`/`FUN_005121e0`; autosave.
  **Mapped.**
* Economy, production, markets, AI, offices/politics, family, crime, religion,
  history mode, missions: **open** (forms and entry strings located).

### Audio — open

Miles (`AIL_*`) wrappers at `0x424a00`; music tracks in `msx/`, effects in
`sfx/`, sample banks loaded by scripts.

### Networking — save/join/load mapped, transport open

Every machine runs the full simulation and saves its own copy of a network
game; a player rejoins with that local copy, and the lobby matches copies by
a checksum list in the header. The local player is the kind-6 character;
other humans are kind 7; AI characters are distributed over the machines by
`object id % player count`. Details in [`savegame.md`](savegame.md) §6
(`net_SaveGame` `0x5c55c0`, join browser `0x521130`, lobby `0x5783a0`,
`net_StartSavedGame` `0x4ff4c0`, `ls_SendGame`/`ls_ReceiveGame`).
The transport (`nt_*`, `server/server.dll`) is **open**.

## Decoded record layouts

Offsets are byte offsets into the in-memory record. Unlisted fields are
unknown; the city loader in `crates/gilde-formats/src/world.rs` names them by
offset (`fXX`).

### Character — 0x21c bytes, `g_characters` `0x79cd40` (max 768)

| Offset | Type | Meaning | Evidence |
|---|---|---|---|
| `+0x00` | u16 | index (0xffff = free) | loops over the array |
| `+0x02` | u8 | kind: 0 NPC, 1/2 employees, 3 politician, 5 AI dynasty head, 6 human on this machine, 7 human on another machine, 8 family, 15 dead | `FUN_00529910`, `net_SaveGame`, `ai_IsLocalCharacter`, `gm_KillSpieler` |
| `+0x04` | u32 | unique object id (used in messages, `FUN_004f3fc0` looks it up) | many |
| `+0x08` | u8 | life, 100 at creation, 0 = dead | `gm_CreateCharacter`, `gm_KillSpieler` |
| `+0x09` | u8 | gender (text 1161 + n; selects female text variants) | `dbg_CharacterLine`, tooltip `0x577140` |
| `+0x0a` | u16 | age | `dbg_CharacterLine` |
| `+0x0c` | u8 | religion (text 1158 + n) | `dbg_CharacterLine` |
| `+0x0d` | u8 | rank/estate (text `0x114 + n`, +10 female) | tooltip, `dbg_CharacterLine` |
| `+0x30`, `+0x40` | char[16] | first and last name | loader, savegame header |
| `+0x50` | u16 | dynasty id (`0x8000 + n`) | dynasty table |
| `+0x54` | u32 | stored −0x59a on disk; the lobby converts `[Network] Wappen` the same way | loader, `menu_NetLobby` |
| `+0x5c` | 8 × u32 | family links: spouse, ?, children (object ids, newborns appended to the mother) | tooltip "Stand", sample saves |
| `+0x164` | u8 | profession (text `0x130 + n`, +0x5f female) | tooltip "Beruf" |
| `+0x165` | u8 | employee job (text 525 + n) | `dbg_CharacterLine` |
| `+0x166` | u8 | office (text `0x253 + n`, +0x28 female) | tooltip "Ämter" |
| `+0x169` | u8 | second office | tooltip |
| `+0x18c` | u32 | family-name text id (stored −0x639 on disk) | loader |
| `+0x1cc` | u32 | flags; bits `0x200`–`0x2000` special offices, `0x100000`/`0x200000` round-start events | tooltip, round start |
| `+0x20c` | i32 | meeting/session the character attends (−1 = none) | `FUN_004d3160` |

Age (`FUN_004f4000`) and wealth (`FUN_004f8810`) are computed, not stored
directly.

### Building instance — 0xa9 bytes, `g_geb_inst` `*0x8b6134`

`+0x27` (u16): owner character slot, `0xffff` = none (players own their
residence and business; checked on sample saves).

`+0` prototype index, `+1` id, `+5` name (32 bytes, `~` = soft hyphen),
`+0x5a` u16 flags (bit `0x40` set while a timed action is pending),
`+0x65` type-specific pointer/data (see above), `+0x99` 16 bytes added in
v0x10043. Other fields: see `world.rs`.

### Dynasty — 0xac bytes × 16 at `0x802140`

Layout as serialised by `FUN_0050bd90`; semantics **open**.

### Markets — 5 × 0x2a90 at `0x802c20`, city info — 4 × 0x2f4 at `0x810100`

One entry per ware (62, or a stored count from v0x10048). Semantics **open**.

### Offices — `0x9512e0`, 37 or 42 entries of 15 bytes

Serialised by `amt_LoadAemter`; semantics **open**.

## Open questions

1. Exact animation time unit (30 fps inferred) and how the time scale applies.
2. Seasonal texture substitution for vegetation.
3. Meaning of light-set entries 1 and 3–5 and of `ambient2`; light objects.
4. Glyph metric fields `+0x11`/`+0x15` were identified as the ink box; the
   text formatting escape codes are not decoded.
5. Field semantics of the prototype tables, dynasties, markets, offices, and
   the remaining character fields (floats `+0x10`–`+0x20`, timers
   `+0x17c`–`+0x19c`, `+0x54`).
6. The navigation network (`.net` files, same container as `.cty`).

## Suggested next steps

1. Decode the remaining character and building fields through the info panels
   (`panel_infopanel_gebaeude`, character book forms), adding names to
   `re/names.tsv`.
2. Name the `cm_Ex*` command handlers and decode their payloads
   ([`commands.tsv`](commands.tsv)).
3. Port the timed message queue and the first handlers (time, building
   purchase, production) into `gilde-sim`.
4. Wire `gilde-esc` to engine host functions so location scripts run.

# File formats

Reverse-engineered from `GildeGold_TL.exe` (Die Gilde Gold 2.05, SHA-256
`cd40c06e…f6d0`) and the shipped data. Addresses refer to that executable
(image base `0x400000`). All integers are little-endian; strings are
NUL-terminated Windows-1252 unless stated otherwise.

Every parser below is exercised against every shipped file by
`gilde check` (see `crates/gilde-tools`), which also asserts that each file is
consumed exactly — no trailing bytes.

| Format | Files | Rust | Status |
|---|---|---|---|
| `.bin` archives (ZIP) + VFS | 9 | `gilde_formats::vfs` | complete |
| `.res` string tables | 116 | `gilde_formats::res` | complete |
| `.bgf` static meshes | 3024 | `gilde_formats::bgf` | complete (fast chunk skipped) |
| `.baf` vertex animations | 1251 | `gilde_formats::baf` | complete |
| `.ed3` scenes | 113 | `gilde_formats::scene` | complete, some fields unnamed |
| `.ogr` object groups | 375 | `gilde_formats::scene` | complete, some fields unnamed |

## VFS (`FUN_00429760` open, `FUN_00428be0` lookup)

* `Resources/<stem>.bin` is a ZIP archive mounted at `<stem>/`. Numbered siblings
  (`scripts.bin1`, `forms.bin0`) overlay it.
* Paths are lower-cased (`FUN_00613f56`), `\` becomes `/` (`FUN_00428000`).
* A last component `*name` searches the whole subtree under the directory
  (`FUN_00428150` with the recursive flag). The engine relies on this heavily:
  `objects/*%s.bgf`, `scenes/*stadt_%s.ed3`, `textures/*%s.bmp`.

## `.res` (`FUN_00427650`, "ReloadTextFile")

```
u32 count; u32 first_id; u32 end_id
u32 offsets[count]          // into blob
char key[count][80]         // "_KEY+N"
u8  flags[count]            // 0xFF
u32 blob_len; u8 blob[blob_len]
```

## `.bgf` meshes (`FUN_00441920` → `FUN_00443e80`)

`"BGF\0"`, then top-level chunks `{u8 tag, u32 size, payload}` ended by `0x2f`:

* `0x2e` — tagged source mesh, parsed by the table-driven reader `FUN_00443530`
  with the grammar rooted at `0x6260b8` (dump: `re/scripts/bgf_grammar.py`).
  Details in `crates/gilde-formats/src/bgf.rs`.
* `0x2d` — engine "fast load" cache (magic `0xFAB50005`, `FUN_00444e70`): the
  already-welded vertex/poly buffers. Not needed; rebuilt from `0x2e`.

Coordinates are converted from file space (Z-up) to engine space (Y-up, left
handed) on load: `(x, y, z) → (−x, z, −y)` (rotate −90° about X, then 180° about Y).

Materials: textures by stem name (`textures/*<name>.bmp`). Flag byte `0x0b`
bit 0 enables the colour key (pure black = transparent; textures are 8-bit
paletted or 24-bit BMPs), bit 1 marks double-sided faces.

## `.baf` vertex animations (`FUN_00443ed0`, `d3_LoadBinaryAnimation`)

Same `BGF\0` container, strictly ordered tags. Frames store absolute positions of
all vertices of all parts; the engine converts them to deltas against frame 0.
Optional per-frame root motion (`0x31`) and named dummies (`0x38..0x3a`). Each
`.baf` has a sibling `.ini` (`[4HEAD Studios Animation-Settings]`: `NumKeys`,
`Keys`, `LoopIn`, `LoopOut`).

## `.ed3` scenes / `.ogr` groups

Version word `0x3a6c00XX` (shipped: `0xaa`–`0xbb`). Scene header
(`FUN_0043fae0`): active camera name, ambient colour, 4–7 light-set slots, then
the object list, an optional floor and scene properties. Groups
(`FUN_00440060`) are just object lists.

Object nodes (`FUN_0043e890`, `rd_object`) form a tree: `present` byte, name,
type-specific body, `has_child` → child chain, `has_sibling` → sibling chain,
then the node's key/value properties (version > `0xa6`). Types: 0 helper,
1/4 mesh (LOD name → `objects/*<lod>.bgf`), 2 dummy, 3 camera, 5–8 lights.

Transforms: children are relative to their parent; an object's local matrix is
`T(position) · R` with `R = Rx(−rx) · Ry(−ry) · Rz(−rz)` (`FUN_004474e0` builds
`Rz·Ry·Rx` row-wise and vectors are multiplied with its transpose).

Floor (`FUN_0043f5b0`, built by `FUN_00438580`): an `n×n` (128) u8 heightfield
plus an `n×n` texture-layer index map and 8 layer texture names. Vertex
`(i, j)` = `origin + (i·cell, h·hscale, −j·cell)` with `cell = 100`,
`hscale = 5` (stored as f32 bits in `params`) and `origin` from the trailing
vec3 — verified against the building heights in all cities.

## `.cty` city / world state (`FUN_0050abf0`)

Version `0x10026`–`0x1005a` (cities: `0x10056`). Sections in order: header
(`FUN_0050b180`, incl. a 160×120 thumbnail), globals, object instances
(`FUN_0050b660`), building instances (`FUN_0050b7e0`, with per-type extras for
forests, graveyards and `K`/`L`/`M` residences), 16 dynasties (`FUN_0050bd90`),
characters (`FUN_0050c100`), 5 markets × N wares and 4 city-info records
(`FUN_0050d310`), offices (`FUN_004d8970`). City files (header flag `& 2`) then
contain a complete `.ed3` scene of the city (`FUN_0050b570` → `FUN_0043fae0` on
the same stream); savegames continue with the dynamic simulation state.
Parser: `gilde_formats::world`; all 7 shipped cities parse to the last byte.

## `.gfx` 2D graphics (`FUN_004635e0`)

Directory of 0x54-byte entries (name, offset, size, format, flags) followed by
`SHAPBANK` sprite banks: run-length encoded RGB rows with soft (alpha 0xc0) run
edges. `"NAME+N"` aliases frame N of bank `"NAME"`. Fonts are banks indexed by
character code. Parser: `gilde_formats::gfx`; all 197 banks decode.

## `.form` UI layouts (`FUN_00463d60`)

`"FRM2"`, `u32 count`, `count × 0x101c`-byte window records:

| Offset | Field |
|---|---|
| `0x000` | `i16 x, y, w, h` (800×600 reference) |
| `0x008` | `u32 style` (bit 2: draw border set at `0xf90`) |
| `0x00c` / `0x06c` | `i16 ctrl_x[48]`, `i16 ctrl_y[48]` |
| `0x0cc` | `i16 ctrl_count` |
| `0x0d0` | `i32 ctrl_type[48]` (`<0x40` image, `0x40` child-window slot, `0x41` button, `0x43` text, `0x45` slider/edit) |
| `0x190` | `char ctrl_name[48][64]` (gfx sprite / text key) |
| `0xd90` | `u8 ctrl_param[48][4]` |
| `0xe50` | `char title_key[64]` (text table key) |
| `0xe90` | `char font[64]` (gfx font, default `_FONT`) |
| `0xed0` | `i32 parent` (1-based window index, 0 = top level) |
| `0xed4` | `char image[64]` (gfx background sprite) |
| `0xf14` | `i16 margins[4]` |
| `0xf4c` | `char texture[64]` (3D texture background, e.g. parchment) |
| `0xf8c` | `f32 alpha` |
| `0xf90` | `char border[64]` (gfx border set) |

Forms describe layout only; the game code adds buttons/lists into the windows.

## Characters

* Meshes: `objects/_dynamic/character/<role>_<body>.bgf` (e.g. `arbeiter_mann`),
  with attachment dummies `d3_Head`, `d3_LeftHand`, `d3_RightHand`. Characters
  are ~70 units tall (1 unit ≈ 2.5 cm; a terrain cell of 100 units ≈ 2.5 m).
* Texture sets `.txs` (`FUN_004417e0`): `u32 0x23f209ae, u32 sets, u32 per_set`,
  then `sets × per_set` NUL-terminated names. Row 0 lists the base textures;
  further rows are variants (empty = unchanged).
* Animations: `animations/character/<body>/<category>/<name>_<body>.baf`. Vertex
  counts equal the merged mesh's vertex count. Unlike `.bgf`, `.baf` vertices and
  root motion are already in engine space. Keyframe times are 3ds Max frames
  (30 fps): the walk cycle (`gehen_mann`, frames 7–28) covers 52 units ≈ 1.3 m in
  35 frames ≈ 1.17 s. The engine tick is 14 ms (`FUN_00427bb0(14)`).
* Next-frame rule (`FUN_0042b7d0`): flag `0x200` reverse; at `loop_out` wrap to
  `loop_in` unless `0x1000` (continue to the outro) or `0x100` (hold).

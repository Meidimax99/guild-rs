# Game architecture

Reconstruction of how *Die Gilde Gold* (`GildeGold_TL.exe`, 2.05) is put
together, from the decompiled executable and `server/server.dll`. Addresses
refer to those binaries; names are in [`re/names.tsv`](../re/names.tsv) and
[`re/names_server.tsv`](../re/names_server.tsv).

## 1. Overview

```
            ┌──────────────────── GildeGold_TL.exe (every player) ─────────────────────┐
 game.ini → │ WinMain ─┬─ menu_Main (menus, lobby)                                       │
            │          └─ game_Session ── master_Step loop (one frame)                   │
            │                 │                                                          │
            │   input / UI ───┼──► cm_Request*  ──► command queue ──► nt (TCP) ──────┐   │
            │                 │                                                      │   │
            │                 │   cm_ExecCommand ◄── received commands ◄── nt ◄──────┤   │
            │                 │        │ (table 0x635030, 106 types)                 │   │
            │                 │        ▼                                             │   │
            │   world state (global arrays: characters, buildings, objects, …)      │   │
            │        ▲             ▲                                               │   │
            │   timed actions   ESC scripts, 3D scene, 2D UI, sound                │   │
            │   (msg queue, 167 types)                                             │   │
            └──────────────────────────────────────────────────────────────────────┼───┘
                                                                                   │
            ┌──────────── server/server.dll (loaded only by the host) ─────────────┴───┐
            │ srv_Main thread: TCP :7531, UDP announce :12345, lobby, world upload,    │
            │ game clock, command validation (own world copy), broadcast to all       │
            └──────────────────────────────────────────────────────────────────────────┘
```

Every machine runs the complete simulation on its own copy of the world.
Changes that must stay identical everywhere are expressed as **commands**:
a client asks (`cm_Request*`), the server orders and validates, and every
client executes the same command (`cm_Ex*`). The server also owns the game
clock. Single player uses the same command path with the network layer in an
offline loop-back mode.

## 2. Process and frame loop

* `WinMain` (`0x52b320`) reads `game.ini` (including `[Network] Server`, the
  server DLL path, default `Server\Server.dll`), initialises graphics and the
  archives, then alternates between `menu_Main` and `game_Session`.
* `master_Step` (`0x51ed60`) runs one frame. Its flag mask selects subsystems:
  `0x1` messages and commands, `0x40` 3D, `0x200` scripts, `0x20000` periodic
  logic every 7 ticks, and so on. Every screen calls it in its own loop.
* Time base: a multimedia timer (`timer_Start`, 14 ms) runs `timer_Tick`,
  which increments `g_tick` and calls registered timers when
  `g_tick % interval == 0` (`timer_Register`, `timer_Pause`).
* Session mode flags `0x71d8d0`: `0x01` single player, `0x02` hot-seat load,
  `0x04` network, `0x08` local clock enabled, `0x10` this machine hosts
  (loads the server), `0x40` loaded from a save, `0x80` tutorial/special
  (tutorial = `0x89`, command-line quick start = `0x09`).

## 3. Subsystems

| Range | Subsystem |
|---|---|
| `0x401000`–`0x410000` | ESC host commands for scripts |
| `0x410000`–`0x420000` | camera, input, sound wrappers, **network transport** (`nt_*`, `0x41d000`…) |
| `0x420000`–`0x427000` | ESC interpreter, Miles sound |
| `0x427000`–`0x42b000` | text tables, timer, VFS |
| `0x42b000`–`0x459000` | 3D engine |
| `0x459000`–`0x46c000` | 2D UI (forms, widgets) |
| `0x46c000`–`0x4b1000` | game logic: characters, buildings, AI, offices, events |
| `0x4b1000`–`0x4bc000` | **command system** (`cm_Request*`, `cm_Ex*`, `cm_ExecCommand`) |
| `0x4bc000`–`0x52c000` | game manager, save/load, cities, session loop |
| `0x52c000`–`0x5c0000` | timed action handlers, gameplay screens |
| `0x5c0000`–`0x605000` | menus, options, network menus and lobby |

## 4. Two kinds of events

**Timed actions** (`msg_*`): local records of 0x99 bytes, scheduled at a game
date and dispatched by `msg_DispatchQueue` (`0x52d410`) through 167 handler
pairs registered with `msg_RegisterHandler(type, request, execute)`. Ring of
8192 records; saved in the savegame (dynamic section 8a).

**Network commands** (`cm_*`): records of the same 0x99-byte shape that change
shared state. 106 types (`0x00`–`0x69`), full table in
[`commands.tsv`](commands.tsv). Layout as far as known:

| Offset | Meaning |
|---|---|
| `+0x00` | type |
| `+0x01` | u16 record length on the wire |
| `+0x03` | connection (sender) index |
| `+0x04` | request id (low 15 bits index the acknowledgement slots `0xf5d320`, 10 bytes each); -1 = broadcast |
| `+0x08` | sequence count (`cm_ExecCommand` reports lost commands) |
| `+0x10` | payload (sub-type byte for some types) |
| `+0x91`, `+0x95` | queue links (prev, next) |

Types 5 and 6 bracket a transaction of several records that is executed
together. The client keeps 0x8000 command records plus their acknowledgement
slots; both are saved in the savegame (dynamic sections 8b, 8c), which is why
those sections differ completely between players' copies.

## 5. Game clock

The simulation advances one step per game minute, driven by command `0x1e`.
Its client handler `cm_ExSetDate` (`0x4b6c20`) sets `g_date` if the new date
is later, then runs: the timed-action dispatch, an economy step
(`FUN_004e7dd0`, plus `FUN_004ec1b0` every 6 minutes), an update for every
character (`FUN_0046ce00`), the AI scheduler (`FUN_0046c9d0`) and two more
updates (`FUN_005a8590`, `FUN_0051a500`). Debug toggles at
`0x64d800`/`0x64d804`/`0x64d808` disable parts of it.

Who sends `0x1e`:

* **Network game: the server.** Once every client has acknowledged the last
  tick (command `0x20` sub-type 1), and its interval has passed, it adds one
  minute to its date (`date_Add`) and broadcasts it. Interval per game minute
  from the speed setting (command `0x20` sub-type `0x12`):

  | Speed | 0 | 1 | 2 (default) | 3 | 4 |
  |---|---|---|---|---|---|
  | ms per game minute | 1155 | 937 | 770 | 577 | 385 |

  When a tick reaches 23:00 the server sends `0x20` sub-type 2 (end of
  round; flagged 1 for the first connection), increments the round, sets
  06:00 and waits until every client has acknowledged again before the next
  tick. Within a day ticks follow at the fixed interval without waiting.
  A round (06:00–23:00 = 1020 minutes) lasts about 13.1 real minutes at the
  default speed.
* **Single player: the client itself.** `clk_Advance` (`0x51f960`), a timer
  every 71 ticks (≈ 1 s), adds `(speed × 0.00625 + 0.5) × 100` game seconds
  to a local date while the hour is below 23. `master_Step` sends that date
  with `cm_RequestSetDate` (`0x4b30c0`) when the network layer is offline;
  the command loops back through the same queue. `speed` is `game.ini`
  `[…] speed` or the options slider (steps of 40; fast-forward sets 750).
  At speed 40 a round takes about 13.6 real minutes.

## 6. Network architecture

### Topology

Client/server over TCP, with the server running **inside the host's game
process**: `game_Session` on the host (flag `0x10`) loads `server.dll`, calls
its export `Init(&config)` and waits 2.5 s; every player, the host included,
then connects with `nt_Init(address, port)` to TCP port 7531
(`game.ini` `[Network] Port` on clients, `[Config] Port` for the server).
`Exit()` stops the server thread. Single player calls `nt_Init(0, 0)`: no
socket, commands loop back locally.

### Server thread (`srv_Main`, `0x10008d20`)

1. **Lobby.** Every 15 ms: accept connections (8 slots of ~2.4 MB, mostly
   record buffers), read and answer records. Every 3 s the config block is
   broadcast to UDP port 12345 so clients on the LAN can list the game. The
   phase ends when the expected number of players (config block; the host
   sets it from the save header's human count when loading) are connected
   and ready.
2. **Client setup.** Each connection receives command `4` (role) and command
   `0x20` sub-type 7 with its **client index** — the index the clients use to
   split AI work (`object id % player count == client index`).
3. **World distribution** (new game only). The host uploads the world through
   the server (commands `8` size, `9` 0x80-byte chunks; `ls_SendGame` /
   `ls_ReceiveGame`), the server keeps a copy (`ls_LoadGame`) and sends it to
   every client the same way. For a saved game every machine loads its own
   local copy instead (see [`savegame.md`](savegame.md) §6), and the host
   streams its `.SRV` to the server only.
4. **Game loop.** Read records into per-connection inbound queues, execute
   them, write outbound queues, run the game clock (§5).

### Connection state machine (`srv_ProcessInput`)

| Flag | State | Records |
|---|---|---|
| `0x001` | connected | |
| `0x002` | handshake | type 3 → reply with the role (1 = the host's own connection, `0x400`; 2 = remote) |
| `0x200` | lobby | type `0x5f` → `lb_ExLobbyServerCommand` |
| `0x008` | world upload | types 8 / 9 into `srv_LoadBuf` |
| none | playing | queued for execution |
| `0x020` | acknowledged the current clock tick | |
| `0x040` | has a record to relay | |

Framing: 3-byte header (type, u16 total length) and the body; a connection
silent for 5 minutes is dropped.

### Authority

`srv_ExecuteCommands` runs each inbound command through the server's own
handler table (`0x1002a530`, 106 entries). About 45 types are validated
against the server's copy of the world — economy (selling objects `0x11`,
building construction `0x4c`, upgrades `0x3a`, …), crimes (`0x22`) and
others; the rest are accepted unchanged. An accepted command is echoed to
its sender and broadcast to all other clients, which execute it in the order
the server sent it. The server therefore contains a reduced copy of the game
logic: prototypes (`A_Geb.dat`, `A_Obj.dat`), buildings and rooms, objects,
characters (`gm_KillSpieler`, `gm_CreateCharacter`), graveyards, plant maps,
and a save loader.

### What is replicated and what is not

Replicated: every command (economy, building, character actions, offices,
crimes, cut-scenes, chat/system messages) and the game clock. Local: the
3D/UI state, timed actions created locally, transient NPCs, pending message
boxes of the local player, the command log. Copies of the world therefore
drift slightly between machines (measured on six players' saves, see
[`savegame.md`](savegame.md) §6.1); there is no state synchronisation beyond
the commands. A player who leaves ends the game for everyone
(`game_Session`: "Ein Spieler hat die Partie verlassen…").

## 7. Open questions

* Payload layouts of most command types; names of the unnamed `cm_Ex*`
  handlers (see `commands.tsv`).
* The UDP announcement format and the lobby protocol (`lb_*`).
* How hot-seat (`0x02`) rotates players over one machine.
* About 40 functions created by the function search carry switch-case names
  (`caseD_*`): the containing switch statements were split; their parents
  should be re-joined in Ghidra.

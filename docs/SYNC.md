# Sync between slot-mini PC and the Miyoo

This is the specification of `crates/slot-mini-sync`, the library both apps
use. Brief 3 (`docs/briefs/`) embeds its server in slot-mini PC; brief 4
embeds its client in slot-mini on the Miyoo. Everything here is implemented
and tested in the crate; nothing here is shipped behaviour yet.

Words used throughout:

- **Hub**: the PC. It runs the server and holds the record of what was last
  synced with each paired device (the **base**).
- **Client**: the Miyoo. It starts every sync.
- **Side**: either of them.

Contents: [What syncs](#what-syncs) · [Items and keys](#items-and-keys) ·
[Layouts](#layouts) · [Names](#names) · [Manifests and hashing](#manifests-and-hashing) ·
[Scopes](#scopes) · [The planner](#the-planner) · [Text merges](#text-merges) ·
[Save rules](#save-rules) · [Remove everywhere](#remove-everywhere) ·
[Space and busy games](#space-and-busy-games) · [The exchange](#the-exchange) ·
[Protocol](#protocol) · [Pairing](#pairing) · [Authentication](#authentication) ·
[Discovery and poke](#discovery-and-poke) · [Failure and resume](#failure-and-resume) ·
[Stored state](#stored-state) · [Crate API](#crate-api) ·
[What each UI must show](#what-each-ui-must-show) · [Limits and open points](#limits-and-open-points)

## What syncs

| Kind | Files | `Auto` | `Full` |
| --- | --- | --- | --- |
| ROMs | `Roms/{GBA,GBC,GB}/`, and one subfolder level | no | yes |
| Labels | `Labels/{GBA,GBC,GB}/<stem>.png` | yes | yes |
| Art | `Roms/<sys>/Imgs/<stem>.png` | yes | yes |
| Shells | `Labels/cart_shell.txt`, per game line | yes | yes |
| Favourites | each side's `favorites.txt`, per entry | yes | yes |
| In-game saves | `<stem>.srm` and `<stem>.rtc` per game | yes | yes |

Never synced: save states, Recents, settings, theme, wallpapers, the per-game
core choice (`.game_config`), caches, and anything else on either side.

## Items and keys

Every synced thing is an **item** with a logical key that does not depend on
where a side keeps it. `<sys>` is `GBA`, `GBC` or `GB`. Keys use `/`.

| Key | What |
| --- | --- |
| `rom:<sys>/<relpath>` | a ROM; `<relpath>` is `<file>` or `<subfolder>/<file>` |
| `label:<sys>/<stem>` | a cart label |
| `art:<sys>/<stem>` | Onion's box art |
| `save:<sys>/<stem>.srm`, `save:<sys>/<stem>.rtc` | a game's in-game save and its clock file |
| `shell:<stem>` | one line of `cart_shell.txt` (a **text item**) |
| `fav:<sys>/<stem>` | one favourite (a **text item**) |

`<stem>` is the ROM's file name without its last extension (`file_stem`),
which is how slot-mini matches labels and how RetroArch names saves. A
**game** is `<sys>/<stem>`: it owns every ROM file of that stem in that
system, its label, art, saves, shell line and favourite.

File items (ROMs, labels, art, saves) are compared by SHA-256. Text items
are compared by value: a shell line's value (`notched 2f5cc0 glitter`, or
`auto`), and for a favourite simply whether it is there.

## Layouts

A `Layout` maps keys to files on one side and back. Both sides use the same
tree shape under a root (the SD card on the Miyoo, the library folder on the
PC); only saves and favourites differ.

| | Miyoo (`Layout::device`) | PC (`Layout::pc`) |
| --- | --- | --- |
| Root | the SD card (`/mnt/SDCARD`) | the PC library folder |
| ROMs, labels, art, shells | as in the table above, under the root | the same, under the root |
| Favourites | `App/slot-mini/data/favorites.txt` | `App/slot-mini-pc/data/favorites.txt` |
| Saves | `Saves/CurrentProfile/saves/<core>/<stem>.srm` | `<saves>/<core>/<stem>.srm` |
| Auto states | `Saves/CurrentProfile/states/<core>/<stem>.state.auto` | `<states>/<core>/<stem>.state.auto` |
| Which core | Onion's rules (below) | asked of the PC app (`CoreChoice`) |
| Sync's own files | `App/slot-mini/data/sync/` | `App/slot-mini-pc/data/sync/` |
| Removed saves | `Saves/slot-mini-removed/` | `Saves/slot-mini-removed/` under the root |
| Names it can store | FAT rules | Windows rules on Windows, anything elsewhere |

`<core>` is the folder RetroArch sorts a core's files into: `mGBA`, `gpSP`
or `Gambatte` (`slot_mini_store::saves::sort_dir`).

**Which ROM files count** is `slot_mini_store::rom_files`, the scan slot-mini
already uses: the system's extensions in any case (`.GBA` included), the
folder and one subfolder level, skipping hidden entries and Onion's `Imgs`,
`Images` and `media`.

**The core on the Miyoo** is what slot-mini already works out
(`slot_mini_store::saves::device_core`, moved from slot-mini's `cores.rs`
unchanged): the game's `.game_config/<name>.cfg` override, else the default
its `Emu/<sys>/launch.sh` names (`default_core=`, else the first
`.retroarch/cores/<name>.so`), else mGBA. This is the same on Onion 4.3.1-1
and the development build. A save with no ROM on the Miyoo cannot be placed
(see "Saves need their game" below).

**The core on the PC** comes from the app through the `CoreChoice` trait:
`fn core(&self, platform: Platform, stem: &str) -> Core`. Brief 2's core
picker implements it. The PC's `<saves>` and `<states>` folders are
RetroArch's `savefile_directory` and `savestate_directory`; the PC app must
run RetroArch with `sort_savefiles_enable` and `sort_savestates_enable` on,
as Onion does, so that the per-core folders exist on both sides.

Saves belong to games, not cores: `save:GBA/Golden Sun.srm` is
`saves/gpSP/Golden Sun.srm` on a Miyoo that runs the game on gpSP and
`saves/mGBA/Golden Sun.srm` on a PC that runs it on mGBA. The bytes are the
same format in both.

**Saves need their game.** A save is sent and received only for a game
whose ROM is on both sides, or arrives in the same sync. Without the ROM a
side cannot tell which core folder the save belongs in, and a save folder
shared by two systems (Gambatte holds GB and GBC) cannot be told apart. The
save waits where it is and moves with a later sync, once the ROM is there.

A side also lists the saves it holds for games it does **not** have (a ROM
deleted here, its save left behind): every `<stem>.srm` or `.rtc` in a core
folder whose stem is no game's here, under each system whose saves would go
in that folder by default. That is what lets a save go out in the same sync
that brings its ROM back; on its own such a save moves nothing.

## Names

- ROM names travel **verbatim**: a ROM created by sync has exactly the
  name and subfolder the other side gave it.
- A label, art file, save, shell line or favourite belongs to a game, and
  is found by the game's name. One that sync creates is named after the
  **receiving side's spelling of the game** (the stem of its ROM there),
  which is the other side's spelling whenever the ROM arrived by sync. A
  side without the game (a label that arrives before its ROM in an `Auto`
  sync) uses the name it was sent.
- Names are **compared** by their Unicode NFC form, lower-cased
  (`names::fold`), everywhere: keys from the two sides, the base, text
  entries. So `Pokémon.gba` in NFD on one side and NFC on the other is one
  item, and so are `GAME.gba` and `game.gba`, as they would be on FAT, exFAT
  and Windows. When both sides already hold an item under different
  spellings, each keeps its own spelling.
- Two files on one side whose names fold to the same key (possible only on
  a case-sensitive PC file system) are a **duplicate**: the first in byte
  order is the item, the other is reported and left alone.
- A name the receiving side **cannot store** is reported and the item is
  skipped; the sync carries on. FAT and Windows rules (`NameRules::Fat`,
  `NameRules::Windows`): none of `< > : " / \ | ? *` or control characters,
  no trailing dot or space, not a reserved device name (`CON`, `NUL`,
  `COM1`…), at most 255 UTF-16 units per name. The Miyoo's card is always
  treated as FAT. The check covers the file the item becomes on that side
  (for a save, `<stem>.srm`).

Onion's own launch restriction (decision 16 in `DECISIONS.md`: no `"`,
`` ` ``, `\`, `:` or control characters) is a subset of the FAT rule except
for the backtick, which syncs fine and is refused at launch as before.

## Manifests and hashing

A **manifest** is one side's list of items for a scope:

- for each file item: its key (in that side's spelling), size, SHA-256 and,
  for saves, a **last played** hint: the save file's modification time in
  Unix seconds;
- the games whose ROM is present (always, whatever the scope), so the
  planner can apply "saves need their game" without hashing ROMs;
- the saves of games that are not present (see [Layouts](#layouts));
- the shell lines and the favourites.

Changes are detected by hash against the base, never by time. The hint is
used only to pick the winner of a save conflict. Because the Miyoo's clock
is unreliable, the client sends its own "now" with the manifest and the hub
compares **ages** (`now - mtime` on each side), which cancels a clock that
is simply set wrong.

**The hash cache.** Each side keeps `hashes.tsv` in its sync folder: path,
size, modification time (nanoseconds) and hash. A file is re-hashed when its
size or time differ, or when its time is within 3 seconds of when it was
hashed (FAT stamps in 2-second steps, so a quick second write could go
unseen). **Saves are never cached**: they are small, keep their size when
they change, and matter most. The first `Full` sync of a big library hashes
every ROM, which is slow on the Miyoo; the scan reports progress in bytes.

## Scopes

Every sync names one scope.

| Scope | Covers | Used for |
| --- | --- | --- |
| `Full` | every item | manual sync, on either side |
| `Auto` | everything except ROMs | automatic syncs |
| `Game(<sys>/<stem>)` | that game's saves, label and art | just before a launch |

Text items and Remove everywhere are handled in `Full` and `Auto`; `Game`
does neither.

## The planner

`plan::plan` is a pure function. For each file item in scope it compares
three things: the client's hash **C**, the hub's hash **H**, and the base
**B** (the hash last synced with this device). Each is a hash or absent
(`–`). An empty save (all `00` or all `ff`) is absent.

| # | B | C | H | Result |
| --- | --- | --- | --- | --- |
| 1 | – | – | – | nothing (the item does not exist) |
| 2 | – | x | – | **push** (new on the client) |
| 3 | – | – | x | **pull** (new on the hub) |
| 4 | – | x | x | **record** B = x (already the same) |
| 5 | – | x | y | **conflict** (never synced, different) |
| 6 | b | b | b | nothing |
| 7 | b | b | y | **pull** (changed on the hub) |
| 8 | b | x | b | **push** (changed on the client) |
| 9 | b | x | x | **record** B = x (changed the same way on both) |
| 10 | b | x | y | **conflict** (changed on both) |
| 11 | b | – | b | **pull** (deleted on the client; deletions don't propagate) |
| 12 | b | – | y | **pull** (deleted on the client, changed on the hub) |
| 13 | b | b | – | **push** (deleted on the hub; deletions don't propagate) |
| 14 | b | x | – | **push** (deleted on the hub, changed on the client) |
| 15 | b | – | – | **forget** B (deleted on both) |

`x` and `y` are hashes different from `b` and from each other. Rows 4 and 9
make seeding by copying an SD card idempotent: nothing moves, the base is
written. Rows 11 to 14 are decision 5: a deletion never deletes on the other
side, so the file comes back. To remove a game, use Remove everywhere.

What a **conflict** does depends on the kind:

| Kind | Conflict |
| --- | --- |
| Save | keep both: see [Save rules](#save-rules) |
| Label, art, ROM | the hub's copy wins: a pull, and nothing extra is kept |

Before the table is applied, in this order:

1. **Remove everywhere.** A game with a tombstone this device has not
   applied becomes one **remove** on the client (see below), which covers
   everything the client holds of it. Whatever the hub holds of that game
   was put there after the removal, and is planned as new: it is pulled
   after the remove.
2. **Busy.** Every item of a game the hub has marked busy is skipped and
   the game is listed.
3. **Saves need their game.** A save whose game is missing on either side,
   and is not arriving in this plan, is skipped silently.
4. **Names.** A pull whose name the client cannot store, or a push the hub
   cannot store, is skipped and listed. Duplicates are listed.
5. **Space** (pulls of ROMs only): see [Space and busy games](#space-and-busy-games).

Plan order: removes, then labels, art and saves in key order, then ROMs in
key order. The client carries the actions out in that order, whichever way
each file goes. An interruption or a full card therefore costs ROMs first,
saves last.

## Text merges

Text items use the same three-way comparison per entry, with two
differences: **removals propagate** (they are edits to a shared file, not
deleted files), and the hub wins a conflict.

Shell lines, per stem (value = the text after `=`, `–` = no line). Only
lines slot-mini can read are entries (a `ShellChoice`, or `auto`);
unreadable lines stay untouched in each side's file.

| B | C | H | Result on both sides |
| --- | --- | --- | --- |
| any | v | v | v (record) |
| – | v | – | v |
| – | – | v | v |
| – | v | w | **w** (the PC wins) |
| b | b | w or – | the hub's (changed or removed on the PC) |
| b | v or – | b | the client's (changed or removed on the Miyoo) |
| b | v or – | w or – (≠ C) | the hub's (the PC wins) |
| b | – | – | – (forget) |

Favourites, per entry (`<sys>/<stem>`), as a set:

| B | C | H | Result |
| --- | --- | --- | --- |
| – | yes | – | added on the hub |
| – | – | yes | added on the client |
| – | yes | yes | record |
| yes | – | yes | removed on the hub |
| yes | yes | – | removed on the client |
| yes | – | – | forget |

A game with a tombstone the device has not applied loses its favourite on
both sides, and its shell line unless a game of that stem (in another
system) is still on either side.

Entries are compared folded, like every name. A line or favourite added to
a side is written under that side's spelling of its game when it has the
game, and replaces lines that differ from it only by case.

Each side keeps its own order; a new favourite goes to the end, as a
toggle does. The files are read and written with slot-mini's own code
(`slot_mini_store::cart_shell`, `slot_mini_store::Favourites`), so comments
and unreadable lines in `cart_shell.txt` survive.

The hub computes the merge when it plans, writes its own files at once, and
sends the client its edits. The base for text entries is written only when
the client commits them, so a client that fails halfway is merged again
next time with the same result.

## Save rules

1. **Empty is absent.** A save that is all `00` or all `ff` (or zero
   bytes) is not listed and is simply replaced when a save arrives.
2. **A save that arrives** (pull on the client, push on the hub) lands
   through one function, `slot_mini_store::saves::receive_save`, which is
   also what slot-mini's core picker (`cores::carry_save`) uses:
   - a different, non-empty save already there is renamed
     `<stem>.srm.bak_<YYYYmmddHHMMSS>` (never over an earlier backup:
     `_1`, `_2`… are added);
   - the new save is renamed into place from a verified temporary file;
   - that side's auto state for the game's core,
     `states/<core>/<stem>.state.auto`, is renamed the same way.
     RetroArch then starts the game from its title screen and the player
     continues from the in-game save. Nothing is deleted.

   A save that arrives where the same bytes already are is not landed
   again: nothing is set aside and the auto state stays (it belongs to
   that very save). This is what happens to the second of a GB and a GBC
   game of one stem, which share a Gambatte file.
3. **A conflict keeps both.** The winner is the copy played more recently
   (smaller age; a tie or a missing hint goes to the PC). The conflict copy
   always lives on the **hub**, beside the hub's save, as
   `<stem>.srm.conflict-<who>-<stamp>`, where `<who>` is the name of the
   side it came from, reduced to `a-z0-9-`. RetroArch never loads that name.
   - **The client wins:** the hub renames its own save to the conflict
     copy, then receives the client's save as in rule 2.
   - **The hub wins:** the client uploads its save, which the hub stores
     as the conflict copy; then the client receives the hub's save as in
     rule 2 (so its own copy also stays on the card as `.bak_`).

   The hub records the conflict (key, device, stamp, file, hash, the
   side the copy came from, and when the copy and the winning save were
   last played, both on the hub's clock; a swap exchanges the two); the
   same copy is recorded once, however often an interrupted sync repeats. The PC app
   lists them and can **swap** one: the conflict copy becomes the save
   (through rule 2, so the PC's auto state is set aside) and the save
   becomes the conflict copy. The next sync then sees a save changed on
   the hub and sends it to the Miyoo. A conflict can also be **dismissed**:
   the copy moves to the removed-saves folder and the record goes.
4. `.rtc` files follow the same rules as `.srm`, item by item; a conflict
   on either uses the same hint, so both go the same way.

Consequence of rule 2: each time a save moves, the receiving side keeps the
save it replaced as a `.bak_`. That is deliberate (no save is ever lost,
whatever the clocks said), and it is what Onion's own save transfer does.
The files are small; the PC app may offer to tidy them.

## Remove everywhere

Only the PC can do it (`Hub::remove_everywhere(game)`). It:

1. removes the game from the PC: every ROM file of that stem in that
   system, its label, art, shell line and favourite; its saves (`.srm`,
   `.rtc`, and any conflict copies) move to
   `Saves/slot-mini-removed/<sys>/<name>.<stamp>` under the library root;
2. writes a **tombstone** for the game and drops its conflict records.

At each `Full` or `Auto` sync, every tombstone the device has not
acknowledged becomes a `remove` action: the client does step 1 on the card
(saves to `Saves/slot-mini-removed/` there) and commits, and the hub notes
the acknowledgement and forgets the game's base entries. A device paired
later has acknowledged nothing, so it applies every tombstone.

The shell line is removed only if no other system has a game of that stem
(shell lines are keyed by stem alone).

A tombstone is **lifted** when the game returns: if the PC has the ROM
again when a plan is made, or a device that already acknowledged the
tombstone pushes it. From then on it is an ordinary game.

## Space and busy games

- The client sends the free space of its card with the manifest. The
  planner adds up the pulls in plan order and keeps 16 MB spare. A ROM that
  does not fit is left out and listed (key and size); smaller ROMs after it
  are still tried. Labels, art and saves are always planned; if the card
  is truly full their transfer fails and is reported as a failed item.
- `Hub::set_busy(Some(game))` marks the game running on the PC. Plans skip
  every item of that game and list it. `set_busy(None)` clears it. A sync
  already in flight is not interrupted: the PC app should mark the game
  before starting RetroArch and should not start it while a sync is
  transferring that game. `Hub::is_syncing_game(&game)` says whether the
  sync in flight has the game in its plan.

## The exchange

```
client                                   hub
  |-- GET  /info ------------------------->|   hub time, protocol, name
  |-- POST /sync/hello ------------------->|   opens a session
  |      (scans and hashes its side)       |
  |-- POST /sync/plan  (manifest) -------->|   scans its side, plans,
  |<------------- plan --------------------|   merges text, records rows 4/9
  |   for each action, in order:           |
  |-- PUT  /sync/file?s=..&a=<id> -------->|   push: verified, landed, base written
  |-- GET  /sync/file?s=..&a=<id> -------->|   pull: client verifies and lands,
  |-- POST /sync/commit {action} --------->|         then base written
  |-- POST /sync/commit {text} ----------->|   after applying the text edits
  |-- POST /sync/done  (report) ---------->|   session closed, report kept
```

- The hub runs **one session at a time**. A hello from the same device
  replaces its old session. A hello from another device while a session
  has been active in the last 30 seconds gets `busy`.
- Files are streamed in both directions in 64 KB pieces; nothing is held
  whole in memory. Each is written to a temporary file beside its
  destination (`.<name>.<pid>.<seq>.tmp`), hashed while it is written,
  compared with the hash in the plan, fsynced and renamed
  (`slot_mini_store::AtomicFile`). A mismatch deletes the temporary file
  and fails that item only.
- The hub serves and accepts only the files its plan named, by action id,
  with the hashes it planned. A file that changed on the sender since the
  manifest fails its hash check and is picked up by the next sync.
- The base is written **per item**: on the hub as soon as a push has
  landed, and when the client commits a pull.

## Protocol

HTTP/1.1. Control messages are JSON (`application/json`, UTF-8); files are
raw bodies with `Content-Length`. `PROTOCOL_VERSION` is **1**. Default
ports: **TCP 24735** for the hub's server, **UDP 24735** for discovery (on
the hub) and **UDP 24736** for pokes (on the Miyoo). All three can be
changed in code for tests.

Every request carries `X-Slot-Protocol: <n>`. A hub that gets another
version answers `426` with error code `protocol` and its own version. Both
sides turn that into `SyncError::Protocol { hub, client }`, whose message
names the app to update: the side with the lower number ("Update slot-mini
on the Miyoo to sync with this PC" / "Update slot-mini PC to sync with this
Miyoo"). `/info` and discovery replies carry the version too, so the
client can say so before it tries.

Errors are `{"error": {"code": "<code>", "message": "<text>"}}`:

| Status | Code | Meaning |
| --- | --- | --- |
| 400 | `bad-request` | malformed request, or an action the plan does not have |
| 401 | `unknown-device` | not paired (or unpaired on the PC): the client forgets the hub |
| 401 | `bad-auth` | the signature does not verify |
| 401 | `stale` | timestamp outside the window; the reply carries `now` |
| 401 | `replay` | nonce already used |
| 403 | `not-pairing` | the PC is not in pairing mode |
| 404 | `not-found` | no such endpoint |
| 409 | `busy` | syncing with another device |
| 409 | `no-session` | the session is gone (hub restarted, or replaced) |
| 422 | `hash-mismatch` | an uploaded file is not what the plan said |
| 426 | `protocol` | protocol versions differ; carries `protocol` |
| 500 | `io` | the hub could not read or write a file |

### Endpoints

`GET /info` (no authentication)
→ `{"name", "id", "protocol", "now", "pairing"}`. `now` is the hub's clock
in Unix seconds; `pairing` says whether the PC is in pairing mode.

`POST /sync/hello` `{"scope", "device_name"}`
→ `{"session", "hub_name"}`. `scope` is `"full"`, `"auto"` or
`"game:<sys>/<stem>"`.

`POST /sync/plan`
```json
{"session": "…", "now": 1700000000, "free": 123456789, "names": "fat",
 "games": ["GBA/Golden Sun (USA)"],
 "items": [{"key": "save:GBA/Golden Sun (USA).srm", "size": 65536,
            "hash": "<64 hex>", "played": 1699990000}],
 "shells": [["Tetris (World)", "notched 2f5cc0 glitter"]],
 "favourites": ["gb/Tetris (World)"]}
```
→
```json
{"actions": [
   {"id": 0, "op": "remove", "game": "GBA/Old Game"},
   {"id": 1, "op": "pull", "key": "label:GBA/X", "size": 1, "hash": "…", "new": true},
   {"id": 2, "op": "push", "key": "save:GBA/X.srm", "size": 1, "hash": "…", "new": false},
   {"id": 3, "op": "conflict", "key": "save:GBA/Y.srm", "winner": "hub",
    "size": 1, "hash": "…", "mine": "…"}],
 "text": {"shells": {"set": [["stem", "value"]], "remove": ["stem"]},
          "favourites": {"add": ["gba/Stem"], "remove": []}},
 "notes": {"didnt_fit": [{"key": "rom:GBA/Big.gba", "size": 33554432}],
           "busy": ["GBA/Running Game"],
           "unstorable": [{"key": "rom:GBA/What?.gba", "side": "client"}],
           "duplicates": [{"key": "rom:GBA/game.gba", "side": "hub"}]}}
```
`key` is the spelling the client must use: its own for a push or an update,
the hub's for a new file (which the client stores under its own spelling of
the game when it is a label, art file or save: see [Names](#names)). For a pull, `size` and `hash` are the hub's file;
for a push, the client's. For a conflict they are the hub's save, and
`mine` is the client's hash. `new` says whether the receiving side had the
item (added or updated, for the report).

`PUT /sync/file?s=<session>&a=<id>` with the file as the body
→ `{}`. For a `push`, and for a `conflict` (whichever side wins: the hub
either makes it the save or keeps it as the conflict copy).

`GET /sync/file?s=<session>&a=<id>`
→ the file. For a `pull`, and for a `conflict` the hub wins.

`POST /sync/commit` `{"session", "action": <id>}` or `{"session", "text": true}`
→ `{}`. The client has landed a pull (or the hub-wins half of a conflict),
finished a remove, or applied the text edits.

`POST /sync/done` `{"session", "report": {…}}` → `{}`. The report is the
[`Report`](#what-each-ui-must-show) as JSON; the hub keeps it for the PC's
UI. A session with no `done` simply expires.

`POST /unpair` `{}` → `{}`. The hub forgets the device.

Pairing endpoints are under [Pairing](#pairing).

## Pairing

Pairing gives both sides a 32-byte key. It is started on the Miyoo and
needs the PC in pairing mode (`Hub::start_pairing()`, the PC's "Pair a
device" screen). X25519 is `ring::agreement`; the code is protected against
a guessing attacker by having the client commit to its key first.

1. The client makes an ephemeral X25519 key pair and a random 16-byte
   nonce, and sends
   `POST /pair/start {"device_id", "device_name", "commit"}` where
   `commit = SHA-256(client_public ‖ nonce)`.
   → `{"pairing", "hub_id", "hub_name", "public"}`: the hub's ephemeral
   public key and an id for this attempt. `403 not-pairing` if the PC is
   not in pairing mode. A new start replaces an unfinished attempt.
2. `POST /pair/reveal {"pairing", "public", "nonce"}` → `{}`. The hub
   checks the commitment (`400` if it does not match).
3. Both sides compute `shared = X25519(own private, other public)` and,
   with HKDF-SHA256 (salt `slot-mini-sync pair v1`, info
   `client_public ‖ hub_public ‖ device_id ‖ hub_id`), 36 bytes: the first
   32 are the key, the last 4 as a big-endian number modulo 1 000 000 are
   the **6-digit code**. Both screens show the code.
4. The player confirms on both. The client confirms with
   `POST /pair/confirm {"pairing"}`, signed with the new key (see
   Authentication; `X-Slot-Device` is the device id). The reply is
   `{"status": "waiting"}` until the PC user has confirmed too
   (`Hub::confirm_pairing()`), then `{"status": "paired"}`; the client asks
   once a second while its pairing screen is open. `{"status": "rejected"}`
   if the PC user cancelled or the attempt expired (2 minutes).
5. On `paired` both sides store the key: the hub under the device's id
   with its name, the client beside the hub's id, name and address.

Either side can unpair. The Miyoo calls `/unpair` (best effort) and deletes
its record; the PC deletes the device (`Hub::unpair(device)`), after which
the Miyoo's next request gets `unknown-device` and it deletes its record
too. Unpairing on the hub also drops that device's base; pairing again
starts from "no base", which moves nothing that is already the same.

A device is paired with one hub at a time. A hub can have several devices.

## Authentication

Every request except `/info`, `/pair/start` and `/pair/reveal` is signed:

```
X-Slot-Device: <device id>
X-Slot-Time:   <hub time, Unix seconds>
X-Slot-Nonce:  <32 hex: 16 random bytes>
X-Slot-Body:   <SHA-256 of the body, hex; of nothing for GET>
X-Slot-Auth:   hex(HMAC-SHA256(key, message))

message = "slot-mini-sync/1\n" METHOD "\n" path-and-query "\n" time "\n" nonce "\n" body-hash
```

- The time is the **hub's** clock: the client reads `now` from `/info` at
  the start of every sync and counts from there, because its own clock
  cannot be trusted. The hub refuses times more than **120 seconds** from
  its own (`stale`), and any nonce it has already seen from that device
  inside the window (`replay`).
- For an upload, `X-Slot-Body` is the file's hash. The hub checks the
  signature before reading the body, and the body against the hash while
  it streams to the temporary file.
- Every reply to a signed request is signed too:
  `X-Slot-Auth: hex(HMAC(key, "slot-mini-sync/1 reply\n" nonce "\n" status "\n" body-hash))`
  with `X-Slot-Body`. The client checks it; for a downloaded file the body
  hash is the file hash it verifies anyway.

**There is no transport encryption.** Sync is LAN-only, and everything on
the wire (ROMs, saves, names) can be read by anyone on that network;
only tampering and impersonation are prevented. The keys are stored in
plain files on the card and on the PC. Revisit both with the off-LAN
strategy: that work needs TLS or an equivalent, and should not reuse this
scheme over the internet.

## Discovery and poke

Datagrams are single JSON objects, at most 512 bytes.

**Discovery.** The client broadcasts
`{"t": "find", "protocol": 1}` to UDP 24735 (255.255.255.255, and it may
also try the hub's last known address). Every hub answers the sender with
`{"t": "hub", "id", "name", "port", "protocol"}`. Replies are not signed:
they only say where to connect, and every request after that is. A paired
client picks the reply whose `id` is its hub.

**Poke.** "Sync now" on the PC sends, to each paired device's last address
on UDP 24736:

```json
{"t": "poke", "hub": "<hub id>", "time": 1700000000123, "port": 24735,
 "mac": "<hex HMAC-SHA256(key, \"slot-mini-sync/1 poke\\n\" hub \"\\n\" time \"\\n\" port)>"}
```

`time` is the hub's clock in milliseconds. The client acts on a poke only
if the hub id is its hub, the MAC verifies, and `time` is greater than any
poke it has accepted and than the hub time of its last sync (kept in its
record), so a recorded poke cannot be replayed. A poke carries no
instruction: the client answers by starting its own `Full` sync with the
hub at the sender's address and the given port. Anything else is dropped
silently.

The Miyoo waits for pokes with a blocking `recv` on a worker thread
(`Poked::wait`), which costs no CPU while idle.

The hub learns a device's address from each signed request it makes.

## Failure and resume

- **Interrupted transfer** (Wi-Fi drops, a side is switched off): the
  temporary file is deleted, or swept at the start of the next sync
  (`sweep_temps`, which removes other runs' `.<name>.<pid>.<seq>.tmp`
  files in the folders sync writes to). Items already committed keep their
  base. The next sync plans again and carries on with what is left; files
  restart from their beginning.
- **Commit lost** (a pull landed, the commit did not arrive): next time
  both sides have the same hash and row 4 or 9 records it.
- **Hub restarted mid-sync:** the session is gone (`no-session`); the
  client reports the sync as interrupted and the next one resumes.
- **A file changed while syncing:** its hash check fails, the item is
  reported as failed, and the next sync sees the new content.
- **Clock wrong on the Miyoo:** no effect except on which copy of a
  conflicting save is "the save"; the other copy is kept either way.
- **Cancelled** (`Cancel`): checked between pieces of a transfer and
  between actions; behaves as an interruption.

A sync never leaves a half-written file under a real name, and never
deletes a save, a state or a backup.

## Stored state

Hub, in `App/slot-mini-pc/data/sync/`:

| File | Holds |
| --- | --- |
| `hub.txt` | `id`, `name` (slot's `key = value` format) |
| `devices/<id>/device.txt` | `name`, `key` (hex), `address`, `paired`, `last_sync` |
| `devices/<id>/base.tsv` | one line per item: folded key, tab, hash or value; appended to during a sync, rewritten at its end |
| `devices/<id>/acked.txt` | tombstones this device has applied |
| `tombstones.txt` | one game per line |
| `conflicts.tsv` | id, key, device name, stamp, the copy's path under the save folder, its hash |
| `last_report.json` | the last sync's report |
| `hashes.tsv` | the hash cache |

Client, in `App/slot-mini/data/sync/`:

| File | Holds |
| --- | --- |
| `client.txt` | `device_id`, `device_name`, and once paired `hub_id`, `hub_name`, `key`, `address`, `hub_time` |
| `hashes.tsv` | the hash cache |

Nothing else is written outside the synced files themselves, the `.bak_`
and conflict copies beside saves, and `Saves/slot-mini-removed/`.

## Crate API

`slot-mini-sync` has two features, both off by default:

- `client` adds `HttpLink` (ureq, no TLS) for the Miyoo;
- `server` adds `Server` (tiny_http) for the PC.

Without either it still holds the model, the planner, both engines and an
in-process `DirectLink`, which is what the property tests use.

| Type | Role |
| --- | --- |
| `Layout` | a side's files: `Layout::device(sd)`, `Layout::pc(root, saves, states, cores)` |
| `Hub` | the PC's engine and state: pairing, busy, remove everywhere, conflicts, events |
| `Server` (`server`) | serves a `Hub` over HTTP and answers discovery; `Server::poke` |
| `Client` | the Miyoo's engine and state: `Client::open(layout)`, pairing, `sync(link, scope, events, cancel)`, `accept_poke` |
| `HttpLink` (`client`) | the client's connection to a hub |
| `discover`, `discover_broadcast`, `Poked` | UDP discovery and the poke listener |
| `Link`, `DirectLink` | the client's view of a hub; `DirectLink` is a hub in the same process |
| `Scope`, `Event`, `Report`, `SyncError` | what the apps pass in and render |

The client engine never needs the `server` feature and the hub never needs
`client`. The device binary takes `client` only. Linked into the static
armv7 `slot-mini` (release profile, 0.6.4) the client adds 258,624 bytes:
5,276,836 becomes 5,535,460, or 4.9%. `ureq`, `ring` and `serde_json` were
already there.

On the PC (brief 3):

```rust
let layout = Layout::pc(&library, &retroarch_saves, &retroarch_states, cores);
let hub = Arc::new(Hub::open(layout, "Desk")?);
hub.set_events(|event| { /* forward to the UI thread */ });
let server = Server::start(hub.clone(), "0.0.0.0:24735".parse()?,
                           Some("0.0.0.0:24735".parse()?))?;   // TCP, UDP discovery
hub.start_pairing();            // "Pair a device" open; PairingCode arrives
hub.confirm_pairing();          // the user said yes
hub.set_busy(Some(game));       // before RetroArch starts; None after
hub.is_syncing_game(&game);     // true: do not start it yet
hub.remove_everywhere(&game)?;
server.poke(POKE_PORT);         // "Sync now"
```

On the Miyoo (brief 4), all of it on a worker thread:

```rust
let mut client = Client::open(Layout::device(sd))?;
let found = discover_broadcast(Duration::from_secs(2))?;      // or the stored address
let mut link = HttpLink::new(&found[0].address.to_string());
let attempt = client.pair(&mut link)?;                        // show attempt.code
while client.finish_pairing(&mut link, &attempt)? == PairingStatus::Waiting { /* 1 s */ }
let report = client.sync(&mut link, Scope::Auto, &mut |event| { /* progress */ }, &cancel);
let ear = Poked::bind(([0, 0, 0, 0], POKE_PORT).into())?;      // blocks in wait()
if ear.wait().is_some_and(|poke| client.accept_poke(&poke)) { /* Full sync at client.hub().address */ }
```

`Client::sync` never returns an error: what stopped it is the report's
`interrupted`. A paired client reconnects to `client.hub().address`, and
discovers again when that fails.

## What each UI must show

All of this is data the apps render; the crate draws nothing.

**Progress** arrives as `Event`s through a callback, on the thread that is
syncing (the apps run it on a worker thread and forward the events):

| Event | Meaning | Show |
| --- | --- | --- |
| `Connecting` | finding and greeting the hub | "Looking for the PC…" |
| `Scanning { done, total }` | hashing this side's files, in bytes | a bar; on the Miyoo's first `Full` sync this is the long step |
| `Planning` | waiting for the hub's plan (the PC is scanning) | "Comparing…" |
| `Transfer { index, count, key, to_client, done, size }` | one file, in bytes | the game's name, n of m, a bar |
| `Finished(report)` | the sync is over | the summary below |

The hub tells the PC's UI through `Hub::set_events`, as `HubEvent`s, on the
thread serving the request:

| `HubEvent` | Meaning |
| --- | --- |
| `Sync { device, event }` | a device's sync is at this stage: `Connecting`, `Planning`, a `Transfer` as each file starts (`index` is the plan's action, `count` how many actions the plan has, `size` the file's bytes; `done` stays 0), `Finished` with the report |
| `Changed(key)` | the hub wrote this item, so the PC can redraw a cart; `Changed("text")` for shells and favourites |
| `PairingCode { device_name, code }` | a device wants to pair: show the code and ask |
| `Paired(device)`, `Unpaired(id)` | the list of devices changed |

**The summary** is `Report`, the same value on both sides (the client
builds it and sends it with `/sync/done`):

| Field | Meaning |
| --- | --- |
| `scope` | what was synced |
| `to_client.added`, `to_client.updated` | keys the Miyoo received, new and changed |
| `to_hub.added`, `to_hub.updated` | keys the PC received |
| `conflicts` | saves kept twice: key, which side's copy is now the save |
| `removed` | games removed by Remove everywhere |
| `didnt_fit` | ROMs left off the card: key and size |
| `busy` | games skipped because they were running on the PC |
| `unstorable` | names one side cannot store: key and side |
| `duplicates` | names that differ only by case on one side: key and side |
| `failed` | items whose transfer failed: key and the error text |
| `text_changed` | whether shells or favourites changed on the Miyoo (the hub says `Changed("text")` for its own) |
| `interrupted` | the error that stopped the sync early, if any |

`Report::is_clean()` is true when nothing needs the player's attention;
`is_noop()` when, besides, nothing moved and nothing changed on the Miyoo.
The Miyoo should reload its shelf after a sync that added, updated or
removed anything, or changed text. Conflicts must be visible on the PC
(`Hub::conflicts()`), with the swap.

**Errors** are `SyncError`; `Display` gives a sentence fit for the screen:

| Error | Shown as |
| --- | --- |
| `NotPaired` | "Pair with slot-mini PC first" |
| `NoHub` | "The PC was not found on this network" |
| `Unpaired` | "This device was unpaired on the PC" (the client has forgotten the hub) |
| `Protocol { hub, client }` | "Update slot-mini on the Miyoo…" or "Update slot-mini PC…" |
| `HubBusy` | "The PC is syncing with another device" |
| `Auth` | "The PC did not accept this device's key. Pair again" |
| `Network(text)` | "Lost the connection to the PC" |
| `Io(text)` | "Could not read or write a file: …" |
| `Cancelled` | "Sync stopped" |
| `Rejected`, `NotPairing` | pairing: "The PC declined" / "Open Pair a device on the PC first" |

**Pairing** on both sides: the hub's or device's name, the 6-digit code,
confirm and cancel. **Unpair** on both sides.

## Limits and open points

- **Renames and moves are a delete and an add.** Since deletions do not
  propagate, renaming a ROM or moving it into a subfolder on one side
  leaves both names on both sides after a `Full` sync. Detecting a move by
  hash would be safe and is left for the maintainer to decide.
- **A stem shared by GB and GBC** on the same core shares one save file on
  each side (as it already does on the Miyoo); sync treats both keys as
  that file. When one side has only one of the two games, the save of that
  one stem still serves both; two different games under one stem are not
  told apart.
- **The same game under two ROM paths** (`Game.gba` on one side,
  `Hacks/Game.gba` on the other) is one game with two ROM files, and after
  a `Full` sync both sides hold both.
- **A case-sensitive PC file system.** Sync finds a game's label, art and
  saves by the ROM's stem. On the Miyoo's FAT card and on Windows any case
  matches; on Linux a `game.srm` beside `Game.gba` is not that game's save
  and is not synced. Files sync creates always match.
- **An unsigned `unknown-device` reply** makes a client forget its hub.
  Someone on the LAN who can answer in the hub's place can therefore
  unpair a Miyoo (it must be paired again); they cannot read or change
  anything by it.
- **Packed ROMs** (`.zip`, `.7z`) sync as files like any other.
- **Hub-side space** is not planned; a full PC disk fails the item.
- **No encryption, keys in plain files** (see Authentication).
- **One hub per device.**

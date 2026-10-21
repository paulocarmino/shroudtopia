# Shroudtopia app — plan (awaiting UI approval)

A desktop app (`.exe`) beside the Shroudtopia loader: it keeps the mods up to date, toggles mods and
their options, and syncs a shared Enshrouded world between friends, the way MPSync did for
Satisfactory: whoever opens the game first hosts, the other joins through Steam, and when the host
closes the game the world goes up so the next one can host from the latest copy.

The UI is not built until Paulo approves a layout in `plans/ui-prototipo/index.html`.

## Decisions

- **Stack**: Rust + Tauri v2 + Svelte, on OrangeMacros' tokens and component rules
  (`orange-macros/DESIGN.md`), not tf3-mods' React. Built locally with OrangeMacros' Docker +
  cargo-xwin script.
- **Sync goes through a server on GAIA**, not PC to PC, so the two PCs never have to be on at the
  same time. World blobs in Garage (`gaia-iac/cluster/80-garage`), a small service beside it for
  metadata, the host lock and invite codes. Deployed the gaia-iac way (HelmChart, chart-owned
  secrets), by a subagent.
- **Only the world syncs.** Each player's character stays on their own PC: the game log shows the
  host sending the joiner's final save back to the joiner (`[save2] Server sent final client save`).
- **The app runs the loop on its own, no DLL needed** for the first version:
  `enshrouded.exe` starts → take the host lock (or show who holds it) and make sure the latest copy
  is in place; the world file is written → upload (every autosave, so a crash loses little);
  the game exits → final upload, release the lock. "Jogar" in the app pulls first, then launches
  the game through Steam. A later loader hook can block world load until the pull lands.
- **Every upload is kept server-side** as history; a bad upload is never the only copy. When both
  played without the other (offline, or lock ignored), the app shows "Vocês jogaram separados" and
  the player picks which one goes on; the other stays in history.
- **Names are Steam nicks, read automatically** (`Steam/config/loginusers.vdf`, `PersonaName`) and sent with the pairing; nobody types a name.
- **Pairing by code**: share a world → code like `ENSH-7KQ2` (24 h, one use) → the friend pastes it
  and the world lands in a free save slot on their PC.

## Spike results (2026-10-21)

1. **Steam Cloud slots are closed to us.** The game lists Steam saves through Steam's remote
   storage API: a `world2.ksd` copied into `userdata/<id>/1203620/remote/` stayed "Espaço Vazio".
2. **Local saves work.** Enshrouded 1.0 has a "Jogos salvos: Local" tab backed by plain files in
   `%USERPROFILE%\Saved Games\keen games\Enshrouded\default\worldN.ksd`. A copied `world0.ksd`
   showed up there as slot 1 with its name. Shared worlds live in that folder; the player's own
   Steam worlds are only read (to offer one for sharing, copied into a free local slot).
3. Writing Steam slots would need the Steam API running as app 1203620 (`ISteamRemoteStorage`);
   not planned. Steam Cloud only syncs one account's PCs anyway; the server's history replaces it.
4. **Pull window.** "Jogar" in the app pulls first, then launches the game.

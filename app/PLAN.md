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
- **Pairing by code**: share a world → code like `ENSH-7KQ2` (24 h, one use) → the friend pastes it
  and the world lands in a free save slot on their PC.

## Risks to settle in the first spike

1. **Steam Cloud.** Saves live in `Steam/userdata/<id>/1203620/remote/`, Steam's cloud folder. A
   file written there while the game is closed may be reverted by Steam or trigger its conflict
   dialog at launch. Test before anything else; fallback is "turn Steam Cloud off for Enshrouded".
2. **Slot identity.** Worlds are `world0.ksd`, `world1.ksd`… per PC, and `global.ksd` is rewritten
   with them and may index the slots. Test: copy a world into a free slot, launch, read
   `enshrouded.log`.
3. **Pull window.** Without the loader hook, a pull must finish before the world is clicked. Fine
   at today's sizes (150 KB – 1.8 MB); "Jogar" in the app removes the race.

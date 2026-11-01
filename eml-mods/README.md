# EML mods

Lua mods that run on [EML](https://github.com/Brabb3l/kfc-parser) (Enshrouded Mod Loader, Brabb3l), a second loader
that sits beside Shroudtopia's: EML is `dbghelp.dll` in the game folder, Shroudtopia is `winmm.dll` + `shroudtopia.dll`.
Both read `mods/`. EML takes the folders whose `mod.json` it understands and runs their `src/mod.lua`; Shroudtopia
loads the DLL in a folder and skips one that has none, as it does `max-stack/`.

- `max-stack/`: Max Stack 1.0.0 from Nexus (MIT) with its options fixed (×5, up to 999, never below vanilla).
- `twitch-drops/`: every Twitch drop hands out the items of all 50 drops (57 outfits and decorations). Keen's server
  still says which drops a player has, so it covers players with at least one drop unlocked; the host's game decides,
  so the host needs the mod. The items keep their `TwitchDrop` flag: claiming needs it.
- `loot-multipliers/`: Gather & Loot Multipliers 2.2.0 from Nexus (MIT), with its patches redone from the game's data
  (`src/mod.lua` lists what changed) so what goes straight into the inventory is multiplied too. The values are in
  `settings.txt` and the `*/items.txt` lists, not in `config.txt`: they ship with the mod, a release replaces them on
  every PC, and the app's toggle (which rewrites `config.txt`) cannot touch them. To change them, edit those files and
  publish a release. Never add a `config.txt` to this folder, or each release would reset the players' on/off. The
  host's game decides what drops, so the host needs the mod.

`eml.lock` pins the EML release the app ships: `release.sh` downloads that `dbghelp.dll` and refuses any other bytes.
To move to a newer EML, change the three lines there to the new release asset's URL, size and sha256 (GitHub shows
the digest in the release's API: `https://api.github.com/repos/Brabb3l/kfc-parser/releases/latest`).

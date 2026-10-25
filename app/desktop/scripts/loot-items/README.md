# Loot filter items

The loot filter panel lists the game's items with their own icons. Both come from the game's data, extracted with EML
and turned into `src/lib/loot-items.json` + `static/loot-icons/*.webp`, which the app ships. Redo it after a game
update that adds items.

1. Make a folder on the game's drive that EML can read without touching the install: hard links to every non-empty
   `enshrouded_0*.dat` (empty ones as empty files), copies of `enshrouded.exe`, `enshrouded.kfc`,
   `enshrouded.kfc_resources` and `.cache/types.json`, and `mods/item-export/` (this folder's `item-export`).
2. Run EML's CLI there with export only (`emm.exe` from the EML release pinned in `eml-mods/eml.lock`):

   ```
   emm.exe run -e -g <that folder>
   ```

   It writes `<that folder>/export/items.json` and `export/icons/<guid>.png` (256×256), about 10 s.
3. `python build.py <that folder>/export` (needs Pillow) writes the list and the 64×64 webp icons.

The icons are `UiTextureResource`s named `<debugName>_icon` (every item has one; their guid is the item's objectId).
`build.py` keeps only items that can drop as loot and merges items sharing a name in a category into one entry.

"""Turns item-export's output into what the app ships for the loot filter:

  src/lib/loot-items.json    one entry per item the filter lists (pt-BR name, English name, category, icon, item ids)
  static/loot-icons/*.webp   its icon, 64x64

Usage: python build.py <export dir>   (the folder holding items.json and icons/, see README.md)

Only items that can lie on the ground as loot are listed: no building pieces, blueprints, colours, cosmetics or
internal entries (no pt-BR name). Items that share a name in a category (one weapon at several levels, say) are one
entry with all their ids, so ticking it covers every variant.
"""

import json
import re
import shutil
import sys
from pathlib import Path

from PIL import Image

DESKTOP = Path(__file__).resolve().parents[2]
ITEMS_OUT = DESKTOP / "src" / "lib" / "loot-items.json"
ICONS_OUT = DESKTOP / "static" / "loot-icons"
ICON_SIZE = 64

CATEGORIES = ["Materials", "Consumables", "Ammunition", "Weapons", "Equipment", "WeaponGem", "Tools", "Collectibles",
              "AnimalFood", "Instrument", "FishingRod"]
SKIP_FLAGS = {"IsVanityItem", "IsBlueprint", "IsNpc", "IsAnimal", "IsLogicEntity", "IsFactoryEntity", "TwitchDrop",
              "CommerceItem"}
MARKUP = re.compile(r"<[^>]+>")


def clean(name):
    return MARKUP.sub("", name).strip()


def main(export_dir):
    items = json.loads((export_dir / "items.json").read_text(encoding="utf-8"))
    groups = {}
    for item in items:
        pt = clean(item["pt"])
        if not pt or not item["icon"] or item["category"] not in CATEGORIES or SKIP_FLAGS & set(item["flags"]):
            continue
        key = (item["category"], pt)
        group = groups.setdefault(key, {"n": pt, "e": clean(item["en"]), "c": item["category"], "i": item["icon"], "ids": []})
        group["ids"].append(item["id"])

    entries = sorted(groups.values(), key=lambda g: (CATEGORIES.index(g["c"]), g["n"].casefold()))
    for entry in entries:
        entry["ids"].sort()

    if ICONS_OUT.exists():
        shutil.rmtree(ICONS_OUT)
    ICONS_OUT.mkdir(parents=True)
    for icon in sorted({e["i"] for e in entries}):
        with Image.open(export_dir / "icons" / f"{icon}.png") as img:
            img.convert("RGBA").resize((ICON_SIZE, ICON_SIZE), Image.LANCZOS).save(ICONS_OUT / f"{icon}.webp", quality=85, method=6)

    ITEMS_OUT.write_text(json.dumps(entries, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
    size = sum(f.stat().st_size for f in ICONS_OUT.iterdir())
    print(f"{len(entries)} entries ({sum(len(e['ids']) for e in entries)} item ids), {size // 1024} KB of icons")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    main(Path(sys.argv[1]))

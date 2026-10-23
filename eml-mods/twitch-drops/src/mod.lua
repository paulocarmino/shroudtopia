-- Twitch Drops for Shroudtopia. config.txt holds only `enabled=true|false`, written by the app's toggle (missing means
-- on). Read at game start.
--
-- How the game gates drops (build 1273043): keen::TwitchDropCollection lists 50 drops (Campaign01..09, CampaignXX),
-- each with a dropId and the items it gives (`items` = ItemInfo itemIds, `itemRefs` = their guids, kept in step). Keen's
-- server tells the host which drops each player has; the Twitch menu lists, and lets a player claim and equip, the items
-- of those drops. The 57 items carry the ItemFlag `TwitchDrop`, which claiming needs: with it cleared the menu still
-- lists the item but Resgatar does nothing (tested 2026-10-22), so the flag stays.
--
-- This mod gives every drop the items of all drops, so a player with any one drop unlocked can claim and wear all of
-- them. A player with no drop at all on Keen's server still gets nothing. The host's game decides, so the host (or
-- dedicated server) needs this mod.
-- Runs on EML (dbghelp.dll), not on the Shroudtopia loader, which skips this folder (no DLL in it).

local function enabled()
    -- EML raises an error for a missing file rather than returning nil
    local ok, content = pcall(io.read_to_string, "config.txt")
    if not ok or not content then return true end
    local value = string.match(content, "enabled%s*=%s*(%a+)")
    return value == nil or string.lower(value) ~= "false"
end

-- every drop item once, in the order the drops list them
local function collectItems(collections)
    local seen, items, refs = {}, {}, {}
    for _, collection in ipairs(collections) do
        for _, drop in ipairs(collection.data.drops) do
            for i, item in ipairs(drop.items) do
                if not seen[item.value] and drop.itemRefs[i] then
                    seen[item.value] = true
                    items[#items + 1] = { value = item.value }
                    refs[#refs + 1] = drop.itemRefs[i]
                end
            end
        end
    end
    return items, refs
end

local function giveAllItems(collections, items, refs)
    local drops = 0
    for _, collection in ipairs(collections) do
        for _, drop in ipairs(collection.data.drops) do
            -- assigning copies the tables, so each drop gets its own arrays
            drop.items = items
            drop.itemRefs = refs
            drops = drops + 1
        end
    end
    return drops
end

if enabled() then
    local collections = game.assets.get_resources_by_type("keen::TwitchDropCollection")
    local items, refs = collectItems(collections)
    if #items == 0 then
        print("[twitch-drops] No Twitch drops found in the game data, nothing changed")
        return
    end
    local drops = giveAllItems(collections, items, refs)
    print(string.format("[twitch-drops] drops=%d items=%d", drops, #items))
else
    print("[twitch-drops] Off (config.txt: enabled=false)")
end

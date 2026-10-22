-- Twitch Drops for Shroudtopia. config.txt holds only `enabled=true|false`, written by the app's toggle (missing means
-- on). Read at game start.
--
-- How the game gates drops (build 1273043): keen::TwitchDropCollection lists 50 drops (Campaign01..09, CampaignXX),
-- each with a dropId and the items it gives (`items` = ItemInfo itemIds, `itemRefs` = their guids, kept in step). Keen's
-- server tells the host which drops each player has; the Twitch menu then hands out the items of those drops. Each of
-- the 57 items carries the ItemFlag `TwitchDrop`, which blocks equipping/placing it without the Twitch permission.
--
-- This mod gives every drop the items of all drops, so a player with any one drop unlocked can claim all of them, and
-- clears the `TwitchDrop` flag so they work without the permission. A player with no drop at all on Keen's server
-- still gets nothing from the menu. The host's game decides, so the host (or dedicated server) needs this mod.
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

local function clearTwitchFlag(refs)
    local cleared = 0
    for _, ref in ipairs(refs) do
        local ok, resource = pcall(function()
            return game.assets.get_resource(ref, "keen::ItemInfo")
        end)
        if ok and resource and resource.data then
            local item = resource.data
            local flags, had = {}, false
            for _, flag in ipairs(item.flags) do
                if flag == "TwitchDrop" then had = true else flags[#flags + 1] = flag end
            end
            if had then
                item.flags = flags
                cleared = cleared + 1
            end
        end
    end
    return cleared
end

if enabled() then
    local collections = game.assets.get_resources_by_type("keen::TwitchDropCollection")
    local items, refs = collectItems(collections)
    if #items == 0 then
        print("[twitch-drops] No Twitch drops found in the game data, nothing changed")
        return
    end
    local drops = giveAllItems(collections, items, refs)
    local cleared = clearTwitchFlag(refs)
    print(string.format("[twitch-drops] drops=%d items=%d twitch flag cleared=%d", drops, #items, cleared))
else
    print("[twitch-drops] Off (config.txt: enabled=false)")
end

-- Max Stack 1.0.0 (Nexus, "Enshrouded Mods", MIT) with its options fixed for Shroudtopia: no items.txt, and
-- config.txt holds only `enabled=true|false`, written by the app's toggle (missing means on). Read at game start.
-- An item that stacks in vanilla stacks 5x as much, up to 999. An item that already stacks more than that keeps its
-- vanilla size (the original lowered it to 999). Items that do not stack (weapons, armour...) are left alone.
-- Runs on EML (dbghelp.dll), not on the Shroudtopia loader, which skips this folder (no DLL in it).

local MAX_STACK_SIZE = 999
local STACK_MULTIPLIER = 5.0

local function enabled()
    -- EML raises an error for a missing file rather than returning nil
    local ok, content = pcall(io.read_to_string, "config.txt")
    if not ok or not content then return true end
    local value = string.match(content, "enabled%s*=%s*(%a+)")
    return value == nil or string.lower(value) ~= "false"
end

local function computeStackSize(vanillaSize)
    if vanillaSize <= 1 then
        return vanillaSize
    end
    local multiplied = math.floor(vanillaSize * STACK_MULTIPLIER + 0.5)
    return math.max(vanillaSize, math.min(MAX_STACK_SIZE, multiplied))
end

local function patchItemStacks()
    local changed = 0
    local total = 0

    for _, registry in ipairs(game.assets.get_resources_by_type("keen::ItemRegistryResource")) do
        local data = registry.data
        if not data.dbgNames or not data.itemRefs then goto continue end

        for i, _ in ipairs(data.dbgNames) do
            local ok, resource = pcall(function()
                return game.assets.get_resource(data.itemRefs[i], "keen::ItemInfo")
            end)
            if not ok or not resource or not resource.data then goto next_item end

            local item = resource.data
            if not item.maxStackSize then goto next_item end

            total = total + 1
            local oldSize = item.maxStackSize
            local newSize = computeStackSize(oldSize)
            if newSize ~= oldSize then
                item.maxStackSize = newSize
                changed = changed + 1
            end

            ::next_item::
        end

        ::continue::
    end

    return changed, total
end

if enabled() then
    print(string.format("[max-stack] Loading (x%s, up to %d, never below vanilla)", tostring(STACK_MULTIPLIER), MAX_STACK_SIZE))
    local changed, total = patchItemStacks()
    print(string.format("[max-stack] items=%d changed=%d unchanged=%d", total, changed, total - changed))
else
    print("[max-stack] Off (config.txt: enabled=false)")
end

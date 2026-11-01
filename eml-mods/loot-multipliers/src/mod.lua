-- Gather & Loot Multipliers 2.2.0 (Nexus, "Enshrouded Mods", MIT) fixed for Shroudtopia.
-- Config: the multipliers live in settings.txt, which ships with the mod (a release replaces it, so every player gets
-- the same values), and config.txt holds only `enabled=true|false`, written by the app's toggle (missing means on).
-- Patches, read from the game's own data (build 1273043); the original missed everything that goes straight into the
-- inventory and multiplied some drops twice:
-- - terrain (ore veins, dirt, stone): TerraformingEfficiencyRegistryResource, voxels mined per item, which the original
--   never touched. Whole numbers rounded down, so it gives at least the multiplier: 5 at x3 is 1 (x5). Nothing goes
--   below 1, so a rate of 2 tops out at x2.
-- - plants, bushes, flame sparks, corpses, shelves: DefaultInventoryResource, also untouched by the original. A template
--   with a default inventory gives that and ignores its PickupItem amount, so only pickups without one are scaled.
-- - resource nodes: only the amount per drop (stackAmount, or the dropped entity's pickupAmount). The original also
--   scaled the number of drops, so x2 gave x4 or more.
-- - chests: SceneRandomLootResource specials; the LootDistributionTreeResource the original patched does not exist.
-- - animals and crops that refill: InteractionLootRefill.
-- - no amount goes above the item's stack size (wax reached 9999), and randomLootStackRange is left alone: it is baked
--   into LootableItemsResource at build time, so scaling it did nothing but count an item's gather values as loot.
-- Runs on EML (dbghelp.dll), not on the Shroudtopia loader, which skips this folder (no DLL in it).

local DEFAULT_CONFIG = {
    enabled = true,
    globalMultiplier = 1.0,

    collectingGlobalMultiplier = 1.0,
    collectingMultiplier = 1.0,

    treesGlobalMultiplier = 1.0,
    treesMultiplier = 1.0,

    farmingGlobalMultiplier = 1.0,
    farmingMultiplier = 1.0,

    enemyGlobalMultiplier = 1.0,
    enemyLootMultiplier = 1.0,

    chestGlobalMultiplier = 1.0,
    chestLootMultiplier = 1.0,

    patchGameSettings = false,
    patchTerrain = true,
    patchEnemyLoot = true,
    patchChestLoot = true,
}

local ITEM_FILES = {
    collecting = "collecting/items.txt",
    trees = "trees/items.txt",
    farming = "farming/items.txt",
    enemies = "enemies/items.txt",
    chests = "chests/items.txt",
}

local BOOL_KEYS = {
    enabled = true,
    patchGameSettings = true,
    patchTerrain = true,
    patchEnemyLoot = true,
    patchChestLoot = true,
}

local MAX_U8 = 255
local MAX_U16 = 65535
local MAX_U32 = 4294967295

local itemById = {}
local itemByName = {}
local guidToName = {}
local itemMultipliers = {
    collecting = {},
    trees = {},
    farming = {},
    enemies = {},
    chests = {},
}
-- gather items by the file that lists them, which wins over the name guess in getActivityType
local gatherSection = {}
-- the default inventories the templates use: tostring(ref) -> { ref, section }
local inventories = {}

-- EML raises an error for a missing file rather than returning nil
local function readFile(path)
    local ok, content = pcall(io.read_to_string, path)
    if ok then return content end
    return nil
end

local function parseBool(value)
    value = string.lower(value)
    return value == "1" or value == "true" or value == "yes" or value == "on"
end

local function parseKeyValueFile(path, into)
    local content = readFile(path)
    if not content then return 0 end

    local loaded = 0
    for line in string.gmatch(content, "[^\r\n]+") do
        local trimmed = string.match(line, "^%s*(.-)%s*$")
        if trimmed ~= "" and not string.match(trimmed, "^#") then
            local key, rawValue = string.match(trimmed, "^([^=]+)=(.*)$")
            if key and rawValue then
                key = string.match(key, "^%s*(.-)%s*$")
                rawValue = string.match(rawValue, "^%s*(.-)%s*$")
                local numberValue = tonumber(rawValue)
                if numberValue ~= nil then
                    into[key] = numberValue
                    loaded = loaded + 1
                end
            end
        end
    end
    return loaded
end

local function parseConfigFile(path)
    local values = {}
    local content = readFile(path)
    if not content then return values end

    for line in string.gmatch(content, "[^\r\n]+") do
        local trimmed = string.match(line, "^%s*(.-)%s*$")
        if trimmed ~= "" and not string.match(trimmed, "^#") then
            local key, rawValue = string.match(trimmed, "^([^=]+)=(.*)$")
            if key and rawValue then
                key = string.match(key, "^%s*(.-)%s*$")
                rawValue = string.match(rawValue, "^%s*(.-)%s*$")
                if BOOL_KEYS[key] then
                    values[key] = parseBool(rawValue)
                else
                    local numberValue = tonumber(rawValue)
                    if numberValue ~= nil then values[key] = numberValue end
                end
            end
        end
    end
    return values
end

local function loadConfig()
    local cfg = {}
    for key, value in pairs(DEFAULT_CONFIG) do cfg[key] = value end
    for key, value in pairs(parseConfigFile("settings.txt")) do cfg[key] = value end
    -- the on/off is each player's, everything else comes with the mod
    cfg.enabled = parseConfigFile("config.txt").enabled ~= false
    return cfg
end

local config = loadConfig()

local function loadItemFiles()
    local total = 0
    for _, path in pairs(ITEM_FILES) do
        local bucket = string.match(path, "^([^/]+)/")
        if bucket and itemMultipliers[bucket] then
            total = total + parseKeyValueFile(path, itemMultipliers[bucket])
        end
    end
    for _, section in ipairs({ "collecting", "trees", "farming" }) do
        for name in pairs(itemMultipliers[section]) do gatherSection[name] = section end
    end
    return total
end

local function sectionGlobal(section)
    local legacy = config.globalMultiplier or 1.0
    if section == "collecting" then
        return config.collectingGlobalMultiplier or legacy
    end
    if section == "trees" then
        return config.treesGlobalMultiplier or legacy
    end
    if section == "farming" then
        return config.farmingGlobalMultiplier or legacy
    end
    if section == "enemies" then
        return config.enemyGlobalMultiplier or legacy
    end
    if section == "chests" then
        return config.chestGlobalMultiplier or legacy
    end
    return legacy
end

local function sectionActivity(section)
    if section == "trees" then
        return config.treesMultiplier or 1.0
    end
    if section == "farming" then
        return config.farmingMultiplier or 1.0
    end
    if section == "enemies" then
        return config.enemyLootMultiplier or 1.0
    end
    if section == "chests" then
        return config.chestLootMultiplier or 1.0
    end
    return config.collectingMultiplier or 1.0
end

local function getActivityType(debugName)
    if not debugName then return "collecting" end

    if string.find(debugName, "Wood", 1, true)
        or string.find(debugName, "Twigs", 1, true)
        or string.find(debugName, "Plant_Fibre", 1, true)
        or string.find(debugName, "TallTree", 1, true)
        or string.match(debugName, "Seedling_.*Tree")
        or string.match(debugName, "_Tree_")
        or string.match(debugName, "Tree_") then
        return "trees"
    end

    if string.match(debugName, "^T%d_Seedling")
        or string.match(debugName, "^_T%d_Seedling")
        or string.match(debugName, "Seedling")
        or string.match(debugName, "^Food_T%d_raw") then
        return "farming"
    end

    return "collecting"
end

-- value x multiplier, at least 1, within the field's limit and the item's stack size (never below what vanilla gives)
local function scaleAmount(value, multiplier, limit, meta)
    if not value or multiplier == 1.0 then return value end
    local scaled = math.min(limit, math.max(1, math.floor(value * multiplier + 0.5)))
    local maxStack = meta and meta.maxStackSize
    if maxStack then
        scaled = math.min(scaled, math.max(value, maxStack))
    end
    return scaled
end

local function safeField(obj, field)
    if not obj then return nil end
    local ok, value = pcall(function() return obj[field] end)
    if ok then return value end
    return nil
end

local function trySetField(obj, field, value)
    if not obj then return false end
    return pcall(function() obj[field] = value end)
end

local function foreachArray(arr, fn)
    if not arr then return end
    local count = 0
    pcall(function() count = #arr end)
    for i = 1, count do
        local entry = nil
        pcall(function() entry = arr[i] end)
        if entry then fn(entry, i) end
    end
end

-- an ItemId may come as a number or as a { value } struct
local function idKey(itemId)
    return tostring(safeField(itemId, "value") or itemId)
end

local function tryResolveItemInfo(itemRef)
    if not itemRef then return nil end
    local ok, resource = pcall(function()
        return game.assets.get_resource(itemRef, "keen::ItemInfo")
    end)
    if ok and resource and resource.data then return resource.data end
    return nil
end

local function registerItemMeta(debugName, itemInfo, itemRef)
    if not debugName then return end

    local meta = {
        debugName = debugName,
        category = itemInfo and itemInfo.category or nil,
        itemId = itemInfo and itemInfo.itemId or nil,
        maxStackSize = itemInfo and itemInfo.maxStackSize or nil,
        activity = gatherSection[debugName] or getActivityType(debugName),
    }
    itemByName[debugName] = meta
    if meta.itemId then
        itemById[idKey(meta.itemId)] = meta
    end
    if itemRef then
        guidToName[tostring(itemRef)] = debugName
    end
end

local function buildItemLookup()
    local registries = game.assets.get_resources_by_type("keen::ItemRegistryResource")
    for _, registryResource in ipairs(registries) do
        local data = registryResource.data
        if data.dbgNames and data.itemRefs then
            for i, debugName in ipairs(data.dbgNames) do
                local itemRef = data.itemRefs[i]
                local itemInfo = tryResolveItemInfo(itemRef)
                registerItemMeta(debugName, itemInfo, itemRef)
            end
        end
    end
end

local function lookupMeta(itemId, itemRef)
    if itemId then
        local meta = itemById[idKey(itemId)]
        if meta then return meta end
    end

    if itemRef then
        local byGuid = guidToName[tostring(itemRef)]
        if byGuid and itemByName[byGuid] then
            return itemByName[byGuid]
        end
    end

    local itemInfo = tryResolveItemInfo(itemRef)
    if itemInfo and itemInfo.debugName and itemByName[itemInfo.debugName] then
        return itemByName[itemInfo.debugName]
    end

    return nil
end

local function perItemMultiplier(section, debugName)
    local bucket = itemMultipliers[section]
    if debugName and bucket and bucket[debugName] then
        return bucket[debugName]
    end
    return 1.0
end

local function computeGatherMultiplier(meta, debugName)
    local name = debugName or (meta and meta.debugName)
    local activity = (meta and meta.activity) or getActivityType(name)
    local mult = sectionGlobal(activity) * sectionActivity(activity) * perItemMultiplier(activity, name)
    return mult
end

local function computeEnemyMultiplier(meta)
    local name = meta and meta.debugName
    local mult = sectionGlobal("enemies") * sectionActivity("enemies") * perItemMultiplier("enemies", name)
    return mult
end

local function computeChestMultiplier(meta)
    local name = meta and meta.debugName
    local mult = sectionGlobal("chests") * sectionActivity("chests") * perItemMultiplier("chests", name)
    return mult
end

local function sectionMultiplier(section, meta)
    if section == "enemies" then return computeEnemyMultiplier(meta) end
    if section == "chests" then return computeChestMultiplier(meta) end
    return computeGatherMultiplier(meta, meta and meta.debugName)
end

local function isSetRef(ref)
    return ref ~= nil and not string.match(tostring(ref), "^[0%-]*$")
end

-- Whose loot a default inventory is, by the name of a template that uses it
local function inventorySection(templateName)
    local name = templateName or ""
    if string.find(name, "Enemy", 1, true)
        or string.find(name, "Wildlife", 1, true)
        or string.find(name, "Wildbeast", 1, true)
        or string.find(name, "Animal", 1, true) then
        return "enemies"
    end
    if string.find(name, "Chest", 1, true)
        or string.find(name, "Container", 1, true)
        or string.match(name, "^Prop_") then
        return "chests"
    end
    return "gather"
end

local SECTION_RANK = { gather = 1, chests = 2, enemies = 3 }

-- A few inventories are shared (a coffin and a corpse): the highest rank decides
local function noteInventory(ref, templateName)
    if not isSetRef(ref) then return false end
    local key = tostring(ref)
    local section = inventorySection(templateName)
    local known = inventories[key]
    if not known or SECTION_RANK[section] > SECTION_RANK[known.section] then
        inventories[key] = { ref = ref, section = section }
    end
    return true
end

local function patchPickupItemValue(value)
    if not value then return 0 end

    -- the shells nodes drop have no item of their own: the drop's stackAmount covers them
    local meta = lookupMeta(safeField(value, "itemId"), safeField(value, "item"))
    if not meta then return 0 end

    local mult = computeGatherMultiplier(meta, meta.debugName)
    if mult == 1.0 then return 0 end

    local pickupAmount = safeField(value, "pickupAmount")
    if pickupAmount and trySetField(value, "pickupAmount", scaleAmount(pickupAmount, mult, MAX_U16, meta)) then
        return 1
    end

    return 0
end

-- Drops that are an entity are scaled by that entity's own PickupItem, as every template is visited
local function patchResourceNodeDrop(drop)
    local meta = lookupMeta(nil, safeField(drop, "item"))
    if not meta then return 0 end

    local mult = computeGatherMultiplier(meta, meta.debugName)
    if mult == 1.0 then return 0 end

    local stackAmount = safeField(drop, "stackAmount")
    if stackAmount and trySetField(drop, "stackAmount", scaleAmount(stackAmount, mult, MAX_U8, meta)) then
        return 1
    end
    return 0
end

local function patchLootRefill(entry)
    local meta = lookupMeta(nil, safeField(entry, "lootableItemRef"))
    if not meta then return 0 end

    local mult = computeGatherMultiplier(meta, meta.debugName)
    if mult == 1.0 then return 0 end

    local changed = 0
    for _, field in ipairs({ "refillCount", "maxCount" }) do
        local count = safeField(entry, field)
        if count and trySetField(entry, field, scaleAmount(count, mult, MAX_U32, meta)) then
            changed = changed + 1
        end
    end
    return changed
end

local function patchTemplateResource(resource)
    local data = resource.data
    if not data or not data.components then return 0 end

    local templateName = safeField(data, "name")
    local changed = 0

    local hasInventory = false
    for _, comp in ipairs(data.components) do
        local compType = tostring(comp.type or "")
        if compType == "keen::ecs::DefaultInventory" and noteInventory(safeField(comp.value, "defaultInventory"), templateName) then
            hasInventory = true
        elseif compType == "keen::ecs::TransformToLoot" then
            noteInventory(safeField(comp.value, "defaultInventory"), templateName)
        end
    end

    for _, comp in ipairs(data.components) do
        local compType = tostring(comp.type or "")
        local value = comp.value
        if not value then goto continue end

        if compType == "keen::ecs::ResourceNodePickupDrops" then
            foreachArray(safeField(value, "drops"), function(drop)
                changed = changed + patchResourceNodeDrop(drop)
            end)
        elseif compType == "keen::ecs::InteractionLootRefill" then
            foreachArray(safeField(value, "items"), function(entry)
                changed = changed + patchLootRefill(entry)
            end)
        elseif compType == "keen::ecs::PickupItem" and not hasInventory then
            changed = changed + patchPickupItemValue(value)
        end

        ::continue::
    end

    return changed
end

local function patchGatherTemplates()
    local changed = 0
    for _, resource in ipairs(game.assets.get_resources_by_type("keen::ecs::TemplateResource")) do
        changed = changed + patchTemplateResource(resource)
    end
    return changed
end

local function patchInventoryGroup(group, section)
    local changed = 0

    foreachArray(safeField(group, "stacks"), function(stack)
        local meta = lookupMeta(safeField(stack, "item"), safeField(stack, "itemRef"))
        local mult = sectionMultiplier(section, meta)
        if mult ~= 1.0 then
            for _, field in ipairs({ "countMin", "countMax" }) do
                local count = safeField(stack, field)
                if count and trySetField(stack, field, scaleAmount(count, mult, MAX_U32, meta)) then
                    changed = changed + 1
                end
            end
        end
    end)

    foreachArray(safeField(group, "groups"), function(child)
        changed = changed + patchInventoryGroup(child, section)
    end)

    return changed
end

-- Runs after patchGatherTemplates, which finds the inventories
local function patchDefaultInventories()
    local changed = 0
    for _, inventory in pairs(inventories) do
        local section = inventory.section
        local skip = (section == "enemies" and not config.patchEnemyLoot)
            or (section == "chests" and not config.patchChestLoot)
        if not skip then
            local ok, resource = pcall(function()
                return game.assets.get_resource(inventory.ref, "keen::ecs::DefaultInventoryResource")
            end)
            if ok and resource and resource.data then
                changed = changed + patchInventoryGroup(safeField(resource.data, "rootGroup"), section)
            end
        end
    end
    return changed
end

-- Voxels mined per item: fewer voxels, more items
local function patchTerrainLoot()
    if not config.patchTerrain then return 0 end

    local changed = 0
    for _, resource in ipairs(game.assets.get_resources_by_type("keen::TerraformingEfficiencyRegistryResource")) do
        foreachArray(safeField(resource.data, "terrainConfigs"), function(terrain)
            local meta = lookupMeta(nil, safeField(terrain, "terrainItem"))
            local mult = meta and computeGatherMultiplier(meta, meta.debugName) or 1.0
            local rate = safeField(terrain, "terrainPerLootItemExchangeRate")
            if rate and mult ~= 1.0 and mult > 0 then
                local newRate = math.max(1, math.floor(rate / mult))
                if newRate ~= rate and trySetField(terrain, "terrainPerLootItemExchangeRate", newRate) then
                    changed = changed + 1
                end
            end
        end)
    end
    return changed
end

local function patchLootableItem(entry)
    local meta = lookupMeta(entry.itemId, nil)
    local mult = computeEnemyMultiplier(meta)
    if mult == 1.0 then return 0 end

    local changed = 0
    if entry.stackSizeMin then
        entry.stackSizeMin = scaleAmount(entry.stackSizeMin, mult, MAX_U16, meta)
        changed = changed + 1
    end
    if entry.stackSizeMax then
        entry.stackSizeMax = scaleAmount(entry.stackSizeMax, mult, MAX_U16, meta)
        changed = changed + 1
    end
    if entry.stackSizeMaxScaled then
        entry.stackSizeMaxScaled = scaleAmount(entry.stackSizeMaxScaled, mult, MAX_U16, meta)
        changed = changed + 1
    end
    return changed
end

-- The random loot table: enemies and chests roll on this same one, so it follows the enemy multipliers
local function patchEnemyLoot()
    if not config.patchEnemyLoot then return 0 end

    local lootable = 0
    for _, typeName in ipairs({ "keen::LootableItemsResource", "keen::DevLootableItemsResource" }) do
        for _, resource in ipairs(game.assets.get_resources_by_type(typeName)) do
            if resource.data.items then
                for _, entry in ipairs(resource.data.items) do
                    lootable = lootable + patchLootableItem(entry)
                end
            end
        end
    end

    return lootable
end

-- The items placed in the world's chests by hand; quest items stay as they are
local function patchChestLoot()
    if not config.patchChestLoot then return 0 end

    local special = 0
    for _, resource in ipairs(game.assets.get_resources_by_type("keen::SceneRandomLootResource")) do
        foreachArray(safeField(resource.data, "specialItems"), function(entry)
            local meta = lookupMeta(safeField(entry, "itemId"), nil)
            local mult = computeChestMultiplier(meta)
            local amount = safeField(entry, "amount")
            if mult ~= 1.0 and amount and not safeField(entry, "isQuestItem")
                and trySetField(entry, "amount", scaleAmount(amount, mult, MAX_U8, meta)) then
                special = special + 1
            end
        end)
    end
    return special
end

local function patchGameSettingsPresets()
    if not config.patchGameSettings then return 0 end

    local mult = math.max(
        sectionGlobal("collecting") * sectionActivity("collecting"),
        sectionGlobal("trees") * sectionActivity("trees"),
        sectionGlobal("farming") * sectionActivity("farming"),
        sectionGlobal("enemies") * sectionActivity("enemies"),
        sectionGlobal("chests") * sectionActivity("chests")
    )
    if mult == 1.0 then return 0 end

    local resources = game.assets.get_resources_by_type("keen::GameSettingsPresetsResource")
    if #resources == 0 then return 0 end

    local changed = 0
    for _, resource in ipairs(resources) do
        if resource.data.presets then
            for _, preset in ipairs(resource.data.presets) do
                local settings = preset.settings
                if settings and settings.resourceDropStackAmountFactor then
                    settings.resourceDropStackAmountFactor = settings.resourceDropStackAmountFactor * mult
                    changed = changed + 1
                end
            end
        end
    end
    return changed
end

local function tableSize(tbl)
    local count = 0
    for _ in pairs(tbl) do count = count + 1 end
    return count
end

print("[loot-multipliers] v2.2.1 Loading...")
print(string.format("[loot-multipliers] enabled=%s", tostring(config.enabled)))

if config.enabled then
    local cfgItems = loadItemFiles()
    buildItemLookup()

    local templates = patchGatherTemplates()
    local defaultInventories = patchDefaultInventories()
    local terrain = patchTerrainLoot()
    local enemyLootable = patchEnemyLoot()
    local chestSpecial = patchChestLoot()
    local gameSettings = patchGameSettingsPresets()

    print(string.format(
        "[loot-multipliers] items=%d configured=%d",
        tableSize(itemByName),
        cfgItems
    ))
    print(string.format(
        "[loot-multipliers] globals collect=%s trees=%s farm=%s enemy=%s chest=%s",
        tostring(sectionGlobal("collecting")),
        tostring(sectionGlobal("trees")),
        tostring(sectionGlobal("farming")),
        tostring(sectionGlobal("enemies")),
        tostring(sectionGlobal("chests"))
    ))
    print(string.format(
        "[loot-multipliers] activity collect=%s trees=%s farm=%s enemy=%s chest=%s",
        tostring(sectionActivity("collecting")),
        tostring(sectionActivity("trees")),
        tostring(sectionActivity("farming")),
        tostring(sectionActivity("enemies")),
        tostring(sectionActivity("chests"))
    ))
    print(string.format(
        "[loot-multipliers] patched templates=%d inventories=%d terrain=%d enemy=%d chest=%d settings=%d",
        templates,
        defaultInventories,
        terrain,
        enemyLootable,
        chestSpecial,
        gameSettings
    ))
else
    print("[loot-multipliers] Disabled in config.txt")
end

print("[loot-multipliers] Done.")

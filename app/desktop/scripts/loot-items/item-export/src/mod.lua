-- Item export for the app's loot filter (development tool, not shipped). Writes, into EML's export folder:
--   items.json        every ItemInfo in the registry: itemId, names (English, pt-BR), category, rarity, flags, icon
--   icons/<guid>.png  the item's inventory icon, 256x256
-- The icons are UiTextureResources named `<debugName>_icon` (a few items point at theirs with iconImage); their guid is
-- the item's objectId. Run it with EML's CLI, export only (`emm.exe run -e`), see ../README.md.

local BS, DQ = string.char(92), string.char(34)

local function json_str(s)
    s = tostring(s or "")
    local out = {}
    for i = 1, #s do
        local c = s:sub(i, i)
        local b = c:byte()
        if c == BS or c == DQ then out[#out + 1] = BS .. c
        elseif b == 10 then out[#out + 1] = BS .. "n"
        elseif b < 32 then out[#out + 1] = " "
        else out[#out + 1] = c end
    end
    return DQ .. table.concat(out) .. DQ
end

local function load_language(content)
    local buf = game.assets.get_content(game.guid.from_content_hash(content.hash)):read_data()
    local data = buf:read_resource("keen::LocaTagCollectionResourceData")
    local dict = {}
    for _, tag in ipairs(data.tags) do dict[tag.id.value] = tag.text end
    return dict
end

local loca = game.assets.get_resources_by_type("keen::LocaTagCollectionResource")[1].data
local english = load_language(loca.keenglishData)
local portuguese = {}
for _, language in ipairs(loca.languages) do
    if tostring(language.language) == "Pt_Br" then portuguese = load_language(language.data) end
end

local texturesByGuid, texturesByName = {}, {}
for _, texture in ipairs(game.assets.get_resources_by_type("keen::UiTextureResource")) do
    texturesByGuid[texture.guid] = texture
    texturesByName[string.lower(texture.data.debugName)] = texture
end

local function export_icon(texture)
    local raw = game.assets.get_content(game.guid.from_content_hash(texture.data.data.hash)):read_data()
    local img = image.decode_texture(raw, texture.data.format, texture.data.size.x, texture.data.size.y)
    io.export("icons/" .. texture.guid .. ".png", image.encode(img, "png"))
end

local registry = game.assets.get_resources_by_type("keen::ItemRegistryResource")[1].data
local rows, exported, noIcon = {}, {}, 0
for _, ref in ipairs(registry.itemRefs) do
    local ok, item = pcall(game.assets.get_resource, ref, "keen::ItemInfo")
    if not ok or not item then goto continue end
    local d = item.data

    local texture = texturesByGuid[tostring(d.iconImage)] or texturesByName[string.lower(d.debugName .. "_icon")]
    local icon = ""
    if texture then
        if exported[texture.guid] == nil then
            local done, err = pcall(export_icon, texture)
            exported[texture.guid] = done
            if not done then print("[item-export] icon failed for " .. d.debugName .. ": " .. tostring(err)) end
        end
        if exported[texture.guid] then icon = texture.guid end
    end
    if icon == "" then noIcon = noIcon + 1 end

    local flags = {}
    for _, flag in ipairs(d.flags) do flags[#flags + 1] = json_str(flag) end
    local name = game.guid.hash(d.name)
    rows[#rows + 1] = string.format(
        '{"id":%d,"debug":%s,"en":%s,"pt":%s,"category":%s,"rarity":%s,"flags":[%s],"icon":%s}',
        d.itemId.value, json_str(d.debugName), json_str(english[name]), json_str(portuguese[name]), json_str(d.category),
        json_str(d.rarity), table.concat(flags, ","), json_str(icon))

    ::continue::
end

io.export("items.json", "[\n" .. table.concat(rows, ",\n") .. "\n]\n")
print(string.format("[item-export] items=%d noIcon=%d", #rows, noIcon))

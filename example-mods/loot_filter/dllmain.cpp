#include "pch.h"

#include <shroudtopia.h>
#include <memory_utils.h>
#include <algorithm>
#include <cstring>
#include <mutex>
#include <unordered_set>
#include <vector>

// Loot filter: decides which items the game's "Coletar Saque Espalhado Automaticamente" (auto-collect loose loot)
// picks up. shroudtopia.json, mods.loot_filter:
//   mode   "deny"  picks up everything but `items` (default)
//          "allow" picks up only `items`
//   items  ItemInfo itemIds (the app's loot filter panel writes them)
// Manual pickup (the interact key) is never filtered. Changes apply with the game open.
//
// How the game does it (build 1273043): the ECS system "loot_pickup_in_range" runs over the loot entities that have
// DynamicIsPlayerInRange, PickupItem, DestroyOnLoot and PickupItemZone, and once a second per entity calls the
// pickup attempt with the entity's components; that attempt is only called from there (manual pickup,
// "loot_pickup_item", has a twin of it). Its third argument holds the PickupItem component pointer at +8, whose
// first u32 is the itemId. The mod points that one call at a function that returns "nothing picked up" for a
// filtered item and calls the game's attempt otherwise. The system ignores the result and tries again a second later.

ModMetaData metaData = {
	"loot_filter",
	"Chooses what auto-collect loose loot picks up.",
	"1.0",
	"Shroudtopia",
	"0.0.1",
	true,
	true,
};

static size_t GameImageSize()
{
	auto base = reinterpret_cast<const uint8_t*>(GetModuleHandle(NULL));
	auto dos = reinterpret_cast<const IMAGE_DOS_HEADER*>(base);
	auto nt = reinterpret_cast<const IMAGE_NT_HEADERS64*>(base + dos->e_lfanew);
	return nt->OptionalHeader.SizeOfImage;
}

using PickupAttempt = uint8_t* (*)(uint8_t* result, void* context, uintptr_t components, void* a4, void* a5, void* a6, uint32_t player, char retry);

static PickupAttempt original = nullptr;
static std::mutex filterLock;
static std::unordered_set<uint32_t> filterItems;
static bool filterAllow = false;

static uint8_t* FilteredPickupAttempt(uint8_t* result, void* context, uintptr_t components, void* a4, void* a5, void* a6, uint32_t player, char retry)
{
	const uint32_t* pickupItem = components ? *reinterpret_cast<uint32_t* const*>(components + 8) : nullptr;
	if (pickupItem)
	{
		bool skip;
		{
			std::lock_guard<std::mutex> lock(filterLock);
			skip = filterItems.count(*pickupItem) ? !filterAllow : filterAllow;
		}
		if (skip)
		{
			// the attempt's own "nothing to pick up" result (0x1f)
			result[0] = 0x1f;
			*reinterpret_cast<uint32_t*>(result + 4) = 0;
			return result;
		}
	}
	return original(result, context, components, a4, a5, a6, player, retry);
}

class LootFilterMod : public Mod
{
	uintptr_t callSite = 0;     // the E8 call to the pickup attempt in loot_pickup_in_range
	int32_t originalRel = 0;
	uint8_t* stub = nullptr;    // near the call: jmp [rip]; FilteredPickupAttempt
	std::vector<uint64_t> lastItems;
	std::string lastMode;

	void log(ModContext* modContext, const std::string& msg) { modContext->Log(std::string(metaData.name).append(": ").append(msg).c_str()); }

	// The pickup attempt starts like its manual-pickup twin; at +0x179 it copies its fourth argument's fields in
	// another order (mov rax,[rbx+10h]; mov [rbp-50h],rax; mov rax,[rbx+30h]), which tells them apart.
	static uintptr_t FindPickupAttempt(const uint8_t* image, size_t size)
	{
		const uint8_t head[] = { 0x4C, 0x89, 0x4C, 0x24, 0x20, 0x55, 0x53, 0x56, 0x57, 0x41, 0x57, 0x48, 0x8D, 0xAC, 0x24, 0xC0, 0x8D, 0xFF, 0xFF, 0xB8, 0x40, 0x73, 0x00, 0x00, 0xE8 };
		const uint8_t tail[] = { 0x48, 0x2B, 0xE0, 0x48, 0x8B, 0xF1, 0x49, 0x8B, 0xD9, 0x49, 0x8B, 0x48, 0x38, 0x49, 0x8B, 0xF8, 0x4C, 0x8B, 0xFA };
		const uint8_t mark[] = { 0x48, 0x8B, 0x43, 0x10, 0x48, 0x89, 0x45, 0xB0, 0x48, 0x8B, 0x43, 0x30 };
		uintptr_t found = 0;
		for (size_t s = 0x1000; s + 0x200 < size; ++s)
		{
			if (std::memcmp(image + s, head, sizeof(head)) != 0 || std::memcmp(image + s + 29, tail, sizeof(tail)) != 0)
				continue;
			if (std::memcmp(image + s + 0x179, mark, sizeof(mark)) != 0)
				continue;
			if (found)
				return 0; // not unique: this build is not the one the mod knows
			found = (uintptr_t)(image + s);
		}
		return found;
	}

	// The one call to it (from loot_pickup_in_range).
	static uintptr_t FindCall(const uint8_t* image, size_t size, uintptr_t target)
	{
		uintptr_t found = 0;
		for (size_t s = 0x1000; s + 5 < size; ++s)
		{
			if (image[s] != 0xE8)
				continue;
			int32_t rel;
			std::memcpy(&rel, image + s + 1, 4);
			if ((uintptr_t)(image + s + 5) + rel != target)
				continue;
			if (found)
				return 0;
			found = (uintptr_t)(image + s);
		}
		return found;
	}

	bool pointCall(uintptr_t to)
	{
		int32_t rel = (int32_t)((intptr_t)to - (intptr_t)(callSite + 5));
		return Mem::WriteToReadOnlyMemory((LPVOID)(callSite + 1), &rel, sizeof(rel));
	}

public:
	void Load(ModContext* modContext)
	{
		auto image = reinterpret_cast<const uint8_t*>(GetModuleHandle(NULL));
		const size_t size = GameImageSize();
		uintptr_t attempt = FindPickupAttempt(image, size);
		if (!attempt)
		{
			log(modContext, "pickup attempt not found (game updated?), not loaded");
			return;
		}
		callSite = FindCall(image, size, attempt);
		if (!callSite)
		{
			log(modContext, "pickup attempt call not found, not loaded");
			return;
		}
		std::memcpy(&originalRel, (void*)(callSite + 1), 4);
		original = reinterpret_cast<PickupAttempt>(attempt);

		stub = (uint8_t*)Mem::AllocateMemoryNearAddress((LPVOID)callSite, 0x1000);
		if (!stub || (intptr_t)stub - (intptr_t)callSite > INT32_MAX || (intptr_t)callSite - (intptr_t)stub > INT32_MAX)
		{
			log(modContext, "no memory near the call, not loaded");
			stub = nullptr;
			return;
		}
		const uint8_t jmp[] = { 0xFF, 0x25, 0x00, 0x00, 0x00, 0x00 }; // jmp qword [rip]
		std::memcpy(stub, jmp, sizeof(jmp));
		const uintptr_t fn = (uintptr_t)&FilteredPickupAttempt;
		std::memcpy(stub + sizeof(jmp), &fn, sizeof(fn));
		log(modContext, "found the auto-collect pickup");
	}

	void Unload(ModContext* modContext)
	{
		if (active)
			Deactivate(modContext);
	}

	void Activate(ModContext* modContext)
	{
		if (!stub)
			return;
		Update(modContext);
		active = pointCall((uintptr_t)stub);
		log(modContext, active ? "activated" : "failed to activate");
	}

	void Deactivate(ModContext* modContext)
	{
		if (callSite)
			Mem::WriteToReadOnlyMemory((LPVOID)(callSite + 1), &originalRel, sizeof(originalRel));
		active = false;
		log(modContext, "deactivated");
	}

	void Update(ModContext* modContext)
	{
		const char* modKey = metaData.name.c_str();
		std::string mode = modContext->config.GetString(modKey, "mode", "deny");
		std::vector<uint64_t> items = modContext->config.GetUint64Array(modKey, "items", {});
		if (mode == lastMode && items == lastItems)
			return;
		{
			std::lock_guard<std::mutex> lock(filterLock);
			filterAllow = mode == "allow";
			filterItems.clear();
			for (uint64_t id : items)
				filterItems.insert((uint32_t)id);
		}
		lastMode = mode;
		lastItems = items;
		log(modContext, std::string(filterAllow ? "only picks up " : "leaves on the ground ").append(std::to_string(items.size())).append(" item(s)"));
	}

	ModMetaData GetMetaData() {
		return metaData;
	}
};

extern "C" __declspec(dllexport) Mod* CreateModInstance() {
	return new LootFilterMod();
}

BOOL APIENTRY DllMain(HMODULE hModule,
	DWORD  ul_reason_for_call,
	LPVOID lpReserved
)
{
	return TRUE;
}

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
//   debug  true logs each pickup attempt (item, whether it was automatic, kept or skipped), up to 300 lines
// Manual pickup (the interact key) is never filtered. Changes apply with the game open.
//
// How the game does it (build 1273043). Two ECS systems hand loot to a player, each through its own "pickup attempt"
// function (twins: same code, the arguments copied in another order), called from one place each:
// - "loot_pickup_item" (InteractionOfferRuntime, PickupItem): goes through the interaction requests on the loot.
//   A request carries a byte that, when set, skips the check that the player is aiming at this loot: auto-collect
//   sets it, the interact key does not. At the call, rsi points at that byte.
// - "loot_pickup_in_range" (DynamicIsPlayerInRange, PickupItem, PickupItemZone): loot that is taken by walking into
//   it, tried once a second. All of it is automatic.
// Both attempts get the PickupItem component pointer at +8 of their third argument (first u32: the itemId). The mod
// points each call at a stub: the interact one first copies the request's byte into the attempt's result buffer
// (byte 1, which the attempt never writes), then jumps to a function that returns the attempt's own "nothing to
// pick up" result (0x1f) for a filtered item and calls the game's attempt otherwise. The systems ignore that result
// and try again later.

ModMetaData metaData = {
#ifndef LOOT_FILTER_NAME
#define LOOT_FILTER_NAME "loot_filter"
#endif
	LOOT_FILTER_NAME,
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

static PickupAttempt originalInRange = nullptr;
static PickupAttempt originalInteract = nullptr;
static std::mutex filterLock;
static std::unordered_set<uint32_t> filterItems;
static bool filterAllow = false;
static ModContext* logContext = nullptr;
static bool filterDebug = false;
static int debugLines = 0;

// Whether an automatic pickup of the attempt's item should be skipped (logs it when debugging).
static bool Skip(const char* path, uintptr_t components, bool automatic, uint32_t player)
{
	const uint32_t* pickupItem = components ? *reinterpret_cast<uint32_t* const*>(components + 8) : nullptr;
	if (!pickupItem)
		return false;
	std::lock_guard<std::mutex> lock(filterLock);
	const bool skip = automatic && (filterItems.count(*pickupItem) ? !filterAllow : filterAllow);
	if (filterDebug && logContext && debugLines < 300)
	{
		++debugLines;
		logContext->Log(std::string(metaData.name).append(": ").append(path).append(" item ").append(std::to_string(*pickupItem))
			.append(automatic ? " auto" : " manual").append(skip ? " skipped" : " kept").append(" player ").append(std::to_string(player)).c_str());
	}
	return skip;
}

static uint8_t* Nothing(uint8_t* result)
{
	result[0] = 0x1f;
	*reinterpret_cast<uint32_t*>(result + 4) = 0;
	return result;
}

static uint8_t* FilteredInRange(uint8_t* result, void* context, uintptr_t components, void* a4, void* a5, void* a6, uint32_t player, char retry)
{
	if (Skip("in-range", components, true, player))
		return Nothing(result);
	return originalInRange(result, context, components, a4, a5, a6, player, retry);
}

static uint8_t* FilteredInteract(uint8_t* result, void* context, uintptr_t components, void* a4, void* a5, void* a6, uint32_t player, char retry)
{
	if (Skip("interact", components, result[1] != 0, player))
		return Nothing(result);
	return originalInteract(result, context, components, a4, a5, a6, player, retry);
}

// One redirected call: the E8 at callSite goes to a stub near it, which jumps to the filter.
struct CallHook
{
	uintptr_t callSite = 0;
	int32_t originalRel = 0;
	uint8_t* stub = nullptr;

	bool prepare(uintptr_t site, const uint8_t* prelude, size_t preludeSize, uintptr_t fn)
	{
		uint8_t* mem = (uint8_t*)Mem::AllocateMemoryNearAddress((LPVOID)site, 0x1000);
		if (!mem || (intptr_t)mem - (intptr_t)site > INT32_MAX || (intptr_t)site - (intptr_t)mem > INT32_MAX)
			return false;
		const uint8_t jmp[] = { 0xFF, 0x25, 0x00, 0x00, 0x00, 0x00 }; // jmp qword [rip]
		std::memcpy(mem, prelude, preludeSize);
		std::memcpy(mem + preludeSize, jmp, sizeof(jmp));
		std::memcpy(mem + preludeSize + sizeof(jmp), &fn, sizeof(fn));
		callSite = site;
		std::memcpy(&originalRel, (void*)(site + 1), 4);
		stub = mem;
		return true;
	}

	bool apply()
	{
		int32_t rel = (int32_t)((intptr_t)stub - (intptr_t)(callSite + 5));
		return stub && Mem::WriteToReadOnlyMemory((LPVOID)(callSite + 1), &rel, sizeof(rel));
	}

	void undo()
	{
		if (stub)
			Mem::WriteToReadOnlyMemory((LPVOID)(callSite + 1), &originalRel, sizeof(originalRel));
	}
};

class LootFilterMod : public Mod
{
	CallHook inRange, interact;
	std::vector<uint64_t> lastItems;
	std::string lastMode;

	void log(ModContext* modContext, const std::string& msg) { modContext->Log(std::string(metaData.name).append(": ").append(msg).c_str()); }

	// The two attempts start alike; at +0x179 they copy their fourth argument's fields in different orders:
	// in-range "mov rax,[rbx+10h]; mov [rbp-50h],rax; mov rax,[rbx+30h]", interact "mov rax,[rbx+18h]; ...; [rbx+38h]".
	static uintptr_t FindAttempt(const uint8_t* image, size_t size, const uint8_t* mark)
	{
		const uint8_t head[] = { 0x4C, 0x89, 0x4C, 0x24, 0x20, 0x55, 0x53, 0x56, 0x57, 0x41, 0x57, 0x48, 0x8D, 0xAC, 0x24, 0xC0, 0x8D, 0xFF, 0xFF, 0xB8, 0x40, 0x73, 0x00, 0x00, 0xE8 };
		const uint8_t tail[] = { 0x48, 0x2B, 0xE0, 0x48, 0x8B, 0xF1, 0x49, 0x8B, 0xD9, 0x49, 0x8B, 0x48, 0x38, 0x49, 0x8B, 0xF8, 0x4C, 0x8B, 0xFA };
		uintptr_t found = 0;
		for (size_t s = 0x1000; s + 0x200 < size; ++s)
		{
			if (std::memcmp(image + s, head, sizeof(head)) != 0 || std::memcmp(image + s + 29, tail, sizeof(tail)) != 0)
				continue;
			if (std::memcmp(image + s + 0x179, mark, 12) != 0)
				continue;
			if (found)
				return 0; // not unique: this build is not the one the mod knows
			found = (uintptr_t)(image + s);
		}
		return found;
	}

	// The one call to it.
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

public:
	void Load(ModContext* modContext)
	{
		logContext = modContext;
		auto image = reinterpret_cast<const uint8_t*>(GetModuleHandle(NULL));
		const size_t size = GameImageSize();

		const uint8_t inRangeMark[] = { 0x48, 0x8B, 0x43, 0x10, 0x48, 0x89, 0x45, 0xB0, 0x48, 0x8B, 0x43, 0x30 };
		const uint8_t interactMark[] = { 0x48, 0x8B, 0x43, 0x18, 0x48, 0x89, 0x45, 0xB0, 0x48, 0x8B, 0x43, 0x38 };
		uintptr_t a = FindAttempt(image, size, inRangeMark), b = FindAttempt(image, size, interactMark);
		uintptr_t aCall = a ? FindCall(image, size, a) : 0, bCall = b ? FindCall(image, size, b) : 0;

		// the interact call must come after the request byte's check, "cmp byte [rsi],0"
		bool rsiIsFlag = false;
		for (uintptr_t p = bCall > 0x80 ? bCall - 0x80 : 0; bCall && p < bCall; ++p)
			if (*(uint8_t*)p == 0x80 && *(uint8_t*)(p + 1) == 0x3E && *(uint8_t*)(p + 2) == 0x00)
				rsiIsFlag = true;

		const uint8_t none[] = { 0x90 };
		const uint8_t copyFlag[] = { 0x8A, 0x06, 0x88, 0x41, 0x01 }; // mov al,[rsi]; mov [rcx+1],al
		if (aCall && inRange.prepare(aCall, none, sizeof(none), (uintptr_t)&FilteredInRange))
			originalInRange = reinterpret_cast<PickupAttempt>(a);
		if (bCall && rsiIsFlag && interact.prepare(bCall, copyFlag, sizeof(copyFlag), (uintptr_t)&FilteredInteract))
			originalInteract = reinterpret_cast<PickupAttempt>(b);
		log(modContext, std::string("in-range pickup ").append(inRange.stub ? "found" : "not found")
			.append(", interact pickup ").append(interact.stub ? "found" : "not found (game updated?)"));
	}

	void Unload(ModContext* modContext)
	{
		if (active)
			Deactivate(modContext);
	}

	void Activate(ModContext* modContext)
	{
		if (!inRange.stub && !interact.stub)
			return;
		Update(modContext);
		const bool a = !inRange.stub || inRange.apply();
		const bool b = !interact.stub || interact.apply();
		active = a && b;
		log(modContext, active ? "activated" : "failed to activate");
	}

	void Deactivate(ModContext* modContext)
	{
		inRange.undo();
		interact.undo();
		active = false;
		log(modContext, "deactivated");
	}

	void Update(ModContext* modContext)
	{
		const char* modKey = metaData.name.c_str();
		std::string mode = modContext->config.GetString(modKey, "mode", "deny");
		std::vector<uint64_t> items = modContext->config.GetUint64Array(modKey, "items", {});
		filterDebug = modContext->config.GetBool(modKey, "debug", false);
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

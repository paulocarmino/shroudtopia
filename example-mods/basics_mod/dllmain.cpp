#include "pch.h"

#include <shroudtopia.h>
#include <memory_utils.h>
#include <cstring>
#include <vector>

#define LOG_CLASS(msg) modContext->Log(std::string(typeid(*this).name()).append(msg).c_str())

ModMetaData metaData = {
	"basics",
	"Enables legacy shroudtopia mods.",
	"1.1-2026",
	"s0T7x",
	"0.0.1",
	true,
	true,
};

// Restrict signature scans to the game image. Scanning past it can match
// copies of the pattern elsewhere (heap, this DLL's own rdata).
static size_t GameImageSize()
{
	auto base = reinterpret_cast<const uint8_t*>(GetModuleHandle(NULL));
	auto dos = reinterpret_cast<const IMAGE_DOS_HEADER*>(base);
	auto nt = reinterpret_cast<const IMAGE_NT_HEADERS64*>(base + dos->e_lfanew);
	return nt->OptionalHeader.SizeOfImage;
}

// Signatures verified on Steam build 25988036 (October 2026)
class _Mod {
public:
	ModContext* modContext;
	_Mod(ModContext* modContext) : modContext(modContext) {}
	virtual ~_Mod() {}  // Add a virtual destructor to enable polymorphism

	bool active = false;
	Mem::Detour* mod = nullptr;

	void activate()
	{
		if (!mod) {
			LOG_CLASS(" pattern not found, not activated");
			return;
		}
		mod->activate();
		active = mod->active;
		if (active)
			LOG_CLASS("Activated");
		else
			LOG_CLASS("Failed to activate");
	}

	// Deactivates the mod by restoring the original code.
	void deactivate()
	{
		if (mod) mod->deactivate();
		active = false;
		LOG_CLASS("Deactivated");
	}
};

// Stamina is an int attribute looked up by hash (mov edx, 0x04B6AA8B). Every
// place that spends it does "add rX, rY; sub dword [rX + idx*4], reg" shortly
// after the lookup. NOP those subtractions; regeneration and resets still run.
// Found with hardware watchpoints on build 25988036 (sprint, jump/attack paths).
class NoStaminaLoss : public _Mod
{
	std::vector<Mem::Patch*> patches;
public:
	NoStaminaLoss(ModContext* modContext) : _Mod(modContext)
	{
		auto image = reinterpret_cast<const uint8_t*>(GetModuleHandle(NULL));
		const size_t size = GameImageSize();
		const uint8_t lookup[] = { 0xBA, 0x8B, 0xAA, 0xB6, 0x04 }; // mov edx, StaminaHash
		for (size_t s = 0x1000; s + 0x90 < size; ++s)
		{
			if (std::memcmp(image + s, lookup, sizeof(lookup)) != 0)
				continue;
			for (size_t i = s + sizeof(lookup); i < s + 0x80; ++i)
			{
				size_t j = i;
				if (image[j] >= 0x40 && image[j] <= 0x4F) ++j;             // optional REX
				const bool sub = image[j] == 0x29 && (image[j + 1] & 0xC7) == 0x04 && // sub [sib], r32
					(image[j + 2] >> 6) == 2 && (image[j + 2] & 7) != 5;            // scale 4, no disp
				const bool addBefore = image[i - 3] >= 0x48 && image[i - 3] <= 0x4F &&
					image[i - 2] == 0x03 && (image[i - 1] >> 6) == 3;              // add r64, r64
				if (!sub || !addBefore)
					continue;
				uint8_t nop3[] = { 0x0F, 0x1F, 0x00 };
				uint8_t nop4[] = { 0x0F, 0x1F, 0x40, 0x00 };
				const size_t len = j + 3 - i;
				patches.push_back(new Mem::Patch((uintptr_t)(image + i), len == 4 ? nop4 : nop3, len));
				break;
			}
		}
		LOG_CLASS(std::string(" found ").append(std::to_string(patches.size())).append(" stamina spend sites").c_str());
	}

	~NoStaminaLoss()
	{
		for (auto p : patches) { p->undo(); delete p; }
	}

	void activate()
	{
		if (patches.empty()) {
			LOG_CLASS(" no stamina spend sites found, not activated");
			return;
		}
		for (auto p : patches) p->apply();
		active = true;
		LOG_CLASS("Activated");
	}

	void deactivate()
	{
		for (auto p : patches) p->undo();
		active = false;
		LOG_CLASS("Deactivated");
	}
};

class NoFallDamage : public _Mod
{
public:
	NoFallDamage(ModContext* modContext) : _Mod(modContext)
	{
		// We scan for 7 bytes now to make sure we have room for the 5-byte Jump
		// Original instructions:
		// 45 89 0C 88    - mov [r8+rcx*4], r9d (4 bytes)
		// 48 8B CB       - mov rcx, rbx        (3 bytes)
		// Total: 7 bytes
		const char* pattern = "\x45\x89\x0C\x88\x48\x8B\xCB";
		const char* mask = "xxxxxxx";

		uintptr_t baseAddress = (uintptr_t)GetModuleHandle(NULL);
		uintptr_t address = Mem::FindPattern(pattern, mask, baseAddress, GameImageSize());

		if (address)
		{
			uint8_t modCode[] = {
				0x45, 0x01, 0x0C, 0x88,             // add [r8+rcx*4], r9d (Your Cheat)
				0x48, 0x8B, 0xCB,                   // mov rcx, rbx        (The instruction we had to overwrite)
				0xE9, 0x00, 0x00, 0x00, 0x00        // jmp return
			};

			// We use 2 NOPs because: 
			// 5 (Jump size) + 2 (NOPs) = 7 (Total bytes overwritten)
			mod = new Mem::Detour(address, modCode, sizeof(modCode), false, 2);

			// Calculate jump back to address + 7
			mod->shellcode->updateValue<uint32_t>(
				sizeof(modCode) - 4,
				(uint32_t)(address + 7) - ((uint32_t)((uintptr_t)mod->shellcode->data->address + sizeof(modCode)))
			);
		}
		else
		{
			LOG_CLASS(" NoFallDamage: Pattern not found!");
		}
	}
};

class InfiniteItemUse : public _Mod
{
public:
	InfiniteItemUse(ModContext* modContext) : _Mod(modContext)
	{
		const char* pattern = "\x41\x29\x6E\x04";
		uintptr_t baseAddress = (uintptr_t)GetModuleHandle(NULL);
		uintptr_t address = Mem::FindPattern(pattern, "xxxx", baseAddress, GameImageSize());
		if (address)
		{
			uint8_t modCode[] = {
				0x41, 0xBF, 0x00, 0x00, 0x00, 0x00, 
				0xBD, 0x00, 0x00, 0x00, 0x00, 
				0xE9, 0x00, 0x00, 0x00, 0x00
			};

			mod = new Mem::Detour(address, modCode, sizeof(modCode), false, 2);

			// Calculate the jump address for detour
			mod->shellcode->updateValue<uint32_t>(
				sizeof(modCode) - 4, (uint32_t)(mod->patch->data->address + mod->patch->data->size)
				- ((uint32_t)((uintptr_t)mod->shellcode->data->address
					+ mod->shellcode->data->size))
			);
		}
	}

	// Not found on build 25988036: the item consumption code no longer
	// contains this signature, so activation is skipped by _Mod.
};

class NoResourceCost : public _Mod
{
public:
	NoResourceCost(ModContext* modContext) : _Mod(modContext)
	{
		// Pattern matching the AOB scan for the original code in the target process.
		// Function prologue; the 6th argument ([rsp+30] at entry) is the amount needed.
		// Frame size and the [rbp+..] displacement change between builds.
		const char* pattern = "\x48\x89\x5C\x24\x08\x48\x89\x74\x24\x18\x48\x89\x7C\x24\x20\x48\x89\x54\x24\x10\x55\x41\x54\x41\x55\x41\x56\x41\x57\x48\x8D\xAC\x24\x00\x00\x00\x00\x48\x81\xEC\x00\x00\x00\x00\x44\x8B\xBD\x00\x00\x00\x00\x4C\x8B\xE1\x48\x8B\x0A";
		const char* mask = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx????xxx????xxx????xxxxxx";

		// Base address of the module (the game).
		uintptr_t baseAddress = (uintptr_t)GetModuleHandle(NULL);

		uintptr_t address = Mem::FindPattern(pattern, mask, baseAddress, GameImageSize());
		if (address)
		{
			uint8_t modCode[] = {
				0xC7, 0x44, 0x24, 0x30, 0x00, 0x00, 0x00, 0x00,	// - mov[rsp + 30],00000000
				0x48, 0x89, 0x5C, 0x24, 0x08,					// - mov[rsp + 08],rbx
				0xE9, 0x00, 0x00, 0x00, 0x00					// - jmp return
			};

			// Creating the detour by replacing the original code with our custom modCode.
			mod = new Mem::Detour(address, modCode, sizeof(modCode), false, 0);

			// Calculate the jump address and update the shellcode.
			mod->shellcode->updateValue<uint32_t>(
				sizeof(modCode) - 4, (uint32_t)(mod->patch->data->address + mod->patch->data->size)
				- ((uint32_t)((uintptr_t)mod->shellcode->data->address
					+ mod->shellcode->data->size))
			);
		}
		else
		{
			LOG_CLASS(std::string("Address not found: Base Address: ").append(std::to_string(baseAddress)).c_str());
		}
	}
};

class UnlockBlueprints : public _Mod
{
public:
	UnlockBlueprints(ModContext* modContext) : _Mod(modContext)
	{
		if (modContext->game.isServer) return;
		// Pattern matching the AOB scan for the original code in the target process.
		// mov r9,rcx; mov rbx,[rcx+..]; je ..; mov r11d,[rdx]; lea rax,[rbx-1]
		const char* pattern = "\x4C\x8B\xC9\x48\x8B\x99\x00\x00\x00\x00\x74\x00\x44\x8B\x1A\x48\x8D\x43\xFF";
		const char* mask = "xxxxxx????x?xxxxxxx";

		// Base address of the module (the game).
		uintptr_t baseAddress = (uintptr_t)GetModuleHandle(NULL);

		uintptr_t address = Mem::FindPattern(pattern, mask, baseAddress, GameImageSize());
		if (address)
		{
			uint8_t modCode[] = {
				0x41,0xBB,0x82,0x53,0x2B,0x54,	// - mov r11d,542B5382
				0x48,0x8D,0x43,0xFF,			// - lea rax,[rbx - 01]
				0xE9,0x27,0x9F,0x89,0x00		// - jmp enshrouded.exe + 879F36

			};

			// Creating the detour by replacing the original code with our custom modCode.
			mod = new Mem::Detour(address + 12, modCode, sizeof(modCode), false, 2);

			// Calculate the jump address and update the shellcode.
			mod->shellcode->updateValue<uint32_t>(
				sizeof(modCode) - 4, (uint32_t)(mod->patch->data->address + mod->patch->data->size)
				- ((uint32_t)((uintptr_t)mod->shellcode->data->address
					+ mod->shellcode->data->size))
			);
		}
		else
		{
			LOG_CLASS(std::string("Address not found: Base Address: ").append(std::to_string(baseAddress)).c_str());
		}
	}

	void activate()
	{
		if (modContext->game.isServer) {
			LOG_CLASS("Cannot activate on server");
			return;
		}
		if (!mod) {
			LOG_CLASS(" pattern not found, not activated");
			return;
		}
		mod->activate();
		active = mod->active;
		if (active)
			LOG_CLASS("Activated");
		else
			LOG_CLASS("Failed to activate");
	}

};

class InfItemSplit : public _Mod
{
public:
	InfItemSplit(ModContext* modContext) : _Mod(modContext)
	{
		// Pattern matching the AOB scan for the original code in the target process.
		// sub [rsi+4],r12d; jmp short ItemHandling; xorps xmm0,xmm0
		const char* pattern = "\x44\x29\x66\x04\xEB\x00\x0F\x57\xC0";
		const char* mask = "xxxxx?xxx";

		// Base address of the module (the game).
		uintptr_t baseAddress = (uintptr_t)GetModuleHandle(NULL);

		uintptr_t address = Mem::FindPattern(pattern, mask, baseAddress, GameImageSize());
		uintptr_t handlingAddress = address ? address + 6 + *(int8_t*)(address + 5) : 0;
		if (address)
		{
			uint8_t modCode[] = {
				0x83, 0x6E, 0x04, 0x00,			// - sub dword ptr[rsi + 04],00
				0xE9, 0xCA, 0x75, 0x71, 0x00,	// - jmp ItemHandling
				0xE9, 0x00, 0x76, 0x71, 0x00	// - jmp enshrouded_server.exe + 70760E
			};

			// Creating the detour by replacing the original code with our custom modCode.
			mod = new Mem::Detour(address, modCode, sizeof(modCode), false, 1);

			mod->shellcode->updateValue<uint32_t>(
				sizeof(modCode) - 9,
				(uint32_t)(handlingAddress - ((uintptr_t)mod->shellcode->data->address
					+ sizeof(modCode) - 5))
			);

			mod->shellcode->updateValue<uint32_t>(
				sizeof(modCode) - 4, (uint32_t)(mod->patch->data->address + mod->patch->data->size)
				- ((uint32_t)((uintptr_t)mod->shellcode->data->address
					+ mod->shellcode->data->size))
			);
		}
		else
		{
			LOG_CLASS(std::string("Address not found: Base Address: ").append(std::to_string(baseAddress)).c_str());
		}
	}
};

class BasicsMod : public Mod
{
	NoStaminaLoss* noStaminaLoss;
	NoFallDamage* noFallDamage;
	InfiniteItemUse* infiniteItemUse;
	NoResourceCost* noResourceCost;
	UnlockBlueprints* unlockBlueprints;
	InfItemSplit* infItemSplit;

public:
	void Load(ModContext* modContext)
	{
		noStaminaLoss = new NoStaminaLoss(modContext);
		noFallDamage = new NoFallDamage(modContext);
		noResourceCost = new NoResourceCost(modContext);
		infiniteItemUse = new InfiniteItemUse(modContext);
		unlockBlueprints = new UnlockBlueprints(modContext);
		infItemSplit = new InfItemSplit(modContext);
	}

	void Unload(ModContext* modContext) {
		delete noStaminaLoss;
		delete noFallDamage;
		delete noResourceCost;
		delete infiniteItemUse;
		delete unlockBlueprints;
		delete infItemSplit;
	}

	void Activate(ModContext* modContext) {
		active = true;
		modContext->Log(std::string().append(metaData.name).append(" activated").c_str());
	}

	void Deactivate(ModContext* modContext) {
		if (noStaminaLoss->active) noStaminaLoss->deactivate();
		if (noFallDamage->active) noFallDamage->deactivate();
		if (noResourceCost->active) noResourceCost->deactivate();
		if (infiniteItemUse->active) infiniteItemUse->deactivate();
		if (unlockBlueprints->active) unlockBlueprints->deactivate();
		if (infItemSplit->active) infItemSplit->deactivate();
		active = false;
		modContext->Log(std::string().append(metaData.name).append(" deactivated").c_str());
	}

	void Update(ModContext* modContext) {
		const char* modKey = metaData.name.c_str();
		bool noStaminaLossEnabled = modContext->config.GetBool(modKey, "no_stamina_loss", false);
		if (noStaminaLossEnabled && !noStaminaLoss->active)
			noStaminaLoss->activate();
		else if (!noStaminaLossEnabled && noStaminaLoss->active)
			noStaminaLoss->deactivate();

		bool noFallDamageEnabled = modContext->config.GetBool(modKey, "no_fall_damage", false);
		if (noFallDamageEnabled && !noFallDamage->active)
			noFallDamage->activate();
		else if (!noFallDamageEnabled && noFallDamage->active)
			noFallDamage->deactivate();


		bool infiniteItemUseEnabled = modContext->config.GetBool(modKey, "_inf_item_use", false);
		if (infiniteItemUseEnabled && !infiniteItemUse->active)
			infiniteItemUse->activate();
		else if (!infiniteItemUseEnabled && infiniteItemUse->active)
			infiniteItemUse->deactivate();

		bool noResourceCostEnabled = modContext->config.GetBool(modKey, "_no_resource_cost", false);
		if (noResourceCostEnabled && !noResourceCost->active)
			noResourceCost->activate();
		else if (!noResourceCostEnabled && noResourceCost->active)
			noResourceCost->deactivate();


		if(modContext->game.isClient) {
			bool unlockBlueprintsEnabled = modContext->config.GetBool(modKey, "_unlock_blueprints", false);
			if (unlockBlueprintsEnabled && !unlockBlueprints->active)
				unlockBlueprints->activate();
			else if (!unlockBlueprintsEnabled && unlockBlueprints->active)
				unlockBlueprints->deactivate();
		}


		bool infItemSplitEnabled = modContext->config.GetBool(modKey, "_inf_item_split", false);
		if (infItemSplitEnabled && !infItemSplit->active)
			infItemSplit->activate();
		else if (!infItemSplitEnabled && infItemSplit->active)
			infItemSplit->deactivate();
	}

	ModMetaData GetMetaData() {
		return metaData;
	}
};

extern "C" __declspec(dllexport) Mod* CreateModInstance() {
	return new BasicsMod();
}

BOOL APIENTRY DllMain(HMODULE hModule,
	DWORD  ul_reason_for_call,
	LPVOID lpReserved
)
{
	switch (ul_reason_for_call)
	{
	case DLL_PROCESS_ATTACH:
	case DLL_THREAD_ATTACH:
	case DLL_THREAD_DETACH:
	case DLL_PROCESS_DETACH:
		break;
	}
	return TRUE;
}


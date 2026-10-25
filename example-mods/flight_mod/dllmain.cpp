#include "pch.h"

#include <shroudtopia.h>
#include <memory_utils.h>

ModMetaData metaData = {
    "Flight Mod",
    "Enables free glider flight.",
    "1.0",
    "s0T7x",
    "0.0.1",
    true,
    true,
};


// Signature for GameVersion (SVN) 637515
class FlightMod : public Mod
{
    Mem::Detour* mod = nullptr;
    Mem::Patch* gamepadPitch = nullptr;
public:
    void Load(ModContext* modContext)
    {
        // F3 0F 10 05 ?? ?? ?? ?? F2 0F 11 4C 24 60
        const char* pattern = "\xF3\x0F\x10\x05\x00\x00\x00\x00\xF2\x0F\x11\x4C";
        const char* mask = "xxxx????xxxx";

        uintptr_t baseAddress = (uintptr_t)GetModuleHandle(NULL);
        uintptr_t address = Mem::FindPattern(pattern, mask, baseAddress, 0x1000000); // Scan 16MB

        if (address)
        {
            uint8_t modCode[] = {
                0xF3, 0x0F, 0x10, 0x05, 0x05, 0x00, 0x00, 0x00, // movss xmm0, [GlideData]
                0xE9, 0x00, 0x00, 0x00, 0x00,
                0xc3, 0xf5, 0xc8, 0xbf };
            mod = new Mem::Detour(address, modCode, sizeof(modCode), false, 3);
            mod->shellcode->updateValue<uint32_t>(9, (uint32_t)(mod->patch->data->address + mod->patch->data->size) - ((uint32_t)((uintptr_t)mod->shellcode->data->address + mod->shellcode->data->size - 4)));
        }

        // Glider pitch has two control modes (a byte on the controller): 0 adds pitch speed * input * dt to the
        // current pitch, so it stays where you leave it (mouse); 1 sets pitch = input * pi/2, so it falls back the
        // moment the stick is centred (gamepad), and a gamepad player has to hold the stick to keep climbing.
        // Send mode 1 down the mode 0 path: test al,al; je incremental; cmp ecx,1; jne ..; [movd xmm8,edx -> jmp].
        const char* pitchPattern = "\x84\xC0\x74\x00\x83\xF9\x01\x75\x00\x66\x44\x0F\x6E\xC2\xF3\x44\x0F\x59\x05\x00\x00\x00\x00\xEB\x00\xF3\x44\x0F\x10\x87";
        const char* pitchMask = "xxx?xxxx?xxxxxxxxxx????x?xxxxx";
        uintptr_t pitch = Mem::FindPattern(pitchPattern, pitchMask, baseAddress, 0x1000000);
        if (pitch)
        {
            const uintptr_t incremental = pitch + 4 + *reinterpret_cast<const int8_t*>(pitch + 3);
            uint8_t jmp[] = { 0xEB, (uint8_t)(int8_t)(incremental - (pitch + 9 + 2)) };
            gamepadPitch = new Mem::Patch(pitch + 9, jmp, sizeof(jmp));
        }
        else
        {
            modContext->Log("Flight Mod: gamepad pitch pattern not found, gamepad keeps absolute pitch");
        }
    }

    void Unload(ModContext* modContext) {
        delete mod;
        mod = nullptr;
        if (gamepadPitch) { gamepadPitch->undo(); delete gamepadPitch; gamepadPitch = nullptr; }
    }

    void Activate(ModContext* modContext) {
        if (!mod) {
            modContext->Log("Flight Mod: glide pattern not found, not activated");
            return;
        }
        active = mod->activate();
        if (gamepadPitch && gamepadPitch->apply())
            modContext->Log("Flight Mod: gamepad pitch now holds like the mouse");
        modContext->Log(std::string().append(metaData.name).append(" activated").c_str());
    }

    void Deactivate(ModContext* modContext) {
        if (mod) mod->deactivate();
        if (gamepadPitch) gamepadPitch->undo();
        active = false;
        modContext->Log(std::string().append(metaData.name).append(" deactivated").c_str());
    }

    void Update(ModContext* modContext) { }

    ModMetaData GetMetaData() {
        return metaData;
    }
};

extern "C" __declspec(dllexport) Mod* CreateModInstance() {
    return new FlightMod();
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


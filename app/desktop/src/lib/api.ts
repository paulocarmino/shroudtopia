// The page's access to the Rust side (Tauri commands and the `app-state` event), with a mock outside Tauri (a plain
// browser, only to look at the layout: `?sync=friend&upd=avail&tab=world&phase=main`).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { mockState, mockSlots } from "./mock";

export type Tone = "ok" | "live" | "friend" | "busy" | "warn" | "muted";
export interface Action { label: string; kind: "play" | "join" | "none" | "update"; icon: string | null; cls: string; disabled: boolean }
export interface Progress { pct: number; text: string }
export interface Tag { cls: string; text: string }
export interface HistRow { who: "me" | "friend"; initial: string; name: string; when: string; size: string; tags: Tag[] }
export interface Choice { initial: string; t: string; s: string }
export interface WorldView {
  name: string; aside: string; state: string; tone: Tone; status: string; line: string;
  action: Action | null; progress: Progress | null; note: { text: string; icon: string } | null;
  choices: { mine: Choice; theirs: Choice } | null; history: HistRow[]; file: string; store: "steam" | "local"; members: number;
}
export interface Person { nick: string; initial: string; me: boolean; sub: string; dot: string }
export interface Snapshot {
  phase: "login" | "setup" | "main"; player: Person | null; gameRunning: boolean; online: boolean;
  world: WorldView | null; people: Person[]; error: string | null;
  login: { waiting: boolean; mismatch: string | null; steamNick: string | null; error: string | null };
}
export type OptValue =
  | { kind: "bool"; value: boolean }
  | { kind: "number"; value: number; min: number; max: number; step: number }
  | { kind: "other"; text: string };
export type Opt = { key: string; label: string } & OptValue;
export interface ModView { key: string; name: string; sub: string; active: boolean; options: Opt[] }
export interface UpdateView { state: "ok" | "avail" | "blocked" | "installing"; name: string; sub: string; action: Action | null; progress: Progress | null; changes: string[]; badge: boolean; appVersion: string }
export interface UiState {
  sync: Snapshot | null; gameRunning: boolean;
  mods: { items: ModView[]; error: string | null };
  update: UpdateView;
  settings: { gameFolder: string | null; autostart: boolean; appVersion: string; steamFound: boolean; localStore: string | null; steamStore: string | null };
}
export interface SlotRow { store: "steam" | "local"; index: number; file: string; title: string; sub: string }
export interface Slots { slots: SlotRow[]; freeSteam: string | null; freeLocal: string | null; steam: boolean; local: boolean }
export interface Invite { code: string; expiresAt: string }

export const IS_TAURI = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (IS_TAURI) return invoke<T>(cmd, args);
  return mockCall(cmd, args) as T;
}

let mock: UiState | null = null;
function mockCall(cmd: string, args?: Record<string, unknown>): unknown {
  mock ??= mockState();
  if (cmd === "get_state") return mock;
  if (cmd === "list_slots") return mockSlots();
  if (cmd === "share" || cmd === "new_invite") return { code: "ENSH-7KQ2", expiresAt: new Date(Date.now() + 864e5).toISOString() };
  if (cmd === "redeem") return { name: "Mundo do bang", sub: "de rodrigao_br · 1,8 MB · foi para o espaço world2.ksd", initial: "R" };
  if (cmd === "set_mod" && args) {
    const m = mock.mods.items.find((x) => x.key === args.modKey);
    if (m && args.key === "active") m.active = args.value as boolean;
    const o = m?.options.find((x) => x.key === args.key);
    if (o && o.kind !== "other") (o as { value: unknown }).value = args.value;
  }
  return null;
}

export const api = {
  state: () => call<UiState>("get_state"),
  play: () => call<void>("play"),
  join: () => call<{ note?: string } | null>("join"),
  resolve: (keepMine: boolean) => call<void>("resolve", { keepMine }),
  loginStart: () => call<void>("login_start"),
  loginCancel: () => call<void>("login_cancel"),
  loginAcceptMismatch: () => call<void>("login_accept_mismatch"),
  logout: () => call<void>("logout"),
  slots: () => call<Slots>("list_slots"),
  share: (store: string, slot: number, into: string, name: string) => call<Invite>("share", { store, slot, into, name }),
  newInvite: () => call<Invite>("new_invite"),
  redeem: (code: string, into: string) => call<{ name: string; sub: string; initial: string }>("redeem", { code, into }),
  setMod: (modKey: string, key: string, value: unknown) => call<void>("set_mod", { modKey, key, value }),
  updateCheck: () => call<void>("update_check"),
  updateApply: () => call<void>("update_apply"),
  setAutostart: (on: boolean) => call<void>("set_autostart", { on }),
  chooseGameFolder: () => call<void>("choose_game_folder"),
  moveStore: (into: string) => call<void>("move_store", { into }),
};

export async function onState(cb: (s: UiState) => void): Promise<UnlistenFn> {
  if (IS_TAURI) return listen<UiState>("app-state", (e) => cb(e.payload));
  return () => {};
}

export async function windowAction(what: "minimize" | "maximize" | "close") {
  if (!IS_TAURI) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const w = getCurrentWindow();
  if (what === "minimize") await w.minimize();
  else if (what === "maximize") await w.toggleMaximize();
  else await w.hide();
}

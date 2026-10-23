// Made-up state for looking at the layout in a plain browser (never in the app): the prototype's seven sync states
// and four update states, picked with `?sync=…&upd=…&phase=…`.

import type { UiState, Slots, WorldView, HistRow, Tag } from "./api";

const FR = "rodrigao_br";
const ME = "[ツ] Ｌ　Ａ　Ｒ　Ａ　Ｎ　Ｊ　Ｏ";
const t = (cls: string): Tag => ({ cls, text: { here: "neste PC", cloud: "no servidor", live: "jogando agora", friendlive: "jogando agora", warn: "versão separada" }[cls]! });
const row = (who: "me" | "friend", when: string, size: string, tags: string[]): HistRow => ({ who, initial: who === "me" ? "L" : "R", name: who === "me" ? "Você" : FR, when, size, tags: tags.map(t) });
const play = (disabled = false) => ({ label: "Jogar", kind: "play" as const, icon: "play", cls: "primary", disabled });

function world(s: string, id = "w1", name = "CopyWithGabe", fr = FR): WorldView {
  const base = { id, name, with: `com ${fr}`, file: id === "w1" ? "world2.ksd" : "world3.ksd", store: "steam" as const, members: 2, updatedAt: new Date(Date.now() - (id === "w1" ? 7 : 95) * 6e4).toISOString(), progress: null, note: null, choices: null, state: s };
  switch (s) {
    case "me":
      return { ...base, tone: "live", status: "Você está hospedando", line: "Desde 21:12 · último autosave enviado às 22:31", action: { label: "Enshrouded aberto", kind: "none", icon: null, cls: "", disabled: true }, history: [row("me", "hoje, desde 21:12", "1,8 MB", ["live"]), row("friend", "ontem, 21:48 – 23:40", "1,8 MB", ["cloud"]), row("me", "ontem, 14:02 – 17:15", "1,7 MB", [])] };
    case "friend":
      return { ...base, tone: "friend", status: `${FR} está hospedando`, line: "Desde 20:47 · o mundo fica com ele até fechar o jogo", action: { label: "Entrar pelo Steam", kind: "join", icon: "join", cls: "friend", disabled: false }, note: { text: "Abrir o mundo daqui fica travado enquanto ele joga, para ninguém perder progresso.", icon: "info" }, history: [row("friend", "hoje, desde 20:47", "—", ["friendlive"]), row("friend", "ontem, 21:48 – 23:40", "1,8 MB", ["here", "cloud"]), row("me", "ontem, 14:02 – 17:15", "1,7 MB", [])] };
    case "behind":
      return { ...base, tone: "busy", status: `Baixando a sessão do ${FR}`, line: `${FR} jogou hoje, 14:10 – 16:02`, progress: { pct: 62, text: "1,1 de 1,8 MB" }, action: play(true), history: [row("friend", "hoje, 14:10 – 16:02", "1,8 MB", ["cloud"]), row("me", "ontem, 21:48 – 23:40", "1,8 MB", ["here"]), row("friend", "ontem, 14:02 – 17:15", "1,7 MB", [])] };
    case "up":
      return { ...base, tone: "busy", status: "Enviando sua sessão", line: "Você jogou hoje, 21:12 – 23:05", progress: { pct: 40, text: "0,7 de 1,8 MB" }, action: play(true), history: [row("me", "hoje, 21:12 – 23:05", "1,8 MB", ["here"]), row("friend", "hoje, 14:10 – 16:02", "1,8 MB", ["cloud"]), row("me", "ontem, 21:48 – 23:40", "1,8 MB", [])] };
    case "diverged":
      return { ...base, tone: "warn", status: "Vocês jogaram separados", line: "Os dois abriram o mundo sem o outro saber. Escolha qual segue.", action: null, choices: { mine: { initial: "L", t: "Seguir com a sua", s: "hoje, 19:00 – 20:30 · neste PC" }, theirs: { initial: "R", t: `Seguir com a do ${FR}`, s: "hoje, 18:40 – 20:10 · no servidor" } }, history: [row("me", "hoje, 19:00 – 20:30", "1,8 MB", ["here", "warn"]), row("friend", "hoje, 18:40 – 20:10", "1,8 MB", ["cloud", "warn"]), row("me", "ontem, 21:48 – 23:40", "1,8 MB", [])] };
    case "offline":
      return { ...base, tone: "muted", status: "Sem conexão com o servidor", line: "Dá para jogar normal. Sua sessão sobe quando a conexão voltar.", action: play(), note: { text: `Se o ${FR} jogar nesse meio-tempo, vocês vão ter que escolher qual versão segue.`, icon: "alert" }, history: [row("me", "ontem, 21:48 – 23:40", "1,8 MB", ["here"]), row("friend", "ontem, 14:02 – 17:15", "1,7 MB", [])] };
    default:
      return { ...base, tone: "ok", status: "Em dia", line: `${FR} jogou por último · ontem, 23:40 · 1h52 de sessão`, action: play(), history: [row("friend", "ontem, 21:48 – 23:40", "1,8 MB", ["here", "cloud"]), row("me", "ontem, 14:02 – 17:15", "1,7 MB", []), row("friend", "dom, 20:30 – 23:58", "1,7 MB", [])] };
  }
}

function update(u: string): UiState["update"] {
  const changes = ["Voo: volta a funcionar depois do pulo em água", "Básicos: opção de esconder a barra de stamina", "App: aviso quando seu amigo começa a hospedar"];
  const upd = (label: string, cls: string, disabled = false) => ({ label, kind: "update" as const, icon: null, cls, disabled });
  const appVersion = "0.1.0";
  if (u === "avail") return { state: "avail", name: "Versão 0.4.3 disponível", sub: "Você está na 0.4.2 · 3,4 MB", action: upd("Atualizar", "primary"), progress: null, changes, badge: true, appVersion };
  if (u === "blocked") return { state: "blocked", name: "Versão 0.4.3 pronta", sub: "Feche o Enshrouded para instalar. Instala sozinho quando fechar.", action: upd("Atualizar", "primary", true), progress: null, changes, badge: true, appVersion };
  if (u === "installing") return { state: "installing", name: "Instalando 0.4.3", sub: "", action: null, progress: { pct: 70, text: "Copiando os mods para a pasta do jogo" }, changes, badge: false, appVersion };
  return { state: "ok", name: "Mods v0.4.2", sub: "Tudo atualizado · verificado hoje, 20:15", action: upd("Verificar", ""), progress: null, changes: [], badge: false, appVersion };
}

export function mockState(): UiState {
  const q = new URLSearchParams(typeof location !== "undefined" ? location.search : "");
  const s = q.get("sync") ?? "sync";
  const game = s === "me" || q.get("upd") === "blocked";
  const fr = s === "friend" ? ["hospedando", "friend"] : s === "offline" ? ["—", ""] : ["online", "ok"];
  return {
    gameRunning: game,
    sync: {
      phase: (q.get("phase") as "login" | "setup" | "main") ?? "main",
      player: { nick: ME, initial: "L", me: true, sub: "online", dot: "ok" },
      gameRunning: game,
      online: s !== "offline",
      // `?worlds=1` for the single card; two by default (the second one in sync, with another friend)
      worlds: q.get("worlds") === "1" ? [world(s)] : [world(s), world(q.get("sync2") ?? "sync", "w2", "Testando1.0", "† THE VOID †")],
      people: [
        { nick: ME, initial: "L", me: true, sub: s === "me" ? "hospedando" : "online", dot: s === "me" ? "live" : "ok" },
        { nick: FR, initial: "R", me: false, sub: fr[0], dot: fr[1] },
      ],
      error: null,
      login: { waiting: q.get("login") === "waiting", mismatch: q.get("login") === "mismatch" ? ME : null, steamNick: ME, error: null },
    },
    mods: {
      error: null,
      items: [
        { key: "basics", name: "Básicos", sub: "Stamina, queda e custo de recursos · 4 opções", active: true, options: [
          { key: "no_stamina_loss", label: "Sem gasto de stamina", kind: "bool", value: true },
          { key: "no_fall_damage", label: "Sem dano de queda", kind: "bool", value: true },
          { key: "_no_resource_cost", label: "Construir e usar itens sem gastar", kind: "bool", value: false },
          { key: "hide_stamina_gauge", label: "Esconder a barra de stamina", kind: "bool", value: true },
        ], restart: false },
        { key: "Flight Mod", name: "Voo", sub: "Planar sem perder altura", active: true, options: [], restart: false },
        { key: "FirstPersonView", name: "Primeira pessoa", sub: "Câmera nos olhos do personagem · 1 opção", active: false, options: [{ key: "camera_distance", label: "Distância da câmera", kind: "number", value: -0.15, min: -0.5, max: 0.5, step: 0.05 }], restart: false },
        { key: "max-stack", name: "Max Stack", sub: "Itens empilham 5× mais, até 999", active: true, options: [], restart: true },
        { key: "twitch-drops", name: "Drops da Twitch", sub: "Quem tem um drop da Twitch resgata todos", active: true, options: [], restart: true },
      ],
    },
    update: update(q.get("upd") ?? "ok"),
    settings: { gameFolder: "E:\\SteamLibrary\\steamapps\\common\\Enshrouded", autostart: true, appVersion: "0.1.0", steamFound: true, localStore: "C:\\Users\\voce\\Saved Games\\keen games\\Enshrouded\\default", steamStore: "C:\\Program Files (x86)\\Steam\\userdata\\1\\1203620\\remote" },
  };
}

export function mockSlots(): Slots {
  return {
    slots: [
      { store: "steam", index: 0, file: "world0.ksd", title: "CopyWithGabe", sub: "1,7 MB · jogado em 17 out, 00:16 · Steam", shared: true },
      { store: "steam", index: 1, file: "world1.ksd", title: "Testando1.0", sub: "145 KB · jogado em 20 out, 22:40 · Steam" },
      { store: "local", index: 0, file: "world0.ksd", title: "CopyWithGabe", sub: "1,7 MB · jogado em 17 out, 00:16 · Local" },
    ],
    freeSteam: "world2.ksd",
    freeLocal: "world1.ksd",
    steam: true,
    local: true,
  };
}

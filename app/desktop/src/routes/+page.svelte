<script lang="ts">
  // The window: the prototype's sidebar layout (closed choice C) over the real state from Rust (`app-state`).
  import { onMount } from "svelte";
  import { ICON } from "$lib/icons";
  import { api, onState, windowAction, IS_TAURI, type UiState } from "$lib/api";
  import WorldCard from "$lib/WorldCard.svelte";
  import Mods from "$lib/Mods.svelte";
  import Update from "$lib/Update.svelte";
  import FirstRun from "$lib/FirstRun.svelte";
  import Note from "$lib/Note.svelte";

  let s = $state<UiState | null>(null);
  let tab = $state<"world" | "mods" | "update">("world");
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** what "Entrar pelo Steam" says when Steam did not start the game by itself */
  let notice = $state<string | null>(null);
  /** the code screen stays up after sharing (or "Gerar código") until "Pronto", though the phase is already main */
  let codeScreen = $state<string | null>(null);
  /** the first-run flow stays up after a redeem until "Pronto" */
  let holdFirstRun = $state(false);
  /** "Compartilhar outro mundo" / "Tenho um código" under the cards: the pairing flow at that step */
  let pairing = $state<"slot" | "paste" | null>(null);
  /** the world a code from "Convidar amigo" is for */
  let codeFor = $state<string | null>(null);

  onMount(() => {
    const q = new URLSearchParams(location.search);
    if (!IS_TAURI && q.get("tab")) tab = q.get("tab") as typeof tab;
    api.state().then((x) => (s = x));
    const un = onState((x) => (s = x));
    return () => {
      un.then((f) => f());
    };
  });

  async function act(f: () => Promise<unknown>) {
    busy = true;
    error = null;
    try {
      await f();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
      s = await api.state();
    }
  }

  const sync = $derived(s?.sync ?? null);
  const phase = $derived(sync?.phase ?? "login");
  const worlds = $derived(sync?.worlds ?? []);
  // the Mundo nav's "!" when any world needs the player (two versions to choose from)
  const attention = $derived(worlds.some((x) => x.state === "diverged"));
  const game = $derived(s?.gameRunning ?? false);
  const updBadge = $derived(s?.update.badge ?? false);
  // while pairing, the flow keeps its own steps (the code, the redeemed world) after the phase turns main
  $effect(() => {
    if (phase === "setup") holdFirstRun = true;
  });
  // the hint is done once the game is up
  $effect(() => {
    if (game) notice = null;
  });
  const firstRun = $derived(phase !== "main" || codeScreen !== null || holdFirstRun || pairing !== null);

  function onaction(world: string, kind: string) {
    if (kind === "play") act(() => api.play(world));
    else if (kind === "join")
      act(async () => {
        notice = (await api.join(world))?.note ?? null;
      });
  }
  function onupdate() {
    if (!s) return;
    act(() => (s!.update.state === "ok" ? api.updateCheck() : api.updateApply()));
  }
  async function invite(world: string) {
    await act(async () => {
      codeFor = worlds.find((x) => x.id === world)?.name ?? null;
      codeScreen = (await api.newInvite(world)).code;
    });
  }
</script>

{#snippet titlebar()}
  <div class="titlebar" data-tauri-drag-region>
    <span class="drag" data-tauri-drag-region></span>
    <button aria-label="Minimizar" onclick={() => windowAction("minimize")}>{@html ICON.min}</button>
    <button aria-label="Maximizar" onclick={() => windowAction("maximize")}>{@html ICON.max}</button>
    <button class="close" aria-label="Fechar para a bandeja" onclick={() => windowAction("close")}>{@html ICON.close}</button>
  </div>
{/snippet}
{#snippet brand()}
  <div class="brand"><img class="st-logo" src="/art/shroudtopia-logo.svg" alt="Shroudtopia" /></div>
{/snippet}
{#snippet chip()}
  <span class="chip"><span class="dot" class:live={game}></span>{game ? "Enshrouded aberto" : "Enshrouded fechado"}</span>
{/snippet}

<div class="win skin-st">
  {#if !s}
    {@render titlebar()}
  {:else}
    {@render titlebar()}
    <div class="split">
      <aside class="rail">
        {@render brand()}
        <button class="nav" role="tab" aria-selected={tab === "world"} onclick={() => (tab = "world")}>{@html ICON.globe}Mundo{#if attention}<span class="badge" style="background:var(--warn)">!</span>{/if}</button>
        <button class="nav" role="tab" aria-selected={tab === "mods"} onclick={() => (tab = "mods")}>{@html ICON.puzzle}Mods</button>
        <button class="nav" role="tab" aria-selected={tab === "update"} onclick={() => (tab = "update")}>{@html ICON.download}Atualização{#if updBadge}<span class="badge">1</span>{/if}</button>
        {#if sync?.people.length}
          <div class="rail-foot">
            <span class="k">No mundo</span>
            {#each sync.people as p}
              <div class="person">
                <span class="avatar {p.me ? 'me' : 'friend'}">{p.initial}</span>
                <span class="who"><span class="nick" title={p.nick}>{p.nick}</span><span class="muted">{p.sub}</span></span>
                <span class="dot {p.dot}"></span>
              </div>
            {/each}
          </div>
        {/if}
      </aside>
      <div class="st-main" style="display:flex;flex-direction:column;min-height:0">
        <div class="st-scene" aria-hidden="true"><img src="/art/mist-valley.jpg" alt="" /></div>
        <div class="app-head" style="padding-top:0;border-bottom:0;padding-bottom:4px" data-tauri-drag-region>
          <span class="spacer"></span>{@render chip()}
          <button class="icon-btn" title="Configurações" aria-label="Configurações" onclick={() => (tab = "update")}>{@html ICON.gear}</button>
        </div>
        <div class="app-body">
          {#if tab === "world" && firstRun}
            <!-- login and pairing live in the Mundo tab, so Mods and Atualização never wait on them -->
            <FirstRun
              {s}
              embedded
              start={codeScreen ? "code" : (pairing ?? "choose")}
              invite={codeScreen}
              inviteFor={codeFor}
              ondone={() => {
                codeScreen = null;
                holdFirstRun = false;
                pairing = null;
              }}
            />
          {:else if tab === "world"}
            <div class="sec">
              <div class="sec-title"><h2>{worlds.length > 1 ? "Mundos compartilhados" : "Mundo compartilhado"}</h2></div>
              {#each worlds as w (w.id)}
                <WorldCard {w} {busy} onaction={(kind) => onaction(w.id, kind)} onresolve={(keepMine) => act(() => api.resolve(w.id, keepMine))} />
              {/each}
              <div class="more-worlds">
                <button class="btn" disabled={busy} onclick={() => (pairing = "slot")}>{@html ICON.plus}Compartilhar outro mundo</button>
                <button class="btn" disabled={busy} onclick={() => (pairing = "paste")}>{@html ICON.link}Tenho um código</button>
              </div>
              {#if sync?.error}<Note text={sync.error} icon="alert" error />{/if}
            </div>
          {:else if tab === "mods"}
            <div class="sec">
              <div class="sec-title"><h2>Mods</h2><span class="aside">muda na hora, mesmo com o jogo aberto</span></div>
              <Mods mods={s.mods.items} error={s.mods.error} onset={(m, k, v) => act(() => api.setMod(m, k, v))} />
            </div>
          {:else}
            <Update
              {s}
              {busy}
              {onupdate}
              onfolder={() => act(() => api.chooseGameFolder())}
              onautostart={(on) => act(() => api.setAutostart(on))}
              onstore={(world, into) => act(() => api.moveStore(world, into))}
              oninvite={invite}
              onlogout={() => act(() => api.logout())}
            />
          {/if}
          {#if notice && tab === "world"}<Note text={notice} icon="info" />{/if}
          {#if error}<Note text={error} icon="alert" error />{/if}
        </div>
      </div>
    </div>
  {/if}
</div>

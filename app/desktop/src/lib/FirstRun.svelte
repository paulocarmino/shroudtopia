<script lang="ts">
  // First run: "Entrar com a Steam" (Steam OpenID, CONTRACT.md), then the prototype's pairing flow: choose, then
  // which world (and where it lives: Steam or Local) or the code pasted. Both end on the cards: a shared world's
  // code opens on its card, a redeemed world is just there.
  import { ICON } from "./icons";
  import { api, type UiState, type Slots, type SlotRow } from "./api";
  import Status from "./Status.svelte";
  import Note from "./Note.svelte";
  import { onMount } from "svelte";

  let { s, start = "choose", embedded = false, ondone, onshared, onredeemed }: {
    s: UiState; start?: string; embedded?: boolean; ondone: () => void;
    onshared: (world: string | null, code: string) => Promise<void>; onredeemed: (world: string, text: string) => Promise<void>;
  } = $props();

  const sync = $derived(s.sync);
  const login = $derived(sync?.phase === "login");
  let step = $state(start);
  let slots = $state<Slots | null>(null);
  let pick = $state<SlotRow | null>(null);
  let into = $state<"steam" | "local">("steam");
  let pasted = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** opened from the cards ("Compartilhar outro mundo", "Tenho um código"): Voltar goes back to them */
  const fromCards = start === "slot" || start === "paste";
  onMount(() => {
    if (start === "slot") toSlots();
    else if (start === "paste") toPaste();
  });

  const stepIdx = $derived(step === "choose" ? 0 : 1);

  async function run(f: () => Promise<void>) {
    busy = true;
    error = null;
    try {
      await f();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function toSlots() {
    await run(async () => {
      slots = await api.slots();
      pick = slots.slots.find((r) => !r.shared) ?? null;
      into = pick?.store ?? (slots.steam ? "steam" : "local");
      step = "slot";
    });
  }
  function toPaste() {
    into = "steam";
    step = "paste";
    api.slots().then((x) => {
      slots = x;
      if (!x.steam) into = "local";
    });
  }
  async function share() {
    if (!pick) return;
    await run(async () => {
      // the shared world keeps the name the game shows for it
      const inv = await api.share(pick!.store, pick!.index, into, pick!.title);
      await onshared(inv.worldId ?? null, inv.code);
    });
  }
  async function redeem() {
    await run(async () => {
      const r = await api.redeem(pasted, into);
      await onredeemed(r.worldId, `${r.name} chegou: ${r.sub}.`);
    });
  }
  const freeSlot = $derived(slots ? (into === "steam" ? slots.freeSteam : slots.freeLocal) : null);
</script>

<div class="fr" class:embedded>
  {#if login}
    <div class="steps"><i class="on"></i><i></i><i></i></div>
    <h2>Entrar com a Steam</h2>
    <p class="lead">O app confirma quem é você pela sua conta da Steam. O navegador abre, você entra lá e volta para cá.</p>
    {#if sync?.login.steamNick}
      <div class="card"><div class="row"><span class="avatar me">{sync.player?.initial ?? "?"}</span><div class="txt"><div class="name">{sync.login.steamNick}</div><div class="sub">Conta da Steam deste PC</div></div></div></div>
    {:else}
      <Note text="O Steam deste PC não foi achado. Dá para entrar mesmo assim, mas os mundos do Steam não aparecem." icon="alert" />
    {/if}
    {#if sync?.login.mismatch}
      <div class="card"><div class="row"><span class="avatar">!</span><div class="txt"><div class="name">O navegador entrou em outra conta da Steam</div><div class="sub">Não é a {sync.login.steamNick} deste PC. Saia dela no navegador e entre de novo, ou continue com ela.</div></div><Status tone="warn" text="conta diferente" /></div></div>
      <div class="fr-actions"><button class="btn" disabled={busy} onclick={() => run(() => api.loginAcceptMismatch())}>Continuar assim</button><button class="btn primary" disabled={busy} onclick={() => run(() => api.loginStart())}>Entrar de novo</button></div>
    {:else if sync?.login.waiting}
      <div class="card"><div class="row"><span class="avatar">{@html ICON.link.replace("<svg", '<svg style="width:12px"')}</span><div class="txt"><div class="name">Termine o login no navegador</div><div class="sub">Volte para cá quando a página disser que está pronto</div></div><Status tone="busy" text="aguardando" /></div></div>
      <div class="fr-actions"><button class="btn" onclick={() => run(() => api.loginCancel())}>Cancelar</button></div>
    {:else}
      <div class="fr-actions"><button class="btn primary big" disabled={busy} onclick={() => run(() => api.loginStart())}>Entrar com a Steam</button></div>
    {/if}
    {#if sync?.login.error}<Note text={sync.login.error} icon="alert" error />{/if}
  {:else if step === "choose"}
    <div class="steps">{#each [0, 1] as i}<i class:on={i <= stepIdx}></i>{/each}</div>
    <h2>Jogar junto com alguém</h2>
    <p class="lead">Quem abrir o jogo primeiro hospeda. Quando fecha, o mundo sobe e fica pronto no PC do outro.</p>
    <div class="pick">
      <button class="opt-card" disabled={busy} onclick={toSlots}>{@html ICON.plus}<span class="t">Compartilhar um mundo meu</span><span class="s">Você escolhe o mundo e manda um código para o amigo</span></button>
      <button class="opt-card" disabled={busy} onclick={toPaste}>{@html ICON.link}<span class="t">Tenho um código</span><span class="s">Um amigo já compartilhou o mundo dele com você</span></button>
    </div>
  {:else if step === "slot"}
    <div class="steps">{#each [0, 1] as i}<i class:on={i <= stepIdx}></i>{/each}</div>
    <h2>Qual mundo?</h2>
    <p class="lead">Mundos deste PC, achados nas pastas de saves do jogo (Steam e local).</p>
    {#if slots && slots.slots.length}
      <div class="card slots">
        {#each slots.slots as r}
          <div class="row" class:taken={r.shared} role="radio" aria-checked={pick === r} aria-disabled={r.shared} tabindex={r.shared ? -1 : 0} onclick={() => { if (!r.shared) { pick = r; into = r.store; } }} onkeydown={(e) => e.key === "Enter" && !r.shared && (pick = r)}>
            <span class="radio" class:on={pick === r}></span>
            <div class="txt"><div class="name">{r.title} <span style="color:var(--muted);font-weight:400">· {r.file}</span></div><div class="sub">{r.sub}</div></div>
            {#if r.shared}<span class="tag cloud">já compartilhado</span>{/if}
          </div>
        {/each}
      </div>
    {:else if slots}
      <Note text="Nenhum mundo achado neste PC. Crie um no jogo e volte aqui." icon="alert" />
    {/if}
    <div class="row" style="padding:0">
      <div class="txt"><div class="sub">Onde o mundo compartilhado fica neste PC</div></div>
      <div class="seg small">
        <button aria-pressed={into === "steam"} disabled={!slots?.steam} onclick={() => (into = "steam")}>Steam</button>
        <button aria-pressed={into === "local"} disabled={!slots?.local} onclick={() => (into = "local")}>Local</button>
      </div>
    </div>
    <div class="fr-actions"><button class="btn" onclick={() => (fromCards ? ondone() : (step = "choose"))}>Voltar</button><button class="btn primary" disabled={busy || !pick} onclick={share}>Compartilhar</button></div>
  {:else if step === "paste"}
    <div class="steps">{#each [0, 1] as i}<i class:on={i <= stepIdx}></i>{/each}</div>
    <h2>Cole o código</h2>
    <p class="lead">O mundo é baixado para um espaço livre de save neste PC. Nenhum mundo seu é tocado.</p>
    <div class="field">{@html ICON.link.replace("<svg", '<svg style="width:16px;color:var(--muted)"')}<input bind:value={pasted} aria-label="Código" placeholder="ENSH-XXXX" /></div>
    <div class="row" style="padding:0">
      <div class="txt"><div class="sub">Guardar em {into === "steam" ? "Jogos salvos: Steam" : "jogos salvos locais"}{freeSlot ? ` · vai para o espaço ${freeSlot}` : ""}</div></div>
      <div class="seg small">
        <button aria-pressed={into === "steam"} disabled={slots ? !slots.steam : false} onclick={() => (into = "steam")}>Steam</button>
        <button aria-pressed={into === "local"} disabled={slots ? !slots.local : false} onclick={() => (into = "local")}>Local</button>
      </div>
    </div>
    <div class="fr-actions"><button class="btn" onclick={() => (fromCards ? ondone() : (step = "choose"))}>Voltar</button><button class="btn primary" disabled={busy || pasted.trim().length < 4} onclick={redeem}>{@html ICON.download}{busy ? "Baixando…" : "Baixar e entrar"}</button></div>
  {/if}
  {#if error}<Note text={error} icon="alert" error />{/if}
</div>

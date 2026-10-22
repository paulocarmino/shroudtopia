<script lang="ts">
  // The "Atualização" page: the update card (four states of the prototype) and "Este PC".
  import { ICON } from "./icons";
  import type { UiState } from "./api";
  import Toggle from "./Toggle.svelte";
  import Btn from "./Btn.svelte";
  import Note from "./Note.svelte";
  import ProgressLine from "./ProgressLine.svelte";

  let { s, busy, onupdate, onfolder, onautostart, onstore, onlogout }: {
    s: UiState; busy: boolean;
    onupdate: () => void; onfolder: () => void; onautostart: (on: boolean) => void;
    onstore: (world: string, into: string) => void; onlogout: () => void;
  } = $props();
  const u = $derived(s.update);
  const avail = $derived(u.state === "avail" || u.state === "blocked");
  const worlds = $derived(s.sync?.worlds ?? []);
</script>

<div class="sec">
  <div class="card upd-wrap" class:avail style={avail ? "border-color:var(--brand)" : ""}>
    <div class="upd" class:avail style="border-radius:var(--radius)">
      <span class="upd-ico">{@html ICON[u.state === "ok" ? "refresh" : "download"]}</span>
      <div class="txt">
        <div class="name">{u.name}</div>
        {#if u.sub}<div class="sub">{u.sub}</div>{/if}
        {#if u.progress}<ProgressLine p={u.progress} />{/if}
      </div>
      {#if u.action}<Btn action={u.action} {busy} onclick={onupdate} />{/if}
    </div>
    {#if u.state !== "ok" && u.changes.length}
      <ul class="changelog">{#each u.changes as c}<li>{c}</li>{/each}</ul>
    {/if}
  </div>
</div>

<div class="sec">
  <div class="sec-title"><h2>Este PC</h2></div>
  <div class="card">
    <div class="row">
      <div class="txt"><div class="name">Pasta do jogo</div><div class="sub">{s.settings.gameFolder ?? "Não achada"}</div></div>
      <button class="btn" onclick={onfolder}>Trocar</button>
    </div>
    <div class="row">
      <div class="txt"><div class="name">Iniciar com o Windows</div><div class="sub">Fica na bandeja e cuida do sync sozinho</div></div>
      <Toggle on={s.settings.autostart} label="Iniciar com o Windows" onchange={onautostart} />
    </div>
    {#each worlds as world (world.id)}
      <!-- per world: where it lives on this PC (its invite is on its card) -->
      <div class="row">
        <div class="txt"><div class="name">Onde o mundo fica</div><div class="sub">{world.name} · {world.file} · {world.store === "steam" ? "jogos salvos do Steam" : "jogos salvos locais"}</div></div>
        <div class="seg small">
          <button aria-pressed={world.store === "steam"} disabled={busy || !s.settings.steamStore} onclick={() => onstore(world.id, "steam")}>Steam</button>
          <button aria-pressed={world.store === "local"} disabled={busy || !s.settings.localStore} onclick={() => onstore(world.id, "local")}>Local</button>
        </div>
      </div>
    {/each}
    {#if s.sync?.player}
      <div class="row">
        <span class="avatar me">{s.sync.player.initial}</span>
        <div class="txt"><div class="name">{s.sync.player.nick}</div><div class="sub">Conta Steam deste app</div></div>
        <button class="btn danger-ghost" disabled={busy} onclick={onlogout}>Sair</button>
      </div>
    {/if}
    <div class="row"><div class="txt"><div class="name">App v{s.settings.appVersion}</div><div class="sub">O app se atualiza junto com os mods</div></div></div>
  </div>
  {#if !s.settings.steamFound}<Note text="O Steam deste PC não foi achado: os mundos do Steam e o login ficam indisponíveis." icon="alert" />{/if}
</div>

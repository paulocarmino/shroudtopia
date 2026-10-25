<script lang="ts">
  // The Filtro de saque tab: the loot_filter mod's toggle and its panel (LootFilter), in a tab of its own instead of
  // the Mods list.
  import type { ModView } from "./api";
  import LootFilter from "./LootFilter.svelte";
  import Toggle from "./Toggle.svelte";
  import Note from "./Note.svelte";

  let { mod, error, onset }: { mod: ModView | null; error: string | null; onset: (key: string, value: unknown) => void } = $props();
</script>

<div class="sec">
  <div class="sec-title">
    <h2>Filtro de saque</h2>
    {#if mod}<Toggle on={mod.active} label="Filtro de saque" onchange={(on) => onset("active", on)} />{/if}
  </div>
  <p class="loot-lead">Escolhe o que o <b>Coletar Saque Espalhado Automaticamente</b> do jogo pega. Pegar com a tecla de interagir continua pegando tudo. Muda na hora, mesmo com o jogo aberto.</p>
  {#if error}<Note text={error} icon="alert" error />{/if}
  {#if mod?.panel}
    <div class="card loot-card">
      <LootFilter
        tall
        mode={mod.panel.values.mode as string}
        ids={mod.panel.values.items as number[]}
        disabled={!mod.active}
        onmode={(v) => onset("mode", v)}
        onitems={(v) => onset("items", v)}
      />
    </div>
  {:else if !error}
    <Note text="O filtro de saque ainda não está na pasta do jogo. Instale os mods em Atualização." />
  {/if}
</div>

<style>
  .loot-lead { margin: -2px 0 0; color: var(--muted); font-size: 13px; max-width: 72ch; }
  .loot-lead b { color: var(--ink); font-weight: 500; }
  .loot-card { padding-top: 14px; }
</style>

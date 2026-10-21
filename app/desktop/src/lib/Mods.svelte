<script lang="ts">
  // The mods list, "expande por mod" (closed choice): each mod's toggle, and its options under a chevron.
  import { ICON } from "./icons";
  import type { ModView, Opt } from "./api";
  import Toggle from "./Toggle.svelte";
  import Note from "./Note.svelte";

  let { mods, error, onset }: { mods: ModView[]; error: string | null; onset: (mod: string, key: string, value: unknown) => void } = $props();
  let open = $state<string | null>("basics");

  const num = (v: number) => v.toFixed(2).replace(".", ",");
  function step(m: ModView, o: Opt & { kind: "number" }, dir: number) {
    const v = Math.min(o.max, Math.max(o.min, Math.round((o.value + dir * o.step) * 1e6) / 1e6));
    onset(m.key, o.key, v);
  }
</script>

{#if error}<Note text={error} icon="alert" error />{/if}
{#if mods.length}
  <div class="card">
    {#each mods as m (m.key)}
      {@const isOpen = open === m.key && m.options.length > 0}
      <div class="mod">
        <div class="mod-row" class:off={!m.active}>
          <div class="txt"><div class="name">{m.name}</div>{#if m.sub}<div class="sub">{m.sub}</div>{/if}</div>
          {#if m.options.length}
            <button class="chev" aria-expanded={isOpen} aria-label="Opções de {m.name}" onclick={() => (open = open === m.key ? null : m.key)}>{@html ICON.chev}</button>
          {/if}
          <Toggle on={m.active} label={m.name} onchange={(on) => onset(m.key, "active", on)} />
        </div>
        {#if isOpen}
          <div class="opts">
            {#each m.options as o (o.key)}
              <div class="opt">
                <span class="on">{o.label}</span>
                {#if o.kind === "bool"}
                  <Toggle sm on={o.value} label={o.label} onchange={(on) => onset(m.key, o.key, on)} />
                {:else if o.kind === "number"}
                  <span class="num"><button aria-label="Menos" onclick={() => step(m, o, -1)}>−</button><span>{num(o.value)}</span><button aria-label="Mais" onclick={() => step(m, o, 1)}>+</button></span>
                {:else}
                  <span class="raw">{o.text}</span>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/each}
  </div>
{:else if !error}
  <Note text="Nenhum mod achado na pasta do jogo. Instale os mods em Atualização." />
{/if}

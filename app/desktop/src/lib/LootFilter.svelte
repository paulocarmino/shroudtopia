<script lang="ts">
  // The loot filter's panel: which items the game's "Coletar Saque Espalhado Automaticamente" picks up. The items and
  // their icons come from the game's own data (scripts/loot-items); an entry can hold several item ids (one weapon at
  // several levels), and ticking it ticks them all.
  import items from "./loot-items.json";

  type Entry = { n: string; e: string; c: string; i: string; ids: number[] };
  const ENTRIES = items as Entry[];
  const CATEGORIES: [string, string][] = [
    ["Materials", "Materiais"], ["Consumables", "Consumíveis"], ["Ammunition", "Munição"], ["Weapons", "Armas"],
    ["Equipment", "Equipamento"], ["WeaponGem", "Gemas"], ["Tools", "Ferramentas"], ["Collectibles", "Colecionáveis"],
    ["AnimalFood", "Comida de animal"], ["Instrument", "Instrumentos"], ["FishingRod", "Varas de pesca"],
  ];

  let { mode, ids, disabled = false, tall = false, onmode, onitems }: {
    mode: string; ids: number[]; disabled?: boolean; tall?: boolean; onmode: (mode: string) => void; onitems: (ids: number[]) => void;
  } = $props();

  // the window's copy, so a click shows at once; the file's list replaces it when it changes
  let picked = $state(new Set<number>());
  $effect(() => { picked = new Set(ids); });

  let cat = $state("Materials");
  let query = $state("");

  const fold = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
  const isOn = (e: Entry) => e.ids.some((id) => picked.has(id));
  const countIn = (c: string) => ENTRIES.filter((e) => e.c === c && isOn(e)).length;
  let marked = $derived(ENTRIES.filter(isOn));
  let shown = $derived.by(() => {
    const q = fold(query.trim());
    if (q) return ENTRIES.filter((e) => fold(e.n).includes(q) || fold(e.e).includes(q));
    if (cat === "marked") return marked;
    return ENTRIES.filter((e) => e.c === cat);
  });

  function toggle(e: Entry) {
    const next = new Set(picked);
    const on = isOn(e);
    for (const id of e.ids) on ? next.delete(id) : next.add(id);
    picked = next;
    onitems([...next].sort((a, b) => a - b));
  }

  function clear() {
    picked = new Set();
    onitems([]);
  }
</script>

<div class="loot" class:off={disabled} class:tall>
  <div class="loot-mode" role="radiogroup" aria-label="Modo do filtro">
    <button role="radio" aria-checked={mode === "deny"} onclick={() => onmode("deny")}>
      <span class="t">Pegar tudo, menos os marcados</span><span class="s">os marcados ficam no chão</span>
    </button>
    <button role="radio" aria-checked={mode === "allow"} onclick={() => onmode("allow")}>
      <span class="t">Pegar só os marcados</span><span class="s">o resto fica no chão</span>
    </button>
  </div>

  <div class="loot-bar">
    <input class="loot-search" type="search" placeholder="Buscar item…" bind:value={query} aria-label="Buscar item" />
    <span class="loot-count">{marked.length} {marked.length === 1 ? "marcado" : "marcados"}</span>
    {#if marked.length}<button class="link" onclick={clear}>Limpar</button>{/if}
  </div>

  {#if !query.trim()}
    <div class="seg loot-cats">
      <button aria-pressed={cat === "marked"} onclick={() => (cat = "marked")}>Marcados</button>
      {#each CATEGORIES as [key, label] (key)}
        {@const n = countIn(key)}
        <button aria-pressed={cat === key} onclick={() => (cat = key)}>{label}{#if n}<span class="n">{n}</span>{/if}</button>
      {/each}
    </div>
  {/if}

  <div class="loot-grid" class:deny={mode === "deny"}>
    {#each shown as e (e.c + e.n)}
      <button class="loot-item" aria-pressed={isOn(e)} title={e.n} onclick={() => toggle(e)}>
        <img src="/loot-icons/{e.i}.webp" alt="" loading="lazy" width="48" height="48" />
        <span class="nm">{e.n}</span>
      </button>
    {:else}
      <p class="loot-empty">{query.trim() ? "Nenhum item com esse nome." : "Nenhum item marcado ainda."}</p>
    {/each}
  </div>
</div>

<style>
  .loot { padding: 2px 16px 14px; display: grid; gap: 10px; }
  .loot.off { opacity: .5; }
  .loot.tall .loot-grid { max-height: calc(100vh - 330px); min-height: 220px; }
  .loot-mode { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .loot-mode button { text-align: left; border: 1px solid var(--border); background: var(--bg); border-radius: var(--radius-sm); padding: 8px 12px; display: grid; gap: 1px; }
  .loot-mode button:hover { border-color: var(--muted); }
  .loot-mode button[aria-checked="true"] { border-color: var(--brand); background: var(--brand-soft); }
  .loot-mode .t { font-size: 13px; font-weight: 500; }
  .loot-mode .s { font-size: 12px; color: var(--muted); }
  .loot-bar { display: flex; align-items: center; gap: 10px; }
  .loot-search { flex: 1; min-width: 0; height: 30px; padding: 0 10px; border: 1px solid var(--border); border-radius: 6px; background: var(--bg); color: var(--ink); font: inherit; font-size: 13px; }
  .loot-search:focus { outline: none; border-color: var(--brand); }
  .loot-count { font-size: 12.5px; color: var(--muted); font-variant-numeric: tabular-nums; }
  .link { border: 0; background: transparent; color: var(--brand); font-weight: 600; padding: 0; font-size: 12.5px; }
  .loot-cats .n { margin-left: 5px; font-size: 11px; color: var(--brand); font-variant-numeric: tabular-nums; }
  .loot-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(84px, 1fr)); gap: 6px; max-height: 340px; overflow-y: auto; padding-right: 2px; align-content: start; }
  .loot-item { position: relative; border: 1px solid var(--border); background: var(--bg); border-radius: var(--radius-sm); padding: 6px 4px 5px; display: grid; justify-items: center; gap: 3px; min-width: 0; }
  .loot-item:hover { border-color: var(--muted); background: var(--surface-2); }
  .loot-item img { width: 48px; height: 48px; }
  .loot-item .nm { font-size: 11px; line-height: 1.25; color: var(--muted); text-align: center; width: 100%; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
  .loot-item[aria-pressed="true"] { border-color: var(--ok); background: var(--ok-soft); }
  .loot-item[aria-pressed="true"] .nm { color: var(--ink); }
  .deny .loot-item[aria-pressed="true"] { border-color: var(--danger); background: var(--danger-soft); }
  .deny .loot-item[aria-pressed="true"] img { opacity: .55; }
  .loot-item[aria-pressed="true"]::after { content: "✓"; position: absolute; top: 3px; right: 5px; font-size: 11px; font-weight: 700; color: var(--ok); }
  .deny .loot-item[aria-pressed="true"]::after { content: "✕"; color: var(--danger); }
  .loot-empty { grid-column: 1 / -1; margin: 8px 0; color: var(--muted); font-size: 13px; }
  @media (max-width: 640px) { .loot-mode { grid-template-columns: 1fr; } }
</style>

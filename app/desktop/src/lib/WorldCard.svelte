<script lang="ts">
  // The world card, "Histórico" variant of the prototype (closed choice): name + state, the action, a transfer's
  // progress, the two choices when diverged, the sessions, and the state's note.
  import { ICON } from "./icons";
  import type { WorldView } from "./api";
  import Status from "./Status.svelte";
  import Note from "./Note.svelte";
  import Btn from "./Btn.svelte";
  import ProgressLine from "./ProgressLine.svelte";

  let { w, busy, onaction, onresolve }: { w: WorldView; busy: boolean; onaction: (kind: string) => void; onresolve: (keepMine: boolean) => void } = $props();
  const chev = ICON.chev.replace("<svg", '<svg style="width:14px;transform:rotate(-90deg);color:var(--muted)"');
</script>

<div class="card">
  <div class="w2-head">
    <div class="txt" title={w.line}><h3>{w.name}</h3><Status tone={w.tone} text={w.status} /></div>
    {#if w.state !== "diverged" && w.action}<Btn action={w.action} {busy} onclick={() => onaction(w.action!.kind)} />{/if}
  </div>
  {#if w.progress}<div style="padding:0 16px 14px"><ProgressLine p={w.progress} /></div>{/if}
  {#if w.choices}
    <div style="padding:0 16px 14px">
      <div class="choices">
        <button class="choice" disabled={busy} onclick={() => onresolve(true)}><span class="avatar me">{w.choices.mine.initial}</span><span><div class="t">{w.choices.mine.t}</div><div class="s">{w.choices.mine.s}</div></span>{@html chev}</button>
        <button class="choice" disabled={busy} onclick={() => onresolve(false)}><span class="avatar friend">{w.choices.theirs.initial}</span><span><div class="t">{w.choices.theirs.t}</div><div class="s">{w.choices.theirs.s}</div></span>{@html chev}</button>
      </div>
      <Note text="A que ficar de fora não é apagada: vai para o histórico do mundo." />
    </div>
  {/if}
  <div class="hist">
    {#each w.history as r, i}
      <div class="hist-row" class:current={i === 0}>
        <span class="avatar {r.who === 'me' ? 'me' : 'friend'}">{r.initial}</span>
        <span><b style="font-weight:500">{r.name}</b> <span class="when">· {r.when}</span></span>
        <span class="tags">{#each r.tags as t}<span class="tag {t.cls}">{t.text}</span>{/each}</span>
        <span class="size">{r.size}</span>
      </div>
    {/each}
  </div>
  {#if w.note}<div style="padding:10px 16px;border-top:1px solid var(--border)"><Note text={w.note.text} icon={w.note.icon} /></div>{/if}
</div>

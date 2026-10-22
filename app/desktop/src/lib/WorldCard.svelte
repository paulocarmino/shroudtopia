<script lang="ts">
  // The world card, "Histórico" variant of the prototype (closed choice): name + state, the action, a transfer's
  // progress, the two choices when diverged, the sessions, and the state's note. "Convidar" opens the world's code
  // right in the card, until closed. The sessions stay folded under "atualizado há 5 min" (open when diverged, where
  // they tell the two versions apart).
  import { ICON } from "./icons";
  import type { WorldView } from "./api";
  import Status from "./Status.svelte";
  import Note from "./Note.svelte";
  import Btn from "./Btn.svelte";
  import ProgressLine from "./ProgressLine.svelte";
  import { onMount } from "svelte";

  let { w, busy, invite = null, onaction, onresolve, oninvite, onclosecode }: {
    w: WorldView; busy: boolean; invite?: { code: string; members: number } | null;
    onaction: (kind: string) => void; onresolve: (keepMine: boolean) => void; oninvite: () => void; onclosecode: () => void;
  } = $props();
  // someone came in after the code was made (the code is for one person)
  const joined = $derived(!!invite && w.members > invite.members);
  let copied = $state(false);
  async function copy() {
    if (!invite) return;
    try {
      await navigator.clipboard.writeText(invite.code);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {}
  }
  let showHist = $state(false);
  const histOpen = $derived(showHist || w.state === "diverged");
  let now = $state(Date.now());
  onMount(() => {
    const t = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(t);
  });
  const ago = $derived.by(() => {
    if (!w.updatedAt) return null;
    const min = Math.floor((now - Date.parse(w.updatedAt)) / 6e4);
    if (min < 1) return "atualizado agora";
    if (min < 60) return `atualizado há ${min} min`;
    const h = Math.floor(min / 60);
    if (h < 24) return `atualizado há ${h} h`;
    const d = Math.floor(h / 24);
    return `atualizado há ${d} ${d === 1 ? "dia" : "dias"}`;
  });
  const fold = ICON.chev.replace("<svg", '<svg class="fold"');
  const chev = ICON.chev.replace("<svg", '<svg style="width:14px;transform:rotate(-90deg);color:var(--muted)"');
</script>

<div class="card">
  <div class="w2-head">
    <img class="st-medal" src="/art/world-medallion.png" alt="" aria-hidden="true" />
    <div class="txt" title={w.line}><h3>{w.name}</h3><Status tone={w.tone} text={w.status} /><span class="with">{w.with}{#if w.history.length}<span class="sep">·</span><button class="ago" class:open={histOpen} aria-expanded={histOpen} title={histOpen ? "Esconder o histórico" : "Ver o histórico"} onclick={() => (showHist = !histOpen)}>{ago ?? "histórico"}{@html fold}</button>{:else if ago}<span class="sep">·</span>{ago}{/if}</span></div>
    <button class="btn" title="Um código para um amigo entrar neste mundo" disabled={busy} onclick={oninvite}>{@html ICON.link}Convidar</button>
    {#if w.state !== "diverged" && w.action}<Btn action={w.action} {busy} onclick={() => onaction(w.action!.kind)} />{/if}
  </div>
  {#if invite}
    <div class="invite">
      <div class="txt">
        <div class="name">{joined ? "Seu amigo entrou no mundo" : "Mande este código para o amigo"}</div>
        <div class="sub">{joined ? "O código já foi usado. Gere outro para convidar mais alguém." : "Ele cola em “Tenho um código” no app dele. Vale por 24 horas e para uma pessoa."}</div>
      </div>
      <span class="val">{invite.code}</span>
      <button class="btn" onclick={copy}>{@html ICON.copy}{copied ? "Copiado" : "Copiar"}</button>
      <button class="icon-btn" title="Fechar" aria-label="Fechar o código" onclick={onclosecode}>{@html ICON.close}</button>
    </div>
  {/if}
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
  {#if histOpen}
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
  {/if}
  {#if w.note}<div style="padding:10px 16px;border-top:1px solid var(--border)"><Note text={w.note.text} icon={w.note.icon} /></div>{/if}
</div>

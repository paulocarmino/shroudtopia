<script lang="ts">
  // OrangeMacros' toggle, as the prototype draws it: 36×20 track, ON/OFF label (the small one has none).
  let { on, sm = false, disabled = false, label = "", onchange }: { on: boolean; sm?: boolean; disabled?: boolean; label?: string; onchange: (on: boolean) => void } = $props();
</script>

<button class="tg" class:sm role="switch" aria-checked={on} aria-label={label} {disabled} onclick={() => onchange(!on)}>
  <span class="track"></span>
  {#if !sm}<span class="lbl">{on ? "ON" : "OFF"}</span>{/if}
</button>

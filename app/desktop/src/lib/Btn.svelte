<script lang="ts">
  import { ICON } from "./icons";
  import type { Action } from "./api";
  let { action, big = false, busy = false, onclick }: { action: Action; big?: boolean; busy?: boolean; onclick: () => void } = $props();
</script>

<button class="btn {action.cls}" class:big disabled={action.disabled || busy} {onclick}>
  {#if action.icon}{@html ICON[action.icon]}{/if}{action.label}
</button>

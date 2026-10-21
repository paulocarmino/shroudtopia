<script lang="ts">
  import { ICON } from "./icons";
  let { text, icon = "info", error = false }: { text: string; icon?: string; error?: boolean } = $props();
</script>

<div class="note" class:err={error}>{@html ICON[icon] ?? ICON.info}<span>{text}</span></div>

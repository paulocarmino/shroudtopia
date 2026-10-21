<script lang="ts">
  import type { Progress } from "./api";
  let { p }: { p: Progress } = $props();
</script>

<div class="progress-line"><span>{p.text}</span><span>{p.pct}%</span><div class="bar"><i style="width:{p.pct}%"></i></div></div>

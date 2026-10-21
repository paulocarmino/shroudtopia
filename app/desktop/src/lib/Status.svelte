<script lang="ts">
  let { tone, text }: { tone: string; text: string } = $props();
</script>

<span class="status {tone}"><span class="dot" class:live={tone === "live"}></span>{text}</span>

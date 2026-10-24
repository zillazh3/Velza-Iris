<script lang="ts">
  import "@fontsource-variable/manrope";
  import "@fontsource-variable/fraunces";
  import "../app.css";
  import { onMount } from "svelte";
  import { themeManager } from "$lib/theme/theme.svelte";

  let { children } = $props();

  onMount(() => {
    themeManager.init();
  });
</script>

{@render children()}

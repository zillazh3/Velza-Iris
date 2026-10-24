<script lang="ts">
  import "@fontsource-variable/manrope";
  import "@fontsource-variable/fraunces";
  import "../app.css";
  import { onMount } from "svelte";
  import { themeManager } from "$lib/theme/theme.svelte";
  import { i18n } from "$lib/i18n/index.svelte";

  let { children } = $props();

  onMount(() => {
    themeManager.init();
    i18n.init();
  });
</script>

{@render children()}

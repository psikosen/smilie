<script lang="ts">
  import TopBar from "$lib/components/TopBar.svelte";
  import Settings from "$lib/components/Settings.svelte";
  import { goto } from "$app/navigation";
</script>

<TopBar onSettings={() => goto("/")} onBack={() => goto("/")} />
<Settings />

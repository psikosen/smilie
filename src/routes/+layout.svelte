<script lang="ts">
  import "../styles/global.css";
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { goto } from "$app/navigation";
  import { cursorGlow } from "$lib/theme/effects";
  import Toast from "$lib/components/Toast.svelte";
  import { activePage } from "$lib/stores/active_page";
  import { requestCloseAll } from "$lib/stores/drawer";
  import {
    classify,
    FOCUS_SEARCH_EVENT,
  } from "$lib/keyboard/shortcuts";

  let { children } = $props();
  let root = $state<HTMLDivElement | undefined>();

  onMount(() => {
    if (root) {
      root.style.setProperty("--mx", "50");
      root.style.setProperty("--my", "50");
    }

    function onKey(ev: KeyboardEvent) {
      const action = classify(ev);
      if (!action) return;

      switch (action) {
        case "esc":
          requestCloseAll();
          return;

        case "focus-search":
          ev.preventDefault();
          document.dispatchEvent(new CustomEvent(FOCUS_SEARCH_EVENT));
          return;

        case "open-settings":
          ev.preventDefault();
          void goto("/settings/");
          return;

        case "page-prev":
        case "page-next": {
          // Only intercept when a paper detail page registered itself.
          // Otherwise let arrow keys reach inputs / scroll containers.
          const s = get(activePage);
          const handled = !!s.setActivePageId && s.pages.length > 0;
          if (!handled) return;
          ev.preventDefault();
          activePage.step(action === "page-prev" ? -1 : 1);
          return;
        }
      }
    }

    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

<div bind:this={root} class="app-shell" use:cursorGlow={{ strength: 0.7 }}>
  {@render children()}
</div>

<Toast />

<style>
  .app-shell {
    position: relative;
    width: 100%;
    height: 100vh;
    height: 100dvh;
    overflow: hidden;
    isolation: isolate;
  }
</style>

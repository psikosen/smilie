<script lang="ts">
  import { onMount } from "svelte";
  import { searchQuery } from "$lib/stores/search";
  import { get } from "svelte/store";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { FOCUS_SEARCH_EVENT } from "$lib/keyboard/shortcuts";

  interface Props {
    onSettings?: () => void;
    onSearch?: (q: string) => void;
    /** When provided, renders a Back button in the brand slot. Callers
     *  pass `() => goto("/")` (or similar) to return to the main menu. */
    onBack?: () => void;
  }
  let { onSettings, onSearch, onBack }: Props = $props();

  let query = $state(get(searchQuery));
  let searchInput = $state<HTMLInputElement | undefined>();

  function submit() {
    const q = query.trim();
    searchQuery.set(q);
    onSearch?.(q);
    if (q && page.url.pathname !== "/") void goto("/");
  }

  function onInput(e: Event) {
    query = (e.target as HTMLInputElement).value;
    searchQuery.set(query);
  }

  onMount(() => {
    const unsubscribe = searchQuery.subscribe((value) => {
      query = value;
    });

    function focusSearch() {
      searchInput?.focus();
      searchInput?.select();
    }
    document.addEventListener(FOCUS_SEARCH_EVENT, focusSearch);
    return () => {
      unsubscribe();
      document.removeEventListener(FOCUS_SEARCH_EVENT, focusSearch);
    };
  });
</script>

<header class="topbar">
  <div class="brand">
    {#if onBack}
      <button class="icon-btn back" type="button" onclick={onBack} aria-label="Back to library">
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none">
          <path d="M15 18l-6-6 6-6" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </button>
    {/if}
    <div class="mark" aria-hidden="true">
      <svg viewBox="0 0 24 24" width="18" height="18" fill="none">
        <path
          d="M4 7l8-4 8 4-8 4-8-4z"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linejoin="round"
        />
        <path
          d="M4 12l8 4 8-4M4 17l8 4 8-4"
          stroke="currentColor"
          stroke-width="1.6"
          stroke-linejoin="round"
          opacity="0.65"
        />
      </svg>
    </div>
    <div class="name">Smilie</div>
  </div>

  <form
    class="search"
    onsubmit={(e) => {
      e.preventDefault();
      submit();
    }}
  >
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" aria-hidden="true">
      <circle cx="7" cy="7" r="5" stroke="currentColor" stroke-width="1.4" />
      <path d="M11 11l3 3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
    </svg>
    <input
      bind:this={searchInput}
      type="search"
      aria-label="Search generated learning pages"
      placeholder="Search pages…"
      value={query}
      oninput={onInput}
      spellcheck="false"
      autocomplete="off"
    />
  </form>

  <div class="actions">
    <button class="icon-btn" type="button" onclick={onSettings} aria-label="Settings">
      <svg viewBox="0 0 24 24" width="16" height="16" fill="none">
        <circle cx="12" cy="12" r="3" stroke="currentColor" stroke-width="1.6" />
        <path
          d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3h.1a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8v.1a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"
          stroke="currentColor"
          stroke-width="1.4"
        />
      </svg>
    </button>
  </div>
</header>

<style>
  .topbar {
    position: relative;
    display: grid;
    grid-template-columns: 1fr minmax(220px, 420px) 1fr;
    align-items: center;
    height: 56px;
    padding: 0 16px;
    /* Honor the iOS notch / status bar. */
    padding-top: env(safe-area-inset-top);
    border-bottom: 1px solid var(--border);
    background:
      linear-gradient(115deg, rgba(255, 255, 255, 0.045), transparent 34%),
      var(--bg-glass);
    backdrop-filter: blur(20px) saturate(140%);
    -webkit-backdrop-filter: blur(20px) saturate(140%);
    z-index: var(--z-sticky);
    box-shadow:
      0 1px 0 rgba(255, 255, 255, 0.035) inset,
      0 16px 38px -32px rgba(154, 107, 255, 0.7);
  }

  /* On phones, give the brand strip a fixed minimum and let the search
     box shrink to fill the rest — the prior fixed-min 220px caused the
     header to overflow on 320px viewports. Hide the brand label so the
     mark + back button + search + settings always fit on one row. */
  @media (max-width: 720px) {
    .topbar {
      grid-template-columns: auto 1fr auto;
      padding: 0 10px;
      padding-top: env(safe-area-inset-top);
      gap: 8px;
    }
    .name { display: none; }
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .mark {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    color: var(--violet-100);
    background:
      radial-gradient(circle at 30% 20%, rgba(255, 255, 255, 0.18), transparent 48%),
      linear-gradient(135deg, #321b6b, #090415);
    border: 1px solid var(--border-strong);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.08),
      0 0 18px -8px var(--violet-glow);
  }

  .name {
    font-weight: 600;
    letter-spacing: 0.01em;
    color: var(--text-hi);
  }

  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 12px;
    height: 34px;
    border-radius: var(--radius-pill);
    background:
      linear-gradient(115deg, rgba(255, 255, 255, 0.035), transparent 35%),
      rgba(8, 7, 16, 0.86);
    border: 1px solid var(--border);
    color: var(--text-mid);
    transition: border-color var(--dur) var(--ease), box-shadow var(--dur) var(--ease);
  }

  .search:focus-within {
    border-color: var(--violet-400);
    box-shadow: 0 0 0 3px rgba(154, 107, 255, 0.18);
  }

  .search input {
    flex: 1;
    min-width: 0;
    background: transparent;
    border: 0;
    outline: 0;
    color: var(--text-hi);
    font-size: var(--text-sm);
  }

  .search input::placeholder {
    color: var(--text-dim);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .icon-btn {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: var(--radius-md);
    color: var(--text-mid);
    border: 1px solid transparent;
    transition:
      color var(--dur) var(--ease),
      background var(--dur) var(--ease),
      border-color var(--dur) var(--ease);
  }

  .icon-btn:hover {
    color: var(--violet-100);
    background: rgba(154, 107, 255, 0.08);
    border-color: var(--border);
  }

  /* Back button sits before the brand mark. Same sizing as other icon
     buttons so the header height stays consistent. */
  .back {
    margin-right: 2px;
  }
</style>

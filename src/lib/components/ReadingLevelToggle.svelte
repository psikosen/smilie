<script lang="ts">
  /**
   * Three-segment reading-level toggle (ELI5 / Default / Expert) for the
   * PageViewer chrome. Pure presentation — the parent owns level-pick
   * orchestration (server call, optimistic pending markers, toasts).
   *
   * Visual states per segment derived via `deriveSegmentState`:
   *   active    → currently displayed level (violet glow + ring)
   *   cached    → instant-switch available
   *   pending   → user just clicked it; soft pulse while regen runs
   *   available → not cached and not pending — clicking kicks regen
   *
   * Includes an (i) info button with a tooltip explaining each level.
   */
  import type { ReadingLevel } from "$lib/ipc";
  import {
    READING_LEVELS,
    levelLabel,
    levelDescription,
    deriveSegmentState,
  } from "./level-toggle";

  interface Props {
    currentLevel: ReadingLevel;
    cachedLevels: ReadingLevel[];
    pendingLevel: ReadingLevel | null;
    /** Disable interaction entirely (e.g. while a regen is in flight). */
    disabled?: boolean;
    onPick: (level: ReadingLevel) => void;
  }
  let {
    currentLevel,
    cachedLevels,
    pendingLevel,
    disabled = false,
    onPick,
  }: Props = $props();

  let tooltipOpen = $state(false);
</script>

<div class="level-toggle" role="radiogroup" aria-label="Reading level">
  {#each READING_LEVELS as level (level)}
    {@const segState = deriveSegmentState(
      level,
      currentLevel,
      cachedLevels,
      pendingLevel,
    )}
    <button
      type="button"
      role="radio"
      aria-checked={segState === "active"}
      class="seg"
      class:active={segState === "active"}
      class:cached={segState === "cached"}
      class:pending={segState === "pending"}
      disabled={disabled}
      onclick={() => onPick(level)}
      title={levelDescription(level)}
    >
      {levelLabel(level)}
    </button>
  {/each}
  <button
    type="button"
    class="info"
    aria-label="What do these reading levels mean?"
    aria-expanded={tooltipOpen}
    onclick={() => (tooltipOpen = !tooltipOpen)}
    onmouseenter={() => (tooltipOpen = true)}
    onmouseleave={() => (tooltipOpen = false)}
    onfocus={() => (tooltipOpen = true)}
    onblur={() => (tooltipOpen = false)}
  >
    i
    {#if tooltipOpen}
      <div class="tooltip" role="tooltip">
        <strong>ELI5</strong>
        <span>{levelDescription("eli5")}</span>
        <strong>Default</strong>
        <span>{levelDescription("default")}</span>
        <strong>Expert</strong>
        <span>{levelDescription("expert")}</span>
      </div>
    {/if}
  </button>
</div>

<style>
  .level-toggle {
    position: relative;
    display: inline-flex;
    align-items: stretch;
    padding: 2px;
    border-radius: var(--radius-pill);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    gap: 0;
  }
  .seg {
    padding: 4px 10px;
    border-radius: var(--radius-pill);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-mid);
    font-size: var(--text-xs);
    font-weight: 500;
    cursor: pointer;
    transition:
      color var(--dur) var(--ease),
      background var(--dur) var(--ease),
      border-color var(--dur) var(--ease);
  }
  .seg:hover:not(:disabled):not(.active) {
    color: var(--text-hi);
    background: rgba(154, 107, 255, 0.08);
  }
  .seg.active {
    color: var(--text-hi);
    background: linear-gradient(
      160deg,
      rgba(154, 107, 255, 0.22),
      rgba(106, 45, 255, 0.10)
    );
    border-color: var(--violet-400);
    box-shadow: 0 0 0 1px var(--violet-400) inset, 0 0 12px var(--violet-glow);
  }
  .seg.cached { color: var(--violet-100); }
  .seg.pending {
    color: var(--violet-100);
    border-color: var(--violet-300);
    background: rgba(154, 107, 255, 0.10);
    animation: seg-pulse 1.4s ease-in-out infinite;
  }
  .seg:disabled { cursor: not-allowed; opacity: 0.7; }
  @keyframes seg-pulse {
    0%, 100% { opacity: 0.85; }
    50%      { opacity: 1; }
  }

  .info {
    position: relative;
    margin-left: 2px;
    width: 22px;
    height: 22px;
    align-self: center;
    border-radius: 50%;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-mid);
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 600;
    line-height: 1;
    cursor: help;
  }
  .info:hover, .info:focus-visible {
    color: var(--violet-100);
    border-color: var(--violet-300);
  }

  .tooltip {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: var(--z-overlay);
    width: 240px;
    padding: 10px 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-strong);
    background: var(--bg-glass);
    backdrop-filter: blur(20px) saturate(140%);
    -webkit-backdrop-filter: blur(20px) saturate(140%);
    box-shadow: 0 18px 40px -16px rgba(0, 0, 0, 0.6);
    color: var(--text-mid);
    font-size: var(--text-xs);
    line-height: 1.5;
    text-align: left;
    display: grid;
    grid-template-columns: auto 1fr;
    column-gap: 8px;
    row-gap: 4px;
    cursor: default;
  }
  .tooltip strong {
    color: var(--violet-100);
    font-weight: 600;
  }

  @media (prefers-reduced-motion: reduce) {
    .seg.pending { animation: none; }
  }
</style>

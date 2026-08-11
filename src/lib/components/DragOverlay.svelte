<script lang="ts">
  import { drag } from "$lib/stores/drag";
</script>

{#if $drag.hovering}
  <div class="drag-overlay" aria-hidden="true">
    <div class="panel">
      <div class="icon">
        <svg viewBox="0 0 48 48" width="48" height="48" fill="none">
          <path
            d="M24 6v24m0 0-8-8m8 8 8-8"
            stroke="currentColor"
            stroke-width="2.4"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
          <path
            d="M8 34v4a4 4 0 0 0 4 4h24a4 4 0 0 0 4-4v-4"
            stroke="currentColor"
            stroke-width="2.4"
            stroke-linecap="round"
          />
        </svg>
      </div>
      <div class="text">
        <div class="headline">Drop a paper to start</div>
        <div class="sub">Release anywhere on this window</div>
      </div>
    </div>
  </div>
{/if}

<style>
  .drag-overlay {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: grid;
    place-items: center;
    background: rgba(7, 6, 11, 0.55);
    backdrop-filter: blur(14px) saturate(140%);
    -webkit-backdrop-filter: blur(14px) saturate(140%);
    pointer-events: none;
    animation: fade-in var(--dur) var(--ease);
  }

  .panel {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 36px 48px;
    border-radius: var(--radius-xl);
    color: var(--violet-100);
    background:
      radial-gradient(
        600px circle at 50% 30%,
        rgba(154, 107, 255, 0.28),
        transparent 70%
      ),
      linear-gradient(135deg, #0e0b18, #1a1030 55%, #0e0b18);
    border: 2px dashed var(--violet-400);
    box-shadow:
      0 30px 90px -20px rgba(154, 107, 255, 0.7),
      0 0 0 2px rgba(154, 107, 255, 0.18) inset;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 88px;
    height: 88px;
    border-radius: var(--radius-lg);
    color: var(--violet-100);
    background:
      radial-gradient(
        circle at 30% 20%,
        rgba(255, 255, 255, 0.12),
        transparent 60%
      ),
      linear-gradient(135deg, #2a1955, #150a30);
    border: 1px solid var(--border-strong);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.12),
      0 10px 30px -10px rgba(106, 45, 255, 0.6);
  }

  .text {
    text-align: center;
  }

  .headline {
    font-size: var(--text-xl);
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--text-hi);
  }

  .sub {
    margin-top: 6px;
    font-size: var(--text-sm);
    color: var(--text-mid);
  }

  @keyframes fade-in {
    from { opacity: 0 }
    to { opacity: 1 }
  }

  @media (prefers-reduced-motion: reduce) {
    .drag-overlay {
      animation: none;
    }
  }
</style>

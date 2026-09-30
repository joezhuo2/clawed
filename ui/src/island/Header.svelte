<script lang="ts">
  import { pickPrimary, stateColorVar } from "../lib/format";
  import type { Snapshot } from "../lib/types";
  import Ring from "./Ring.svelte";

  let { snap }: { snap: Snapshot | null } = $props();

  const MAX_DOTS = 5;
  const sessions = $derived(snap?.sessions ?? []);
  const primary = $derived(pickPrimary(sessions));
  const showBar = $derived(!!primary && primary.todos_total > 0);
  const progress = $derived(primary && primary.todos_total > 0 ? primary.todos_done / primary.todos_total : 0);
</script>

<div class="header">
  <div class="dots" aria-label="{sessions.length} sessions">
    {#each sessions.slice(0, MAX_DOTS) as s (s.id)}
      <span class="dot" class:pulse={s.state === "awaiting_approval"} style:background={stateColorVar(s.state)}></span>
    {/each}
    {#if sessions.length > MAX_DOTS}
      <span class="more">+{sessions.length - MAX_DOTS}</span>
    {/if}
  </div>

  <div class="bar" class:hidden={!showBar}>
    <div class="track"><div class="fill" style:transform="scaleX({progress})"></div></div>
    <span class="count">{primary?.todos_done ?? 0}/{primary?.todos_total ?? 0}</span>
  </div>

  <svg class="rings" viewBox="0 0 24 24" aria-hidden="true">
    <Ring ring={snap?.usage.seven_day ?? null} r={10} stroke={2.4} cx={12} cy={12} />
    <Ring ring={snap?.usage.five_hour ?? null} r={5.8} stroke={2.4} cx={12} cy={12} />
  </svg>
</div>

<style>
  .header {
    display: flex;
    align-items: center;
    gap: 10px;
    height: var(--pill-h);
    width: var(--pill-w);
    padding: 0 10px 0 14px;
    box-sizing: border-box;
    margin: 0 auto;
  }
  .dots {
    display: flex;
    gap: 4px;
    align-items: center;
    flex: none;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    transition: background 300ms;
  }
  /* One-shot attention cue, not a loop: runs 3 times then stops. */
  .pulse {
    animation: pulse 900ms ease-in-out 3;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
  .more {
    font-size: 10px;
    color: var(--island-muted);
  }
  .bar {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    transition: opacity var(--fade-ms);
  }
  .bar.hidden {
    opacity: 0;
  }
  .track {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--island-track);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    width: 100%;
    background: var(--state-working);
    transform-origin: left;
    transition: transform 400ms var(--ease);
  }
  .count {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: var(--island-text);
  }
  .rings {
    width: 22px;
    height: 22px;
    flex: none;
    margin-left: auto;
  }
</style>

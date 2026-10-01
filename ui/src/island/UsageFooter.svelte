<script lang="ts">
  import { formatCountdown } from "../lib/format";
  import type { Ring as RingT, SystemView, UsageView } from "../lib/types";
  import Ring from "./Ring.svelte";

  let { usage, system, now }: { usage: UsageView; system: SystemView | null; now: number } = $props();

  interface Item {
    label: string;
    ring: RingT | null;
    /** Optional third line under the label. */
    extra?: string;
  }

  function resetText(r: RingT | null): string {
    if (!r?.resets_at) return "";
    const secs = r.resets_at - Math.floor(now / 1000);
    return secs > 0 ? `↻ ${formatCountdown(secs)}` : "resetting";
  }

  const items = $derived<Item[]>([
    { label: "5-hour", ring: usage.five_hour, extra: resetText(usage.five_hour) },
    { label: "7-day", ring: usage.seven_day, extra: resetText(usage.seven_day) },
    { label: "CPU", ring: system?.cpu ?? null },
    {
      label: "RAM",
      ring: system?.ram ?? null,
      extra: system ? `${system.ram_used_gb.toFixed(1)}/${Math.round(system.ram_total_gb)}G` : "",
    },
    { label: "GPU", ring: system?.gpu ?? null },
  ]);
</script>

{#snippet item(it: Item)}
  <div class="item">
    <div class="gauge">
      <svg viewBox="0 0 36 36" aria-hidden="true">
        <Ring ring={it.ring} r={15} stroke={3.5} cx={18} cy={18} />
      </svg>
      <div class="pct">{it.ring ? `${Math.round(it.ring.pct)}%` : "--"}</div>
    </div>
    <div class="sub">{it.label}</div>
    {#if it.extra}<div class="sub">{it.extra}</div>{/if}
  </div>
{/snippet}

<div class="usage">
  <div class="row">
    {#each items as it (it.label)}{@render item(it)}{/each}
  </div>
  {#if usage.source !== "exact"}
    <div class="note">{usage.source === "estimated" ? "plan usage estimated" : "no plan usage data yet"}</div>
  {/if}
</div>

<style>
  .usage {
    padding: 10px 0 2px;
    border-top: 1px solid var(--island-faint);
  }
  .row {
    display: flex;
    align-items: flex-start;
    gap: 4px;
  }
  .item {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
  }
  .gauge {
    position: relative;
    width: 40px;
    height: 40px;
    margin-bottom: 3px;
  }
  svg {
    width: 100%;
    height: 100%;
  }
  .pct {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    font-size: 11px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .sub {
    font-size: 10px;
    color: var(--island-muted);
    white-space: nowrap;
  }
  .note {
    margin-top: 6px;
    font-size: 10px;
    color: var(--island-muted);
  }
</style>

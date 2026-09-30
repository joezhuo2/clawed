<script lang="ts">
  import { formatCountdown } from "../lib/format";
  import type { Ring as RingT, SystemView, UsageView } from "../lib/types";
  import Ring from "./Ring.svelte";

  let { usage, system, now }: { usage: UsageView; system: SystemView | null; now: number } = $props();

  interface Item {
    label: string;
    ring: RingT | null;
    sub: string;
  }

  function resetText(r: RingT | null): string {
    if (!r?.resets_at) return "";
    const secs = r.resets_at - Math.floor(now / 1000);
    return secs > 0 ? `resets in ${formatCountdown(secs)}` : "resetting";
  }

  const plan = $derived<Item[]>([
    { label: "5-hour", ring: usage.five_hour, sub: resetText(usage.five_hour) },
    { label: "7-day", ring: usage.seven_day, sub: resetText(usage.seven_day) },
  ]);

  const machine = $derived<Item[]>([
    { label: "CPU", ring: system?.cpu ?? null, sub: "" },
    {
      label: "RAM",
      ring: system?.ram ?? null,
      sub: system ? `${system.ram_used_gb.toFixed(1)}/${Math.round(system.ram_total_gb)} GB` : "",
    },
    { label: "GPU", ring: system?.gpu ?? null, sub: "" },
  ]);
</script>

{#snippet item(it: Item)}
  <div class="item">
    <svg viewBox="0 0 36 36" aria-hidden="true">
      <Ring ring={it.ring} r={15} stroke={3.5} cx={18} cy={18} />
    </svg>
    <div class="text">
      <div class="pct">{it.ring ? `${Math.round(it.ring.pct)}%` : "--"}</div>
      <div class="sub">{it.label}{it.sub ? ` · ${it.sub}` : ""}</div>
    </div>
  </div>
{/snippet}

<div class="usage">
  <div class="row">
    {#each plan as it (it.label)}{@render item(it)}{/each}
  </div>
  {#if usage.source !== "exact"}
    <div class="note">{usage.source === "estimated" ? "plan usage estimated" : "no plan usage data yet"}</div>
  {/if}
  <div class="row machine">
    {#each machine as it (it.label)}{@render item(it)}{/each}
  </div>
</div>

<style>
  .usage {
    padding: 10px 0 2px;
    border-top: 1px solid var(--island-faint);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 18px;
  }
  .machine {
    margin-top: 10px;
    padding-top: 10px;
    border-top: 1px solid var(--island-faint);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  svg {
    width: 30px;
    height: 30px;
  }
  .pct {
    font-size: 13px;
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

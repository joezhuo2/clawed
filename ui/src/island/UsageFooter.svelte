<script lang="ts">
  import { formatCountdown } from "../lib/format";
  import type { Ring as RingT, UsageView } from "../lib/types";
  import Ring from "./Ring.svelte";

  let { usage, now }: { usage: UsageView; now: number } = $props();

  const items = $derived([
    { label: "5-hour", ring: usage.five_hour },
    { label: "7-day", ring: usage.seven_day },
  ] as { label: string; ring: RingT | null }[]);

  function resetText(r: RingT | null): string {
    if (!r?.resets_at) return "";
    const secs = r.resets_at - Math.floor(now / 1000);
    return secs > 0 ? `resets in ${formatCountdown(secs)}` : "resetting";
  }
</script>

<div class="usage">
  {#each items as it (it.label)}
    <div class="item">
      <svg viewBox="0 0 36 36" aria-hidden="true">
        <Ring ring={it.ring} r={15} stroke={3.5} cx={18} cy={18} />
      </svg>
      <div class="text">
        <div class="pct">{it.ring ? `${Math.round(it.ring.pct)}%` : "--"}</div>
        <div class="sub">{it.label}{resetText(it.ring) ? ` · ${resetText(it.ring)}` : ""}</div>
      </div>
    </div>
  {/each}
  {#if usage.source !== "exact"}
    <div class="note">{usage.source === "estimated" ? "estimated" : "no usage data yet"}</div>
  {/if}
</div>

<style>
  .usage {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 18px;
    padding: 10px 0 2px;
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
  }
  .note {
    flex-basis: 100%;
    font-size: 10px;
    color: var(--island-muted);
  }
</style>

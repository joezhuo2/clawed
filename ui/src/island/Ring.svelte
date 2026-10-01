<script lang="ts">
  import { ringDash } from "../lib/format";
  import type { Ring } from "../lib/types";

  let {
    ring,
    r,
    stroke,
    cx,
    cy,
    color,
  }: { ring: Ring | null; r: number; stroke: number; cx: number; cy: number; color: string } = $props();

  const dash = $derived(ringDash(ring?.pct ?? 0, r));
  const stroke_ = $derived(!ring ? "transparent" : ring.level === "ok" ? color : `var(--ring-${ring.level})`);
</script>

<circle {cx} {cy} {r} fill="none" stroke="var(--island-track)" stroke-width={stroke} />
<circle
  class="arc"
  {cx}
  {cy}
  {r}
  fill="none"
  stroke={stroke_}
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-dasharray={dash.circumference}
  stroke-dashoffset={dash.offset}
  transform="rotate(-90 {cx} {cy})"
/>

<style>
  .arc {
    transition:
      stroke-dashoffset 400ms var(--ease),
      stroke 400ms;
  }
</style>

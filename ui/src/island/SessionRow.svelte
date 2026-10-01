<script lang="ts">
  import { formatElapsed, stateColorVar, stateLabel, stepText } from "../lib/format";
  import type { SessionView } from "../lib/types";

  let { s, now }: { s: SessionView; now: number } = $props();

  const progress = $derived(s.todos_total > 0 ? s.todos_done / s.todos_total : 0);
  const ctx = $derived(s.context?.used_pct ?? null);
  const ctxLevel = $derived(ctx === null ? "ok" : ctx >= 90 ? "crit" : ctx >= 70 ? "warn" : "ok");
</script>

<div class="row">
  <div class="top">
    <span class="dot" style:background={stateColorVar(s.state)}></span>
    <span class="repo" title={s.repo}>{s.repo || "session"}</span>
    <span class="state">{stateLabel(s.state)}</span>
    <span class="elapsed">{formatElapsed((s.turn_ended_at ?? now) - s.turn_started_at)}</span>
  </div>
  <div class="step" title={stepText(s)}>{stepText(s) || " "}</div>
  <div class="meters">
    {#if s.todos_total > 0}
      <div class="meter">
        <div class="track"><div class="fill todo" style:transform="scaleX({progress})"></div></div>
        <span class="num">{s.todos_done}/{s.todos_total}</span>
      </div>
    {/if}
    <div class="meter" class:flash={s.compact_warning} title="Context window{s.context ? ` (${s.context.source})` : ''}">
      <span class="label">ctx</span>
      <div class="track">
        <div class="fill ctx-{ctxLevel}" style:transform="scaleX({(ctx ?? 0) / 100})"></div>
      </div>
      <span class="num">{ctx === null ? "--" : `${Math.round(ctx)}%`}</span>
    </div>
    {#if s.files_touched > 0}
      <span class="files">{s.files_touched} file{s.files_touched === 1 ? "" : "s"}</span>
    {/if}
  </div>
</div>

<style>
  .row {
    padding: 8px 0;
    border-top: 1px solid var(--island-faint);
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: none;
  }
  .repo {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .state {
    color: var(--island-muted);
    white-space: nowrap;
  }
  .elapsed {
    margin-left: auto;
    color: var(--island-muted);
    font-variant-numeric: tabular-nums;
  }
  .step {
    margin: 4px 0 6px 15px;
    font-size: 12px;
    color: var(--island-text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meters {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-left: 15px;
  }
  .meter {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }
  .label,
  .num,
  .files {
    font-size: 10px;
    color: var(--island-muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .track {
    flex: 1;
    height: 3px;
    border-radius: 2px;
    background: var(--island-track);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    transform-origin: left;
    transition: transform 400ms var(--ease);
  }
  .todo {
    background: var(--state-working);
  }
  .ctx-ok {
    background: var(--ring-ok);
  }
  .ctx-warn {
    background: var(--ring-warn);
  }
  .ctx-crit {
    background: var(--ring-crit);
  }
  .flash {
    animation: flash 500ms ease-in-out 6;
  }
  @keyframes flash {
    50% {
      opacity: 0.25;
    }
  }
</style>

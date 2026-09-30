<script lang="ts">
  import type { ApprovalView } from "../lib/types";

  let {
    approval,
    index,
    total,
    ondecide,
  }: { approval: ApprovalView; index: number; total: number; ondecide: (id: string, allow: boolean) => void } =
    $props();

  let busy = $state(false);

  function decide(allow: boolean) {
    if (busy) return;
    busy = true;
    ondecide(approval.id, allow);
  }
</script>

<div class="card">
  <div class="top">
    <span class="tag">Permission</span>
    <span class="repo">{approval.repo}</span>
    {#if total > 1}<span class="queue">{index + 1} of {total}</span>{/if}
  </div>
  <div class="tool">{approval.tool}</div>
  {#if approval.summary}
    <pre class="summary" title={approval.summary}>{approval.summary}</pre>
  {/if}
  <div class="actions">
    <button class="deny" disabled={busy} onclick={() => decide(false)}>Deny</button>
    <button class="allow" disabled={busy} onclick={() => decide(true)}>Allow</button>
  </div>
</div>

<style>
  .card {
    padding: 10px 0 12px;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
  }
  .tag {
    color: var(--state-approval);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .repo {
    color: var(--island-muted);
  }
  .queue {
    margin-left: auto;
    color: var(--island-muted);
  }
  .tool {
    margin-top: 6px;
    font-size: 13px;
    font-weight: 600;
  }
  .summary {
    margin: 6px 0 0;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--island-hover);
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 72px;
    overflow: hidden;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
  button {
    flex: 1;
    height: 30px;
    border: 0;
    border-radius: 15px;
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 120ms;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .allow {
    background: var(--allow-bg);
    color: var(--allow-text);
  }
  .deny {
    background: var(--deny-bg);
    color: var(--deny-text);
  }
</style>

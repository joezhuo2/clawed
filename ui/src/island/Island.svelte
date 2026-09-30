<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, untrack } from "svelte";
  import type { Snapshot, SystemView } from "../lib/types";
  import ApprovalCard from "./ApprovalCard.svelte";
  import Header from "./Header.svelte";
  import SessionRow from "./SessionRow.svelte";
  import UsageFooter from "./UsageFooter.svelte";

  // Must match COLLAPSED in src-tauri/src/window.rs and theme.css geometry.
  const COLLAPSED_W = 260;
  const COLLAPSED_H = 48;
  const EXPANDED_W = 420;
  const MAX_H = 640;

  let snap = $state<Snapshot | null>(null);
  let system = $state<SystemView | null>(null);
  let hovered = $state(false);
  let expanded = $state(false);
  let showPanel = $state(false);
  let now = $state(Date.now());
  let dismissed = $state(new Set<string>());
  let islandEl: HTMLDivElement | undefined = $state();

  const noticeKey = (id: string, ts: number) => `${id}:${ts}`;
  const waiting = $derived(
    (snap?.sessions ?? []).filter(
      (s) => s.state === "waiting_input" && !dismissed.has(noticeKey(s.id, s.last_event_at)),
    ),
  );
  const approvals = $derived(snap?.approvals ?? []);
  const pinned = $derived(approvals.length > 0 || waiting.length > 0);
  const wantOpen = $derived(hovered || pinned);

  let generation = 0;

  async function open() {
    const gen = ++generation;
    showPanel = true;
    if (expanded) return;
    await invoke("set_interactive", { interactive: true });
    await invoke("set_island_size", { width: EXPANDED_W, height: MAX_H });
    if (gen !== generation) return;
    // The cursor may have left while the window was growing.
    if (wantOpen) expanded = true;
    else close();
  }

  function close() {
    const gen = ++generation;
    expanded = false;
    const finish = async () => {
      if (gen !== generation || expanded) return;
      showPanel = false;
      lastHeight = 0;
      await invoke("set_island_size", { width: COLLAPSED_W, height: COLLAPSED_H });
      await invoke("set_interactive", { interactive: false });
    };
    // The window shrinks only after the shape animation has finished.
    const el = islandEl;
    let done = false;
    const onEnd = (e: TransitionEvent) => {
      if (e.propertyName !== "clip-path" || done) return;
      done = true;
      el?.removeEventListener("transitionend", onEnd);
      finish();
    };
    el?.addEventListener("transitionend", onEnd);
    setTimeout(() => {
      if (!done) {
        done = true;
        el?.removeEventListener("transitionend", onEnd);
        finish();
      }
    }, 450);
  }

  // Collapse starts as soon as the cursor leaves; re-entering mid-collapse
  // reverses the transition.
  $effect(() => {
    if (wantOpen) open();
    else if (untrack(() => expanded)) close();
  });

  // Fit the window to the expanded content.
  let lastHeight = 0;
  $effect(() => {
    if (!expanded || !islandEl) return;
    const el = islandEl;
    const ro = new ResizeObserver(() => {
      const h = Math.min(MAX_H, Math.ceil(el.getBoundingClientRect().height) + 16);
      if (Math.abs(h - lastHeight) > 2) {
        lastHeight = h;
        invoke("set_island_size", { width: EXPANDED_W, height: h });
      }
    });
    ro.observe(el);
    return () => ro.disconnect();
  });

  // Elapsed times and countdowns tick only while expanded.
  $effect(() => {
    if (!expanded) return;
    now = Date.now();
    const t = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(t);
  });

  function decide(id: string, allow: boolean) {
    invoke("decide", { id, allow });
  }

  function dismiss(id: string, ts: number) {
    dismissed = new Set(dismissed).add(noticeKey(id, ts));
  }

  onMount(() => {
    invoke<Snapshot>("get_state").then((s) => (snap = s));
    const unlisten = [
      listen<Snapshot>("state", (e) => (snap = e.payload)),
      listen<boolean>("hover", (e) => (hovered = e.payload)),
      listen<SystemView>("system", (e) => (system = e.payload)),
    ];
    return () => unlisten.forEach((p) => p.then((f) => f()));
  });
</script>

<div class="island" class:expanded bind:this={islandEl}>
  <Header {snap} />
  {#if showPanel && snap}
    <div class="panel">
      {#if approvals.length > 0}
        {#key approvals[0].id}
          <ApprovalCard approval={approvals[0]} index={0} total={approvals.length} ondecide={decide} />
        {/key}
      {/if}

      {#each waiting as s (s.id)}
        <div class="notice">
          <div class="notice-text">
            <strong>{s.repo}</strong>
            <span>{s.message ?? "Waiting for your input"}</span>
          </div>
          <button class="dismiss" aria-label="Dismiss" onclick={() => dismiss(s.id, s.last_event_at)}>×</button>
        </div>
      {/each}

      {#each snap.sessions as s (s.id)}
        <SessionRow {s} {now} />
      {:else}
        <div class="empty">No Claude Code sessions</div>
      {/each}

      <UsageFooter usage={snap.usage} {system} {now} />
      {#if snap.paused}<div class="paused">Approvals paused</div>{/if}
    </div>
  {/if}
</div>

<style>
  .island {
    position: absolute;
    top: var(--top-gap);
    left: 50%;
    width: var(--panel-w);
    transform: translateX(-50%);
    background: var(--island-bg);
    color: var(--island-text);
    border-radius: var(--panel-radius);
    /* Collapsed: only the centered pill is visible. */
    clip-path: inset(
      0 calc(50% - var(--pill-w) / 2) calc(100% - var(--pill-h)) calc(50% - var(--pill-w) / 2)
        round calc(var(--pill-h) / 2)
    );
    transition: clip-path var(--shape-ms) var(--ease);
  }
  .island.expanded {
    clip-path: inset(0 0 0 0 round var(--panel-radius));
  }
  .panel {
    padding: 0 var(--pad) var(--pad);
    opacity: 0;
    transition: opacity var(--fade-ms) ease-out;
  }
  /* Content fades in after the shape grows, and out before it shrinks. */
  .expanded .panel {
    opacity: 1;
    transition-delay: calc(var(--shape-ms) * 0.6);
  }
  .notice {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    margin: 6px 0 8px;
    border-radius: 10px;
    background: var(--island-hover);
    border-left: 3px solid var(--state-waiting);
    font-size: 12px;
  }
  .notice-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .notice-text span {
    color: var(--island-muted);
  }
  .dismiss {
    margin-left: auto;
    background: none;
    border: 0;
    color: var(--island-muted);
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
  }
  .empty {
    padding: 12px 0;
    font-size: 12px;
    color: var(--island-muted);
    border-top: 1px solid var(--island-faint);
  }
  .paused {
    margin-top: 6px;
    font-size: 10px;
    color: var(--state-waiting);
  }
</style>

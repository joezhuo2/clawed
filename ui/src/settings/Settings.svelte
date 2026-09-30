<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { InstallerStatus, Settings } from "../lib/types";

  let status = $state<InstallerStatus | null>(null);
  let settings = $state<Settings | null>(null);
  let overridesText = $state("");
  let confirm = $state<"install" | "uninstall" | null>(null);
  let message = $state("");
  let error = $state("");

  async function refresh() {
    status = await invoke<InstallerStatus>("installer_status");
  }

  onMount(async () => {
    await refresh();
    settings = await invoke<Settings>("get_settings");
    overridesText = Object.entries(settings.context_overrides)
      .map(([k, v]) => `${k}=${v}`)
      .join("\n");
  });

  async function apply(install: boolean) {
    error = "";
    message = "";
    try {
      const backup = await invoke<string>("installer_apply", { install });
      message = `${install ? "Installed" : "Uninstalled"}.${backup ? ` Backup: ${backup}` : ""} Restart running Claude Code sessions to pick up the change.`;
      confirm = null;
      await refresh();
    } catch (e) {
      error = String(e);
    }
  }

  function parseOverrides(text: string): Record<string, number> {
    const out: Record<string, number> = {};
    for (const line of text.split("\n")) {
      const [k, v] = line.split("=").map((x) => x.trim());
      const n = Number(v);
      if (k && Number.isFinite(n) && n > 0) out[k] = Math.round(n);
    }
    return out;
  }

  async function save() {
    if (!settings) return;
    error = "";
    try {
      await invoke("save_settings", {
        settings: { ...settings, context_overrides: parseOverrides(overridesText) },
      });
      message = "Settings saved.";
    } catch (e) {
      error = String(e);
    }
  }

  function diffLines(diff: string) {
    return diff.split("\n").map((l) => ({
      text: l,
      cls: l.startsWith("+") && !l.startsWith("+++") ? "add" : l.startsWith("-") && !l.startsWith("---") ? "del" : "",
    }));
  }
</script>

<main>
  <h1>clawed</h1>

  <section>
    <h2>Claude Code hooks</h2>
    {#if status}
      <p class="muted">
        {status.installed ? "Installed" : "Not installed"} in <code>{status.settings_path}</code>
      </p>
      {#if status.error}<p class="error">{status.error}</p>{/if}
      {#if !status.hook_source_found}
        <p class="error">clawed-hook was not found next to the app. Build it with <code>cargo build -p clawed-hook</code>.</p>
      {/if}
      {#if status.foreign_statusline}
        <p class="muted">
          You already have a status line, so clawed leaves it alone. Plan usage will be estimated from local
          transcripts instead of read from Claude Code.
        </p>
      {/if}

      <div class="buttons">
        <button onclick={() => (confirm = "install")} disabled={!status.hook_source_found}>
          {status.installed ? "Reinstall hooks…" : "Install hooks…"}
        </button>
        <button class="secondary" onclick={() => (confirm = "uninstall")} disabled={!status.installed}>
          Uninstall hooks…
        </button>
      </div>

      {#if confirm}
        {@const diff = confirm === "install" ? status.install_diff : status.uninstall_diff}
        <div class="confirm">
          <p>
            These changes will be written to <code>settings.json</code>. A backup is saved next to it.
          </p>
          <pre class="diff">{#each diffLines(diff) as l}<span class={l.cls}>{l.text}
</span>{/each}</pre>
          <div class="buttons">
            <button onclick={() => apply(confirm === "install")}>
              Confirm {confirm}
            </button>
            <button class="secondary" onclick={() => (confirm = null)}>Cancel</button>
          </div>
        </div>
      {/if}
    {/if}
  </section>

  {#if settings}
    <section>
      <h2>Island</h2>
      <label class="check">
        <input type="checkbox" bind:checked={settings.low_memory} />
        Low memory mode: close the island after
        <input class="num" type="number" min="1" max="120" bind:value={settings.idle_minutes} />
        minutes with no active session
      </label>
    </section>

    <section>
      <h2>Usage rings</h2>
      <div class="grid">
        <label>Warn at (%) <input type="number" min="1" max="100" bind:value={settings.thresholds.warn} /></label>
        <label>Critical at (%) <input type="number" min="1" max="100" bind:value={settings.thresholds.crit} /></label>
        <label>
          Estimate cap, 5 hours (tokens)
          <input type="number" min="1" bind:value={settings.caps.five_hour_tokens} />
        </label>
        <label>
          Estimate cap, 7 days (tokens)
          <input type="number" min="1" bind:value={settings.caps.seven_day_tokens} />
        </label>
      </div>
      <p class="muted">
        Caps only apply when exact usage from the Claude Code status line is unavailable. Plan limits are not
        published as token counts, so estimated rings are approximate.
      </p>
    </section>

    <section>
      <h2>Context window overrides</h2>
      <p class="muted">One per line: part of a model id, then the window size. Example: <code>opus=1000000</code></p>
      <textarea rows="3" bind:value={overridesText}></textarea>
    </section>

    <div class="buttons">
      <button onclick={save}>Save settings</button>
    </div>
  {/if}

  {#if message}<p class="ok">{message}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}

  <p class="muted small">Launch at login, pause approvals and low memory mode are also in the tray menu.</p>
</main>

<style>
  main {
    max-width: 640px;
    margin: 0 auto;
    padding: 24px;
  }
  h1 {
    font-size: 22px;
    margin: 0 0 16px;
  }
  h2 {
    font-size: 14px;
    margin: 0 0 8px;
  }
  section {
    background: var(--card-bg);
    border: 1px solid var(--card-border);
    border-radius: 10px;
    padding: 16px;
    margin-bottom: 14px;
  }
  .muted {
    color: var(--page-muted);
    font-size: 13px;
  }
  .small {
    font-size: 12px;
  }
  .error {
    color: var(--diff-del);
    font-size: 13px;
  }
  .ok {
    color: var(--diff-add);
    font-size: 13px;
  }
  code {
    font-family: var(--font-mono);
    font-size: 12px;
    word-break: break-all;
  }
  .buttons {
    display: flex;
    gap: 8px;
    margin-top: 10px;
  }
  button {
    font: inherit;
    font-size: 13px;
    padding: 6px 14px;
    border-radius: 6px;
    border: 1px solid var(--accent);
    background: var(--accent);
    color: #fff;
    cursor: pointer;
  }
  button.secondary {
    background: transparent;
    color: var(--accent);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .confirm {
    margin-top: 12px;
  }
  .diff {
    max-height: 260px;
    overflow: auto;
    font-family: var(--font-mono);
    font-size: 11px;
    line-height: 1.45;
    padding: 10px;
    border-radius: 6px;
    border: 1px solid var(--card-border);
    background: var(--page-bg);
    margin: 0;
  }
  .diff .add {
    color: var(--diff-add);
  }
  .diff .del {
    color: var(--diff-del);
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 13px;
  }
  label.check {
    flex-direction: row;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  input[type="number"],
  textarea {
    font: inherit;
    font-size: 13px;
    padding: 4px 6px;
    border-radius: 5px;
    border: 1px solid var(--card-border);
    background: var(--page-bg);
    color: var(--page-text);
  }
  input.num {
    width: 56px;
  }
  textarea {
    width: 100%;
    box-sizing: border-box;
    font-family: var(--font-mono);
  }
</style>

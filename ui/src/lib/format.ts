import type { Colors, SessionState, SessionView, Step } from "./types";

/** Mirrors `Colors::default()` in src-tauri/src/settings.rs. */
export const DEFAULT_COLORS: Colors = {
  working: "#4c8dff",
  done: "#34c759",
  error: "#ff453a",
  request: "#a970ff",
  five_hour: "#e5e5ea",
  seven_day: "#e5e5ea",
  cpu: "#64d2ff",
  ram: "#5e5ce6",
  gpu: "#66d4cf",
  ctx: "#e5e5ea",
  warn: "#ffb340",
  crit: "#ff453a",
};

/** CSS custom property each configurable color drives. */
export const COLOR_VARS: Record<keyof Colors, string> = {
  working: "--state-working",
  done: "--state-done",
  error: "--state-error",
  request: "--state-approval",
  five_hour: "--meter-5h",
  seven_day: "--meter-7d",
  cpu: "--meter-cpu",
  ram: "--meter-ram",
  gpu: "--meter-gpu",
  ctx: "--meter-ctx",
  warn: "--ring-warn",
  crit: "--ring-crit",
};

/** `45s`, `12m`, `2h 10m`, `3d 4h`. Negative input clamps to 0. */
export function formatCountdown(secs: number): string {
  const s = Math.max(0, Math.floor(secs));
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m`;
  return `${s}s`;
}

/** `0:42`, `12:05`, `1:02:03`. */
export function formatElapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

/** stroke-dasharray / offset for a ring of radius r filled to pct. */
export function ringDash(pct: number, r: number): { circumference: number; offset: number } {
  const circumference = 2 * Math.PI * r;
  const clamped = Math.min(100, Math.max(0, pct));
  return { circumference, offset: circumference * (1 - clamped / 100) };
}

/** Session shown in the collapsed step bar: most recently active working one. */
export function pickPrimary(sessions: SessionView[]): SessionView | null {
  let best: SessionView | null = null;
  for (const s of sessions) {
    if (s.state !== "working" && s.state !== "awaiting_approval") continue;
    if (!best || s.last_event_at > best.last_event_at) best = s;
  }
  return best;
}

export function stateColorVar(state: SessionState): string {
  switch (state) {
    case "working":
      return "var(--state-working)";
    case "waiting_input":
      return "var(--state-waiting)";
    case "awaiting_approval":
      return "var(--state-approval)";
    case "done":
      return "var(--state-done)";
    case "error":
      return "var(--state-error)";
    default:
      return "var(--state-stale)";
  }
}

export function stateLabel(state: SessionState): string {
  switch (state) {
    case "working":
      return "Working";
    case "waiting_input":
      return "Waiting for you";
    case "awaiting_approval":
      return "Needs approval";
    case "done":
      return "Done";
    case "error":
      return "Error";
    default:
      return "Ended";
  }
}

export function stepText(s: SessionView): string {
  if (s.active_todo) return s.active_todo;
  const step: Step | null = s.current_step;
  if (step) return step.summary ? `${step.tool} · ${step.summary}` : step.tool;
  return s.prompt ?? "";
}

/** File name part of a path, for compact display. */
export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

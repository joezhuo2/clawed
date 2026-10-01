// Mirrors the serde types in src-tauri/src/{store,approvals,usage,state,settings}.rs.

export type SessionState = "working" | "waiting_input" | "awaiting_approval" | "done" | "error" | "stale";

export interface Step {
  ts: number;
  tool: string;
  summary: string | null;
}

export interface ContextMeter {
  used_pct: number;
  window: number;
  source: "statusline" | "transcript";
}

export interface SessionView {
  id: string;
  repo: string;
  state: SessionState;
  current_step: Step | null;
  active_todo: string | null;
  todos_done: number;
  todos_total: number;
  context: ContextMeter | null;
  compact_warning: boolean;
  model: string | null;
  prompt: string | null;
  message: string | null;
  started_at: number;
  turn_started_at: number;
  turn_ended_at: number | null;
  last_event_at: number;
  files_touched: number;
  steps: Step[];
}

export interface ApprovalView {
  id: string;
  session_id: string;
  repo: string;
  tool: string;
  summary: string | null;
  created_at: number;
}

export type Level = "ok" | "warn" | "crit";

export interface Ring {
  pct: number;
  resets_at: number | null;
  level: Level;
}

export interface UsageView {
  five_hour: Ring | null;
  seven_day: Ring | null;
  source: "exact" | "estimated" | "unknown";
}

export interface SystemView {
  cpu: Ring | null;
  ram: Ring | null;
  gpu: Ring | null;
  ram_used_gb: number;
  ram_total_gb: number;
}

export interface Snapshot {
  sessions: SessionView[];
  approvals: ApprovalView[];
  usage: UsageView;
  paused: boolean;
  now: number;
}

export interface Settings {
  low_memory: boolean;
  idle_minutes: number;
  pause_approvals: boolean;
  thresholds: { warn: number; crit: number };
  caps: { five_hour_tokens: number; seven_day_tokens: number };
  context_overrides: Record<string, number>;
  first_run_done: boolean;
  hooks_notice_shown: boolean;
  check_updates: boolean;
}

export interface InstallerStatus {
  settings_path: string;
  hook_path: string;
  hook_source_found: boolean;
  installed: boolean;
  foreign_statusline: boolean;
  install_diff: string;
  uninstall_diff: string;
  error: string | null;
}

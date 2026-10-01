import { describe, expect, it } from "vitest";
import { baseName, formatCountdown, formatElapsed, pickPrimary, ringDash, stateColorVar, stepText } from "./format";
import type { SessionView } from "./types";

function session(over: Partial<SessionView>): SessionView {
  return {
    id: "s",
    repo: "r",
    state: "done",
    current_step: null,
    active_todo: null,
    todos_done: 0,
    todos_total: 0,
    context: null,
    compact_warning: false,
    model: null,
    prompt: null,
    narration: null,
    message: null,
    started_at: 0,
    turn_started_at: 0,
    turn_ended_at: null,
    last_event_at: 0,
    files_touched: 0,
    steps: [],
    ...over,
  };
}

describe("formatCountdown", () => {
  it("formats each unit", () => {
    expect(formatCountdown(45)).toBe("45s");
    expect(formatCountdown(12 * 60 + 5)).toBe("12m");
    expect(formatCountdown(2 * 3600 + 600)).toBe("2h 10m");
    expect(formatCountdown(3 * 86400 + 4 * 3600)).toBe("3d 4h");
    expect(formatCountdown(-5)).toBe("0s");
  });
});

describe("formatElapsed", () => {
  it("pads minutes and seconds", () => {
    expect(formatElapsed(42_000)).toBe("0:42");
    expect(formatElapsed(12 * 60_000 + 5_000)).toBe("12:05");
    expect(formatElapsed(3_723_000)).toBe("1:02:03");
  });
});

describe("ringDash", () => {
  it("maps pct to offset", () => {
    const full = ringDash(100, 10);
    expect(full.offset).toBeCloseTo(0);
    const half = ringDash(50, 10);
    expect(half.offset).toBeCloseTo(half.circumference / 2);
    expect(ringDash(150, 10).offset).toBeCloseTo(0);
    expect(ringDash(-1, 10).offset).toBeCloseTo(ringDash(0, 10).circumference);
  });
});

describe("pickPrimary", () => {
  it("picks the most recently active working session", () => {
    const a = session({ id: "a", state: "working", last_event_at: 5 });
    const b = session({ id: "b", state: "working", last_event_at: 9 });
    const c = session({ id: "c", state: "done", last_event_at: 20 });
    expect(pickPrimary([a, b, c])?.id).toBe("b");
    expect(pickPrimary([c])).toBeNull();
  });
});

describe("labels", () => {
  it("maps state to a CSS variable", () => {
    expect(stateColorVar("awaiting_approval")).toBe("var(--state-approval)");
    expect(stateColorVar("stale")).toBe("var(--state-stale)");
  });

  it("prefers the active todo, then the tool step", () => {
    expect(stepText(session({ active_todo: "Write tests" }))).toBe("Write tests");
    expect(stepText(session({ current_step: { ts: 0, tool: "Bash", summary: "npm test" } }))).toBe("Bash · npm test");
    expect(stepText(session({ prompt: "hi" }))).toBe("hi");
  });

  it("takes the file name", () => {
    expect(baseName("C:\\a\\b\\c.rs")).toBe("c.rs");
    expect(baseName("/a/b/")).toBe("b");
  });
});

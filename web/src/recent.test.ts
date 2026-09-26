import { describe, expect, it } from "vitest";
import type { WbEvent } from "./model";
import { afterEvent, type Recent } from "./recent";

const event = (partial: Partial<WbEvent>): WbEvent => ({
  seq: 1,
  actor: "agent",
  turn: 3,
  kind: "node.added",
  payload: { id: "n5" },
  ...partial,
});

describe("afterEvent", () => {
  const start: Recent = { turn: 3, items: new Set(["n1"]) };

  it("adds what the agent touches during the same turn", () => {
    expect([...afterEvent(start, event({})).items]).toEqual(["n1", "n5"]);
  });

  it("starts over when the agent begins a new turn", () => {
    expect(afterEvent(start, event({ turn: 5 }))).toEqual({ turn: 5, items: new Set(["n5"]) });
  });

  it("ignores the human's edits, messages and handovers", () => {
    expect(afterEvent(start, event({ actor: "human" }))).toBe(start);
    expect(afterEvent(start, event({ kind: "message.sent", payload: { text: "hi" } }))).toBe(start);
    expect(afterEvent(start, event({ kind: "turn.passed", payload: {} }))).toBe(start);
  });

  it("forgets deleted items", () => {
    expect([...afterEvent(start, event({ kind: "item.deleted", payload: { id: "n1" } })).items]).toEqual([]);
  });

  it("treats the opening sketch as nothing in particular", () => {
    expect(afterEvent({ turn: 0, items: new Set() }, event({ turn: 1 })).items.size).toBe(0);
  });
});

import { describe, expect, it } from "vitest";
import type { Board, BoardNode } from "./model";
import { viewAt } from "./view";

const node = (id: string, altitude?: number, parent?: string): BoardNode => ({
  id,
  kind: "service",
  label: id,
  at: { x: 0, y: 0 },
  by: "agent",
  ...(altitude ? { altitude } : {}),
  ...(parent ? { parent } : {}),
});

const board: Board = {
  nodes: {
    n1: node("n1", 1),
    n2: node("n2", 1),
    n3: node("n3", 2, "n1"),
    n4: node("n4", 2, "n1"),
    n5: node("n5", 4, "n4"),
    n6: node("n6"),
  },
  edges: {
    e1: { id: "e1", from: "n3", to: "n4", by: "agent" },
    e2: { id: "e2", from: "n4", to: "n2", label: "charges", by: "agent" },
    e3: { id: "e3", from: "n5", to: "n2", label: "retries", by: "agent" },
  },
  groups: {},
  annotations: {},
  turn: 1,
  holder: "agent",
  last_seq: 0,
};

describe("viewAt", () => {
  it("shows what is at that altitude or higher, and anything without one", () => {
    expect([...viewAt(board, 1).visible]).toEqual(["n1", "n2", "n6"]);
    expect([...viewAt(board, 4).visible]).toEqual(["n1", "n2", "n3", "n4", "n5", "n6"]);
  });

  it("rolls edges into hidden detail up to the nearest visible parent", () => {
    expect(viewAt(board, 1).rollups).toEqual([{ from: "n1", to: "n2", via: ["e2", "e3"] }]);
    expect(viewAt(board, 2).rollups).toEqual([]);
  });

  it("frames a parent around its children when they show", () => {
    expect([...viewAt(board, 1).containers]).toEqual([]);
    expect([...viewAt(board, 4).containers]).toEqual(["n1", "n4"]);
  });
});

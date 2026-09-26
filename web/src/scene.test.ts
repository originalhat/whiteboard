import { describe, expect, it } from "vitest";
import { type Board, NODE_HEIGHT, NODE_WIDTH } from "./model";
import { AGENT_COLOR, HUMAN_COLOR, RECENT_FILL, toSkeletons } from "./scene";

function board(partial: Partial<Board>): Board {
  return { nodes: {}, edges: {}, groups: {}, annotations: {}, turn: 1, holder: "agent", last_seq: 0, ...partial };
}

const sample = board({
  nodes: {
    n1: { id: "n1", kind: "service", label: "Order Service", at: { x: 0, y: 0 }, by: "agent" },
    n2: { id: "n2", kind: "db", label: "Orders DB", at: { x: 240, y: 0 }, by: "human" },
    n3: { id: "n3", kind: "note", label: "sync?", at: { x: 0, y: 160 }, attached_to: "n1", by: "agent" },
  },
  edges: { e1: { id: "e1", from: "n1", to: "n2", label: "writes", by: "agent" } },
  groups: { g1: { id: "g1", label: "Orders", members: ["n1", "n2"], by: "agent" } },
  annotations: {
    a1: {
      id: "a1",
      stroke: [
        [10, 20, 0, 0.5],
        [40, 60, 16, 0.6],
        [15, 90, 32],
      ],
      zoom: 1,
      targets: [],
      by: "human",
    },
  },
});

const byId = (id: string) => toSkeletons(sample).find((s) => s.id === id) as any;

describe("toSkeletons", () => {
  it("draws each node at its position under its own id, labelled", () => {
    const n1 = byId("n1");
    expect(n1).toMatchObject({ x: 0, y: 0, width: NODE_WIDTH, height: NODE_HEIGHT, label: { text: "Order Service" } });
  });

  it("gives each kind a recognisable shape", () => {
    expect(byId("n1").type).toBe("rectangle");
    expect(byId("n2").type).toBe("ellipse");
    expect(byId("n3").backgroundColor).not.toBe("transparent");
  });

  it("uses text small enough for real labels to fit", () => {
    expect(byId("n1").label.fontSize).toBeLessThanOrEqual(16);
    expect(byId("e1").label.fontSize).toBeLessThan(byId("n1").label.fontSize);
  });

  it("draws the human's marks as the strokes they drew, on top of everything", () => {
    const skeletons = toSkeletons(sample);
    const mark = byId("a1");
    expect(mark).toMatchObject({ type: "freedraw", x: 10, y: 20, width: 30, height: 70, strokeColor: HUMAN_COLOR });
    expect(mark.points).toEqual([
      [0, 0],
      [30, 40],
      [5, 70],
    ]);
    expect(mark.pressures).toEqual([0.5, 0.6, 0.5]);
    expect(skeletons.at(-1)?.id).toBe("a1");
  });

  it("shades what the agent changed in its latest turn", () => {
    const skeletons = toSkeletons(sample, new Set(["n1", "e1"]));
    const find = (id: string) => skeletons.find((s) => s.id === id) as any;
    expect(find("n1")).toMatchObject({ backgroundColor: RECENT_FILL, strokeWidth: 3 });
    expect(find("e1")).toMatchObject({ strokeWidth: 3 });
    expect(find("n2").strokeWidth).toBeUndefined();
  });

  it("colours the agent's marks differently from the human's", () => {
    expect(byId("n1").strokeColor).toBe(AGENT_COLOR);
    expect(byId("n2").strokeColor).toBe(HUMAN_COLOR);
  });

  it("binds edges to the nodes they connect", () => {
    expect(byId("e1")).toMatchObject({ type: "arrow", start: { id: "n1" }, end: { id: "n2" }, label: { text: "writes" } });
  });

  it("points each edge from the border of one node to the border of the other", () => {
    const below = toSkeletons(
      board({
        nodes: {
          n1: { id: "n1", kind: "service", label: "A", at: { x: 0, y: 0 }, by: "agent" },
          n2: { id: "n2", kind: "service", label: "B", at: { x: 0, y: 160 }, by: "agent" },
        },
        edges: { e1: { id: "e1", from: "n1", to: "n2", by: "agent" } },
      }),
    ).find((s) => s.id === "e1") as any;
    const [start, end] = below.points;
    expect([below.x, below.y]).toEqual([NODE_WIDTH / 2, NODE_HEIGHT + 6]);
    expect(start).toEqual([0, 0]);
    expect(end).toEqual([0, 160 - NODE_HEIGHT - 12]);
  });

  it("links a note to what it is about with a dashed line", () => {
    const link = byId("n3~link");
    expect(link).toMatchObject({ strokeStyle: "dashed", start: { id: "n3" }, end: { id: "n1" }, locked: true });
    expect(link.y).toBeLessThanOrEqual(160 - 6);
  });

  it("frames a group around its members, behind them, and names it", () => {
    const skeletons = toSkeletons(sample);
    const frame = byId("g1");
    expect(frame).toMatchObject({ strokeStyle: "dashed", locked: true });
    expect(frame.x).toBeLessThan(0);
    expect(frame.x + frame.width).toBeGreaterThan(240 + NODE_WIDTH);
    expect(skeletons.findIndex((s) => s.id === "g1")).toBeLessThan(skeletons.findIndex((s) => s.id === "n1"));
    expect(byId("g1~label")).toMatchObject({ type: "text", text: "Orders", locked: true });
  });
});

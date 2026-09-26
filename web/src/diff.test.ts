import { describe, expect, it } from "vitest";
import type { Board } from "./model";
import { type CanvasElement, humanOps } from "./diff";

const board: Board = {
  nodes: {
    n1: { id: "n1", kind: "service", label: "A", at: { x: 0, y: 0 }, by: "agent" },
    n2: { id: "n2", kind: "service", label: "B", at: { x: 240, y: 0 }, by: "agent" },
  },
  edges: { e1: { id: "e1", from: "n1", to: "n2", label: "calls", by: "agent" } },
  groups: {},
  annotations: { a1: { id: "a1", stroke: [[0, 0, 0]], zoom: 1, targets: [], by: "human" } },
  turn: 1,
  holder: "human",
  last_seq: 3,
};

const scene: CanvasElement[] = [
  { id: "n1", type: "rectangle", x: 0, y: 0, isDeleted: false },
  { id: "t1", type: "text", x: 10, y: 30, isDeleted: false, text: "A", containerId: "n1" },
  { id: "n2", type: "rectangle", x: 240, y: 0, isDeleted: false },
  { id: "t2", type: "text", x: 250, y: 30, isDeleted: false, text: "B", containerId: "n2" },
  { id: "e1", type: "arrow", x: 160, y: 40, isDeleted: false, startBinding: { elementId: "n1" }, endBinding: { elementId: "n2" } },
  { id: "t3", type: "text", x: 180, y: 30, isDeleted: false, text: "calls", containerId: "e1" },
];

const edit = (id: string, change: Partial<CanvasElement>) =>
  scene.map((e) => (e.id === id ? { ...e, ...change } : e));
const ops = (elements: CanvasElement[]) =>
  humanOps(elements, board, new Set()).map(({ client_id, ...rest }) => rest);

describe("humanOps", () => {
  it("sees nothing to do when the canvas matches the board", () => {
    expect(ops(scene)).toEqual([]);
  });

  it("turns a drag into one move", () => {
    expect(ops(edit("n1", { x: 480, y: 160 }))).toEqual([{ op: "move", id: "n1", to: { x: 480, y: 160 } }]);
  });

  it("turns an erased node or edge into a delete", () => {
    expect(ops(edit("e1", { isDeleted: true }))).toEqual([{ op: "delete", id: "e1" }]);
  });

  it("turns edited text into a relabel of whatever holds it", () => {
    expect(ops(edit("t1", { text: "Checkout" }))).toEqual([{ op: "relabel", id: "n1", label: "Checkout" }]);
    expect(ops(edit("t3", { text: "charges" }))).toEqual([{ op: "relabel", id: "e1", label: "charges" }]);
  });

  it("ignores line breaks Excalidraw adds when it wraps a label", () => {
    expect(ops(edit("t1", { text: "A\n", originalText: "A" }))).toEqual([]);
    expect(ops(edit("t1", { text: "Check\nout", originalText: "Checkout" }))).toEqual([
      { op: "relabel", id: "n1", label: "Checkout" },
    ]);
  });

  it("adds a labelled shape the human drew, using the shape to pick a kind", () => {
    const drawn = [
      ...scene,
      { id: "tmpA", type: "ellipse", x: 500, y: 300, isDeleted: false },
      { id: "tmpT", type: "text", x: 510, y: 330, isDeleted: false, text: "Cache", containerId: "tmpA" },
    ];
    expect(humanOps(drawn, board, new Set())).toEqual([
      { op: "add_node", client_id: "tmpA", kind: "db", label: "Cache", at: { x: 500, y: 300 } },
    ]);
  });

  it("waits for a drawn shape to get a label before adding it", () => {
    expect(ops([...scene, { id: "tmpA", type: "rectangle", x: 500, y: 300, isDeleted: false }])).toEqual([]);
  });

  it("connects two nodes when the human draws an arrow between them", () => {
    const drawn = [
      ...scene,
      { id: "tmpE", type: "arrow", x: 0, y: 0, isDeleted: false, startBinding: { elementId: "n2" }, endBinding: { elementId: "n1" } },
    ];
    expect(humanOps(drawn, board, new Set())).toEqual([{ op: "connect", client_id: "tmpE", from: "n2", to: "n1" }]);
  });

  it("leaves arrows that don't join two nodes on the canvas only", () => {
    const drawn = [...scene, { id: "tmpE", type: "arrow", x: 0, y: 0, isDeleted: false, startBinding: null, endBinding: { elementId: "n1" } }];
    expect(ops(drawn)).toEqual([]);
  });

  it("turns free text into a note", () => {
    const drawn = [...scene, { id: "tmpN", type: "text", x: 50, y: 400, isDeleted: false, text: "what about retries?" }];
    expect(humanOps(drawn, board, new Set())).toEqual([
      { op: "note", client_id: "tmpN", text: "what about retries?", at: { x: 50, y: 400 } },
    ]);
  });

  it("sends a pen stroke as a mark, in board coordinates, with the zoom it was drawn at", () => {
    const drawn = [
      ...scene,
      { id: "tmpS", type: "freedraw", x: 100, y: 50, isDeleted: false, points: [[0, 0], [10, 5], [20, 0]] as [number, number][], pressures: [0.2, 0.4, 0.6], simulatePressure: false },
    ];
    expect(humanOps(drawn, board, new Set(), 2)).toEqual([
      { op: "annotate", client_id: "tmpS", zoom: 2, stroke: [[100, 50, 0, 0.2], [110, 55, 0, 0.4], [120, 50, 0, 0.6]] },
    ]);
  });

  it("leaves pressure out when Excalidraw only simulated it", () => {
    const drawn = [...scene, { id: "tmpS", type: "freedraw", x: 0, y: 0, isDeleted: false, points: [[0, 0], [10, 10]] as [number, number][], pressures: [], simulatePressure: true }];
    expect(humanOps(drawn, board, new Set())).toMatchObject([{ stroke: [[0, 0, 0], [10, 10, 0]] }]);
  });

  it("ignores a pen tap too small to mean anything at the zoom it was made", () => {
    const tap = { id: "tmpS", type: "freedraw", x: 0, y: 0, isDeleted: false, points: [[0, 0], [2, 1]] as [number, number][] };
    expect(ops([...scene, tap])).toEqual([]);
    expect(humanOps([...scene, tap], board, new Set(), 4)).toHaveLength(1);
  });

  it("turns an erased mark into a delete", () => {
    expect(ops([...scene, { id: "a1", type: "freedraw", x: 0, y: 0, isDeleted: true }])).toEqual([{ op: "delete", id: "a1" }]);
  });

  it("leaves a parent's frame alone, since its size comes from its children", () => {
    expect(humanOps(edit("n1", { x: -24, y: -52 }), board, new Set(), 1, new Set(["n1"]))).toEqual([]);
  });

  it("doesn't resend an edit that is still on its way", () => {
    const moved = edit("n1", { x: 480, y: 160 });
    const [first] = humanOps(moved, board, new Set());
    expect(humanOps(moved, board, new Set([first.client_id]))).toEqual([]);
  });
});

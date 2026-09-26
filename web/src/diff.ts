import type { Board, NodeKind, Op, StrokePoint } from "./model";

/** The parts of an Excalidraw element the diff reads. */
export interface CanvasElement {
  id: string;
  type: string;
  x: number;
  y: number;
  isDeleted: boolean;
  text?: string;
  /** The text as typed, before Excalidraw wraps it to fit its container. */
  originalText?: string;
  containerId?: string | null;
  startBinding?: { elementId: string } | null;
  endBinding?: { elementId: string } | null;
  points?: readonly (readonly [number, number])[];
  pressures?: readonly number[];
  simulatePressure?: boolean;
}

const SHAPE_KINDS: Record<string, NodeKind> = {
  rectangle: "component",
  ellipse: "db",
  diamond: "actor",
};

const MODEL_ID = /^[nega]\d+$/;
/** A stroke smaller than this on screen is a stray tap, not a mark. */
const MIN_STROKE_PIXELS = 6;

/**
 * What the human changed on the canvas, as edits for the server. Ops whose client id is still
 * pending are left out so a slow round trip doesn't send them twice.
 */
export function humanOps(
  elements: readonly CanvasElement[],
  board: Board,
  pending: Set<string>,
  zoom = 1,
): Op[] {
  const boundText = new Map<string, string>();
  for (const e of elements) {
    if (e.type === "text" && e.containerId && !e.isDeleted) boundText.set(e.containerId, typed(e));
  }
  const ops: Op[] = [];
  for (const e of elements) {
    const node = board.nodes[e.id];
    if (node) {
      if (e.isDeleted) {
        ops.push({ op: "delete", client_id: `delete:${e.id}`, id: e.id });
      } else if (Math.abs(e.x - node.at.x) > 0.5 || Math.abs(e.y - node.at.y) > 0.5) {
        const to = { x: Math.round(e.x), y: Math.round(e.y) };
        ops.push({ op: "move", client_id: `move:${e.id}:${to.x}:${to.y}`, id: e.id, to });
      }
      continue;
    }
    if (board.edges[e.id] || board.annotations[e.id]) {
      if (e.isDeleted) ops.push({ op: "delete", client_id: `delete:${e.id}`, id: e.id });
      continue;
    }
    if (e.isDeleted || MODEL_ID.test(e.id) || e.id.includes("~")) continue;
    if (e.type === "text" && e.containerId) {
      const owner = board.nodes[e.containerId] ?? board.edges[e.containerId];
      const text = typed(e);
      if (owner && (owner.label ?? "") !== text && text.trim()) {
        ops.push({ op: "relabel", client_id: `relabel:${owner.id}:${text}`, id: owner.id, label: text });
      }
      continue;
    }
    const drawn = drawnOp(e, board, boundText.get(e.id), zoom);
    if (drawn) ops.push(drawn);
  }
  return ops.filter((op) => !pending.has(op.client_id));
}

function drawnOp(e: CanvasElement, board: Board, label: string | undefined, zoom: number): Op | null {
  if (e.type === "freedraw") {
    const points = e.points ?? [];
    const span = (axis: 0 | 1) => Math.max(...points.map((p) => p[axis])) - Math.min(...points.map((p) => p[axis]));
    if (points.length < 2 || Math.max(span(0), span(1)) * zoom < MIN_STROKE_PIXELS) return null;
    const stroke = points.map(([px, py], i): StrokePoint => {
      const point: StrokePoint = [e.x + px, e.y + py, 0];
      return e.simulatePressure || e.pressures?.[i] === undefined ? point : [...point, e.pressures[i]];
    });
    return { op: "annotate", client_id: e.id, stroke, zoom };
  }
  const kind = SHAPE_KINDS[e.type];
  if (kind) {
    if (!label?.trim()) return null;
    return { op: "add_node", client_id: e.id, kind, label, at: { x: Math.round(e.x), y: Math.round(e.y) } };
  }
  if (e.type === "arrow") {
    const from = e.startBinding?.elementId;
    const to = e.endBinding?.elementId;
    if (!from || !to || !board.nodes[from] || !board.nodes[to]) return null;
    return { op: "connect", client_id: e.id, from, to, ...(label?.trim() ? { label } : {}) };
  }
  if (e.type === "text" && typed(e).trim()) {
    return { op: "note", client_id: e.id, text: typed(e), at: { x: Math.round(e.x), y: Math.round(e.y) } };
  }
  return null;
}

function typed(e: CanvasElement): string {
  return e.originalText ?? e.text ?? "";
}

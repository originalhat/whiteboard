import {
  type Board,
  type BoardAnnotation,
  type BoardNode,
  LOWEST_ALTITUDE,
  NODE_HEIGHT,
  NODE_WIDTH,
} from "./model";
import { viewAt } from "./view";

export const AGENT_COLOR = "#1971c2";
export const HUMAN_COLOR = "#1e1e1e";
const QUIET_COLOR = "#868e96";
const NOTE_FILL = "#fff3bf";
export const RECENT_FILL = "#e7f5ff";
const RECENT_STROKE_WIDTH = 3;
const GROUP_PADDING = 24;
const GROUP_LABEL_SPACE = 28;
const NODE_FONT_SIZE = 16;
const EDGE_FONT_SIZE = 14;

export type Skeleton = { id: string; type: string } & Record<string, unknown>;

/** The board as Excalidraw element skeletons. Ids match the board's, so edits map straight back. */
export function toSkeletons(
  board: Board,
  recent: Set<string> = new Set(),
  altitude: number = LOWEST_ALTITUDE,
): Skeleton[] {
  const view = viewAt(board, altitude);
  const nodes = Object.values(board.nodes).filter(
    (n) => view.visible.has(n.id) && (!n.attached_to || !board.nodes[n.attached_to] || view.visible.has(n.attached_to)),
  );
  const rects = new Map<string, Rect>();
  const rectOf = (id: string): Rect => {
    const known = rects.get(id);
    if (known) return known;
    const children = nodes.filter((n) => n.parent === id).map((n) => rectOf(n.id));
    const rect = view.containers.has(id) ? frameAround(children) : nodeRect(board.nodes[id].at);
    rects.set(id, rect);
    return rect;
  };
  const depth = (id: string): number => (board.nodes[id]?.parent ? 1 + depth(board.nodes[id].parent as string) : 0);
  const containers = [...view.containers].sort((a, b) => depth(a) - depth(b));
  const skeletons: Skeleton[] = [
    ...Object.values(board.groups).flatMap((group) => groupFrame(board, group.id, view.visible)),
    ...containers.flatMap((id) => containerFrame(board.nodes[id], rectOf(id))),
    ...nodes.filter((n) => !view.containers.has(n.id)).map(shape),
    ...Object.values(board.edges)
      .filter((edge) => view.visible.has(edge.from) && view.visible.has(edge.to))
      .map((edge) => ({
        id: edge.id,
        type: "arrow",
        ...connector(rectOf(edge.from), rectOf(edge.to)),
        strokeColor: colorOf(edge.by),
        start: { id: edge.from },
        end: { id: edge.to },
        ...(edge.label ? { label: { text: edge.label, fontSize: EDGE_FONT_SIZE } } : {}),
      })),
    ...view.rollups.map((rollup) => {
      const labels = rollup.via.map((id) => board.edges[id]?.label).filter(Boolean);
      const text = rollup.via.length === 1 ? labels[0] : `${rollup.via.length} edges`;
      return {
        id: `r~${rollup.from}~${rollup.to}`,
        type: "arrow",
        ...connector(rectOf(rollup.from), rectOf(rollup.to)),
        strokeColor: QUIET_COLOR,
        strokeStyle: "dashed",
        locked: true,
        start: { id: rollup.from },
        end: { id: rollup.to },
        ...(text ? { label: { text, fontSize: EDGE_FONT_SIZE } } : {}),
      };
    }),
    ...nodes.flatMap((node) => noteLink(board, node)),
    ...Object.values(board.annotations).map(markStroke),
  ];
  return skeletons.map((s) => (recent.has(s.id) ? standOut(s) : s));
}

function standOut(skeleton: Skeleton): Skeleton {
  const fill = skeleton.backgroundColor === "transparent" ? { backgroundColor: RECENT_FILL, fillStyle: "solid" } : {};
  return { ...skeleton, ...fill, strokeWidth: RECENT_STROKE_WIDTH };
}

/**
 * A complete Excalidraw freedraw element: the skeleton converter passes these through untouched.
 * Excalidraw only records pressure per point, so a missing value falls back to a medium press.
 */
function markStroke(mark: BoardAnnotation): Skeleton {
  const [x0, y0] = mark.stroke[0] ?? [0, 0];
  const points = mark.stroke.map(([x, y]) => [x - x0, y - y0]);
  const xs = points.map(([x]) => x);
  const ys = points.map(([, y]) => y);
  return {
    id: mark.id,
    type: "freedraw",
    x: x0,
    y: y0,
    width: Math.max(...xs) - Math.min(...xs),
    height: Math.max(...ys) - Math.min(...ys),
    points,
    pressures: mark.stroke.map((p) => p[3] ?? 0.5),
    simulatePressure: false,
    lastCommittedPoint: null,
    strokeColor: colorOf(mark.by),
    backgroundColor: "transparent",
    fillStyle: "solid",
    strokeWidth: 2,
    strokeStyle: "solid",
    roughness: 1,
    opacity: 100,
    angle: 0,
    roundness: null,
    seed: 1,
    version: 1,
    versionNonce: 1,
    index: null,
    isDeleted: false,
    groupIds: [],
    frameId: null,
    boundElements: null,
    updated: 1,
    link: null,
    locked: false,
  };
}

function colorOf(by: string) {
  return by === "agent" ? AGENT_COLOR : HUMAN_COLOR;
}

function shape(node: BoardNode): Skeleton {
  const color = colorOf(node.by);
  const base = {
    id: node.id,
    x: node.at.x,
    y: node.at.y,
    width: NODE_WIDTH,
    height: NODE_HEIGHT,
    strokeColor: color,
    backgroundColor: "transparent",
    fillStyle: "solid",
    roundness: { type: 3 },
    label: { text: node.label, strokeColor: color, fontSize: NODE_FONT_SIZE },
  };
  switch (node.kind) {
    case "db":
      return { ...base, type: "ellipse", roundness: null };
    case "actor":
      return { ...base, type: "diamond", roundness: null };
    case "component":
      return { ...base, type: "rectangle", roundness: null };
    case "queue":
      return { ...base, type: "rectangle", backgroundColor: "#d0ebff", fillStyle: "hachure" };
    case "external":
      return { ...base, type: "rectangle", strokeStyle: "dashed" };
    case "note":
      return { ...base, type: "rectangle", backgroundColor: NOTE_FILL, roundness: null };
    default:
      return { ...base, type: "rectangle" };
  }
}

/** A parent seen from below: a frame around its children, named at the top. */
function containerFrame(node: BoardNode, rect: Rect): Skeleton[] {
  const color = colorOf(node.by);
  return [
    { id: node.id, type: "rectangle", ...rect, strokeColor: color, backgroundColor: "#f8f9fa", fillStyle: "solid", roundness: { type: 3 }, locked: true },
    { id: `${node.id}~label`, type: "text", x: rect.x + 10, y: rect.y + 8, text: node.label, fontSize: NODE_FONT_SIZE, strokeColor: color, locked: true },
  ];
}

function frameAround(children: Rect[]): Rect {
  if (children.length === 0) return { x: 0, y: 0, width: NODE_WIDTH, height: NODE_HEIGHT };
  const x = Math.min(...children.map((r) => r.x)) - GROUP_PADDING;
  const y = Math.min(...children.map((r) => r.y)) - GROUP_PADDING - GROUP_LABEL_SPACE;
  const right = Math.max(...children.map((r) => r.x + r.width)) + GROUP_PADDING;
  const bottom = Math.max(...children.map((r) => r.y + r.height)) + GROUP_PADDING;
  return { x, y, width: right - x, height: bottom - y };
}

function groupFrame(board: Board, id: string, visible: Set<string>): Skeleton[] {
  const group = board.groups[id];
  const members = group.members.filter((m) => visible.has(m)).map((m) => board.nodes[m]).filter(Boolean);
  if (members.length === 0) return [];
  const xs = members.map((n) => n.at.x);
  const ys = members.map((n) => n.at.y);
  const x = Math.min(...xs) - GROUP_PADDING;
  const y = Math.min(...ys) - GROUP_PADDING - GROUP_LABEL_SPACE;
  const width = Math.max(...xs) + NODE_WIDTH + GROUP_PADDING - x;
  const height = Math.max(...ys) + NODE_HEIGHT + GROUP_PADDING - y;
  return [
    { id, type: "rectangle", x, y, width, height, strokeColor: QUIET_COLOR, strokeStyle: "dashed", backgroundColor: "transparent", locked: true },
    { id: `${id}~label`, type: "text", x: x + 8, y: y + 6, text: group.label, fontSize: 16, strokeColor: QUIET_COLOR, locked: true },
  ];
}

function noteLink(board: Board, note: BoardNode): Skeleton[] {
  const target = note.attached_to;
  if (!target) return [];
  const targetRect = board.nodes[target]
    ? nodeRect(board.nodes[target].at)
    : board.edges[target]
      ? edgeMidpoint(board, target)
      : null;
  if (!targetRect) return [];
  const link = {
    id: `${note.id}~link`,
    type: "arrow",
    ...connector(nodeRect(note.at), targetRect),
    strokeColor: QUIET_COLOR,
    strokeStyle: "dashed",
    endArrowhead: null,
    locked: true,
    start: { id: note.id },
  };
  return [board.nodes[target] ? { ...link, end: { id: target } } : link];
}

interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

const GAP = 6;

function nodeRect(at: { x: number; y: number }): Rect {
  return { x: at.x, y: at.y, width: NODE_WIDTH, height: NODE_HEIGHT };
}

function edgeMidpoint(board: Board, id: string): Rect {
  const edge = board.edges[id];
  const [a, b] = [board.nodes[edge.from].at, board.nodes[edge.to].at];
  return { x: (a.x + b.x) / 2 + NODE_WIDTH / 2, y: (a.y + b.y) / 2 + NODE_HEIGHT / 2, width: 0, height: 0 };
}

/** A straight line between two boxes, from border to border, leaving a small gap at each end. */
function connector(from: Rect, to: Rect) {
  const center = (r: Rect) => ({ x: r.x + r.width / 2, y: r.y + r.height / 2 });
  const [a, b] = [center(from), center(to)];
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const length = Math.hypot(dx, dy) || 1;
  const exit = (r: Rect, sign: number) => {
    const reach = Math.min(
      dx === 0 ? Infinity : r.width / 2 / Math.abs(dx),
      dy === 0 ? Infinity : r.height / 2 / Math.abs(dy),
    );
    const t = (Number.isFinite(reach) ? reach : 0) + GAP / length;
    return { x: sign * dx * t, y: sign * dy * t };
  };
  const out = exit(from, 1);
  const back = exit(to, -1);
  const start = { x: a.x + out.x, y: a.y + out.y };
  const end = { x: b.x + back.x, y: b.y + back.y };
  return {
    x: start.x,
    y: start.y,
    width: Math.abs(end.x - start.x),
    height: Math.abs(end.y - start.y),
    points: [
      [0, 0],
      [end.x - start.x, end.y - start.y],
    ],
  };
}

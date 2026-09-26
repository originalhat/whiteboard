export type Actor = "human" | "agent";
export type NodeKind = "service" | "db" | "queue" | "actor" | "external" | "component" | "note";

export interface Point {
  x: number;
  y: number;
}

export interface BoardNode {
  id: string;
  kind: NodeKind;
  label: string;
  at: Point;
  attached_to?: string;
  by: Actor;
}

export interface BoardEdge {
  id: string;
  from: string;
  to: string;
  label?: string;
  by: Actor;
}

export interface BoardGroup {
  id: string;
  label: string;
  members: string[];
  by: Actor;
}

export type StrokePoint = [number, number, number] | [number, number, number, number];

export interface Target {
  relation: "encloses" | "connects" | "strikes";
  items: string[];
}

export interface BoardAnnotation {
  id: string;
  stroke: StrokePoint[];
  zoom: number;
  targets: Target[];
  text?: string;
  by: Actor;
}

export interface Board {
  nodes: Record<string, BoardNode>;
  edges: Record<string, BoardEdge>;
  groups: Record<string, BoardGroup>;
  annotations: Record<string, BoardAnnotation>;
  turn: number;
  holder: Actor;
  last_seq: number;
}

export interface Message {
  seq: number;
  turn: number;
  actor: Actor;
  text: string;
}

export interface Presence {
  text: string;
  at: number;
}

export interface Snapshot {
  meta: { id: string; title: string; source?: string };
  board: Board;
  seq: number;
  presence: Presence | null;
  messages: Message[];
  agent_turn_items: string[];
}

export interface WbEvent {
  seq: number;
  actor: Actor;
  turn: number;
  kind: string;
  payload: Record<string, unknown>;
  client_id?: string;
}

export type Op =
  | { op: "add_node"; client_id: string; kind: NodeKind; label: string; at: Point }
  | { op: "note"; client_id: string; text: string; at: Point; on?: string }
  | { op: "move"; client_id: string; id: string; to: Point }
  | { op: "connect"; client_id: string; from: string; to: string; label?: string }
  | { op: "relabel"; client_id: string; id: string; label: string }
  | { op: "delete"; client_id: string; id: string }
  | { op: "annotate"; client_id: string; stroke: StrokePoint[]; zoom: number }
  | { op: "pass_turn"; client_id: string; message?: string; pointing_at?: string[] };

export const NODE_WIDTH = 200;
export const NODE_HEIGHT = 80;

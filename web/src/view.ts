import { type Board, type BoardNode, LOWEST_ALTITUDE } from "./model";

export interface Rollup {
  from: string;
  to: string;
  via: string[];
}

export interface View {
  visible: Set<string>;
  /** Visible nodes with visible children, drawn as a frame around them. */
  containers: Set<string>;
  rollups: Rollup[];
}

const byNumber = (a: string, b: string) => Number(a.slice(1)) - Number(b.slice(1));

export function shows(node: BoardNode, altitude: number): boolean {
  return node.altitude === undefined || node.altitude <= altitude;
}

/** Mirrors the core's view: what shows at an altitude and how hidden edges roll up. */
export function viewAt(board: Board, altitude: number = LOWEST_ALTITUDE): View {
  const nodes = Object.values(board.nodes);
  const visible = new Set(nodes.filter((n) => shows(n, altitude)).map((n) => n.id).sort(byNumber));
  const containers = new Set(
    nodes
      .filter((n) => visible.has(n.id) && n.parent && visible.has(n.parent))
      .map((n) => n.parent as string)
      .sort(byNumber),
  );
  const standIn = (id: string): string | undefined => {
    let next: string | undefined = id;
    while (next) {
      if (visible.has(next)) return next;
      next = board.nodes[next]?.parent;
    }
    return undefined;
  };
  const edges = Object.values(board.edges).sort((a, b) => byNumber(a.id, b.id));
  const drawn = new Set(
    edges.filter((e) => visible.has(e.from) && visible.has(e.to)).map((e) => `${e.from}>${e.to}`),
  );
  const grouped = new Map<string, Rollup>();
  for (const edge of edges) {
    if (visible.has(edge.from) && visible.has(edge.to)) continue;
    const from = standIn(edge.from);
    const to = standIn(edge.to);
    if (!from || !to || from === to || drawn.has(`${from}>${to}`)) continue;
    const key = `${from}>${to}`;
    const rollup = grouped.get(key) ?? { from, to, via: [] };
    rollup.via.push(edge.id);
    grouped.set(key, rollup);
  }
  return { visible, containers, rollups: [...grouped.values()] };
}

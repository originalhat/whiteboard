import type { WbEvent } from "./model";

/** What the agent added or changed in its latest turn. */
export interface Recent {
  turn: number;
  items: Set<string>;
}

export function afterEvent(recent: Recent, event: WbEvent): Recent {
  const id = event.payload.id;
  if (event.actor !== "agent" || typeof id !== "string") return recent;
  if (event.kind === "item.deleted") {
    const items = new Set(recent.items);
    items.delete(id);
    return { ...recent, items };
  }
  if (event.turn <= 1) return { turn: event.turn, items: new Set() };
  const items = event.turn === recent.turn ? new Set(recent.items) : new Set<string>();
  items.add(id);
  return { turn: event.turn, items };
}

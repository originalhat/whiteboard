import type { Board, Op, Presence, Snapshot, WbEvent } from "./model";

export interface Location {
  boardId: string;
  token: string;
}

export function readLocation(): Location | null {
  const match = window.location.pathname.match(/^\/b\/([^/]+)/);
  const token = new URLSearchParams(window.location.search).get("t");
  return match && token ? { boardId: match[1], token } : null;
}

export interface Handlers {
  snapshot(snapshot: Snapshot): void;
  change(event: WbEvent, board: Board): void;
  presence(presence: Presence | null): void;
  connection(online: boolean): void;
}

/** Follows the board. EventSource reconnects on its own and the server resends a snapshot. */
export function follow({ boardId, token }: Location, handlers: Handlers): () => void {
  const source = new EventSource(`/api/boards/${boardId}/events?t=${encodeURIComponent(token)}`);
  const data = (e: Event) => JSON.parse((e as MessageEvent).data);
  source.addEventListener("snapshot", (e) => {
    handlers.connection(true);
    handlers.snapshot(data(e));
  });
  source.addEventListener("change", (e) => {
    const { event, board } = data(e);
    handlers.change(event, board);
  });
  source.addEventListener("presence", (e) => handlers.presence(data(e).presence));
  source.addEventListener("resync", () => {
    source.close();
    window.location.reload();
  });
  source.onerror = () => handlers.connection(false);
  return () => source.close();
}

export async function send({ boardId, token }: Location, op: Op): Promise<{ error?: string }> {
  try {
    const response = await fetch(`/api/boards/${boardId}/ops?t=${encodeURIComponent(token)}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(op),
    });
    return response.ok ? {} : { error: (await response.json()).error ?? response.statusText };
  } catch (e) {
    return { error: String(e) };
  }
}

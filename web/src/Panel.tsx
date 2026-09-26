import { type FormEvent, useEffect, useRef, useState } from "react";
import type { Message, Presence, Snapshot } from "./model";

interface Props {
  snapshot: Snapshot | null;
  messages: Message[];
  presence: Presence | null;
  online: boolean;
  error: string | null;
  recentCount: number;
  altitude: number;
  onAltitude(altitude: number): void;
  onDismissError(): void;
  onHandOver(message: string | undefined): Promise<boolean>;
}

const ALTITUDES = ["", "Whole system", "Containers", "Components", "Code"];

export function Panel({
  snapshot,
  messages,
  presence,
  online,
  error,
  recentCount,
  altitude,
  onAltitude,
  onDismissError,
  onHandOver,
}: Props) {
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const list = useRef<HTMLOListElement>(null);
  const board = snapshot?.board;
  const yours = board?.holder === "human";

  const handOver = async () => {
    if (!yours || sending) return;
    setSending(true);
    if (await onHandOver(draft.trim() || undefined)) setDraft("");
    setSending(false);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
        e.preventDefault();
        void handOver();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });

  useEffect(() => {
    list.current?.scrollTo({ top: list.current.scrollHeight });
  }, [messages.length]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    void handOver();
  };

  return (
    <aside className="panel">
      <header>
        <h1>{snapshot?.meta.title ?? "Whiteboard"}</h1>
        {snapshot?.meta.source && <div className="source">{snapshot.meta.source}</div>}
      </header>
      <div className={`turn ${yours ? "yours" : "agents"}`}>
        {board ? (yours ? "Your turn" : "Agent's turn") : "Connecting…"}
        {board && <span className="turn-number">turn {board.turn}</span>}
      </div>
      {!yours && presence && <div className="status">{presence.text}</div>}
      {board && Object.values(board.nodes).some((n) => n.altitude !== undefined) && (
        <label className="altitude">
          <span>Altitude</span>
          <input
            type="range"
            min={1}
            max={ALTITUDES.length - 1}
            value={altitude}
            onChange={(e) => onAltitude(Number(e.target.value))}
          />
          <span className="altitude-name">{ALTITUDES[altitude]}</span>
        </label>
      )}
      {!online && <div className="offline">Reconnecting…</div>}
      {recentCount > 0 && (
        <div className="legend">
          <span className="swatch" /> Shaded: {recentCount === 1 ? "1 thing" : `${recentCount} things`} the agent changed last turn
        </div>
      )}
      <ol className="messages" ref={list}>
        {messages.length === 0 && <li className="empty">No messages yet.</li>}
        {messages.map((m) => (
          <li key={m.seq} className={m.actor}>
            <b>{m.actor === "agent" ? "Agent" : "You"}</b>
            <p>{m.text}</p>
          </li>
        ))}
      </ol>
      {error && (
        <button type="button" className="error" onClick={onDismissError}>
          {error}
        </button>
      )}
      <form onSubmit={submit}>
        <textarea
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder={
            yours
              ? "What should the agent look at? Circle things with the pen, or select them before handing over."
              : "You can keep drawing while the agent works"
          }
          rows={4}
        />
        <button type="submit" disabled={!yours || sending}>
          Hand over <kbd>⌘↵</kbd>
        </button>
      </form>
    </aside>
  );
}

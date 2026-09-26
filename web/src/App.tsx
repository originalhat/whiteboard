import { Excalidraw } from "@excalidraw/excalidraw";
import type { ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { useEffect, useMemo, useState } from "react";
import { type Location, follow, readLocation } from "./api";
import { CommentBox, describeMark } from "./CommentBox";
import { LOWEST_ALTITUDE, type Message, type Presence, type Snapshot, type WbEvent } from "./model";
import { Panel } from "./Panel";
import { CanvasSync, type NewMark } from "./sync";

export function App() {
  const location = useMemo(readLocation, []);
  if (!location) {
    return (
      <div className="missing">
        Open this canvas with the link from <code>wb open</code>.
      </div>
    );
  }
  return <Canvas location={location} />;
}

function Canvas({ location }: { location: Location }) {
  const [api, setApi] = useState<ExcalidrawImperativeAPI | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [presence, setPresence] = useState<Presence | null>(null);
  const [online, setOnline] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [newMark, setNewMark] = useState<NewMark | null>(null);
  const [recentCount, setRecentCount] = useState(0);
  const [altitude, setAltitude] = useState(LOWEST_ALTITUDE);
  const fontsReady = useFonts(api);
  const sync = useMemo(
    () => (api && fontsReady ? new CanvasSync(api, location, setError, setNewMark, setRecentCount) : null),
    [api, fontsReady, location],
  );

  useEffect(() => {
    if (!sync) return;
    return follow(location, {
      snapshot(s) {
        setSnapshot(s);
        setMessages(s.messages);
        setPresence(s.presence);
        sync.setBoard(s.board, undefined, s.agent_turn_items);
      },
      change(event, board) {
        setSnapshot((prev) => prev && { ...prev, board });
        const message = messageOf(event);
        if (message) setMessages((ms) => [...ms, message]);
        sync.setBoard(board, event);
      },
      presence: setPresence,
      connection: setOnline,
    });
  }, [sync, location]);

  useEffect(() => sync?.setAltitude(altitude), [sync, altitude]);

  useEffect(() => {
    if (snapshot) document.title = `${snapshot.meta.title} · Whiteboard`;
  }, [snapshot?.meta.title]);

  return (
    <div className="app">
      <div className="canvas">
        {newMark && snapshot && (
          <CommentBox
            mark={newMark}
            describe={describeMark(snapshot.board, newMark.id)}
            onSave={(text) => {
              sync?.comment(newMark.id, text);
              setNewMark(null);
            }}
            onClose={() => setNewMark(null)}
          />
        )}
        <Excalidraw
          excalidrawAPI={setApi}
          onChange={(elements, appState) => sync?.onChange(elements, appState)}
          UIOptions={{ canvasActions: { loadScene: false, saveToActiveFile: false, export: false } }}
        />
      </div>
      <Panel
        snapshot={snapshot}
        messages={messages}
        presence={presence}
        online={online}
        error={error}
        recentCount={recentCount}
        altitude={altitude}
        onAltitude={setAltitude}
        onDismissError={() => setError(null)}
        onHandOver={(message) => sync?.passTurn(message) ?? Promise.resolve(false)}
      />
    </div>
  );
}

/** Excalidraw measures label text when elements are created, so wait for its hand-drawn font. */
function useFonts(api: ExcalidrawImperativeAPI | null): boolean {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    if (!api) return;
    const loads = [14, 16].map((size) => document.fonts.load(`${size}px Excalifont`));
    const timeout = new Promise((resolve) => setTimeout(resolve, 3000));
    void Promise.race([Promise.all(loads), timeout]).then(() => setReady(true));
  }, [api]);
  return ready;
}

function messageOf(event: WbEvent): Message | null {
  const text =
    event.kind === "message.sent"
      ? event.payload.text
      : event.kind === "turn.passed"
        ? event.payload.message
        : undefined;
  return typeof text === "string" ? { seq: event.seq, turn: event.turn, actor: event.actor, text } : null;
}

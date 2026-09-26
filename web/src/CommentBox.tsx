import { type KeyboardEvent, useState } from "react";
import type { Board } from "./model";

const VERBS = { encloses: "circled", connects: "connected", strikes: "struck through" } as const;

/** How the server read a mark, e.g. `circled CreatePayment`. */
export function describeMark(board: Board, id: string): string {
  const mark = board.annotations[id];
  if (!mark || mark.targets.length === 0) return "a mark";
  const name = (item: string) =>
    board.nodes[item]?.label ?? board.edges[item]?.label ?? board.groups[item]?.label ?? item;
  return mark.targets.map((t) => `${VERBS[t.relation]} ${t.items.map(name).join(" and ")}`).join("; ");
}

interface Props {
  mark: { id: string; x: number; y: number };
  describe: string;
  onSave(text: string): void;
  onClose(): void;
}

/** A real text field, so iPad Scribble and keyboard dictation work in it. */
export function CommentBox({ mark, describe, onSave, onClose }: Props) {
  const [text, setText] = useState("");
  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    e.stopPropagation();
    if (e.key === "Escape") onClose();
    if (e.key === "Enter" && !(e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      if (text.trim()) onSave(text.trim());
      else onClose();
    }
  };
  return (
    <div className="comment-box" style={{ left: mark.x + 12, top: mark.y + 12 }}>
      <div className="comment-reading">You {describe}</div>
      <input
        autoFocus
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKeyDown}
        onBlur={() => (text.trim() ? onSave(text.trim()) : onClose())}
        placeholder="Add a comment (Enter)"
      />
    </div>
  );
}

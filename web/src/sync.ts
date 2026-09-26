import { CaptureUpdateAction, convertToExcalidrawElements } from "@excalidraw/excalidraw";
import type { ExcalidrawElement } from "@excalidraw/excalidraw/element/types";
import type { AppState, ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { type Location, send } from "./api";
import { type CanvasElement, humanOps } from "./diff";
import type { Board, Op, WbEvent } from "./model";
import { afterEvent, type Recent } from "./recent";
import { toSkeletons } from "./scene";

const FLASH_COLOR = "#f08c00";
const FLASH_MS = 900;
const PENDING_MS = 5000;
const DERIVED = /^[nega]\d+$|~/;
const MODEL_ID = /^[nega]\d+$/;

/** Where a freshly drawn mark ended on screen, so a comment box can open beside it. */
export interface NewMark {
  id: string;
  x: number;
  y: number;
}

/**
 * Keeps the Excalidraw scene in step with the board. Remote updates wait while the human is
 * mid-gesture or has edits in flight, so nothing jumps under their pointer.
 */
export class CanvasSync {
  private board: Board | null = null;
  private queued = false;
  private interacting = false;
  private pending = new Map<string, number>();
  private superseded = new Set<string>();
  private flashUntil = new Map<string, number>();
  private fitted = false;
  private zoom = 1;
  private pointingAt: string[] = [];
  private recent: Recent = { turn: 0, items: new Set() };

  constructor(
    private api: ExcalidrawImperativeAPI,
    private location: Location,
    private onError: (message: string) => void,
    private onNewMark: (mark: NewMark) => void,
    private onRecent: (count: number) => void,
  ) {}

  setBoard(board: Board, event?: WbEvent, recentItems?: string[]) {
    this.board = board;
    if (recentItems) this.recent = { turn: board.holder === "agent" ? board.turn : board.turn - 1, items: new Set(recentItems) };
    if (event) this.recent = afterEvent(this.recent, event);
    this.onRecent(this.recent.items.size);
    if (event?.client_id) {
      const key = event.client_id.split("#")[0];
      const ours = this.pending.delete(key);
      this.superseded.add(key);
      if (ours && event.kind === "annotation.added") this.announceMark(board, String(event.payload.id));
    }
    if (event?.actor === "agent") this.flash(event);
    this.render();
  }

  comment(id: string, text: string) {
    void this.dispatch({ op: "relabel", client_id: `comment:${id}`, id, label: text });
  }

  onChange(elements: readonly ExcalidrawElement[], appState: AppState) {
    this.interacting =
      appState.cursorButton === "down" ||
      !!appState.editingTextElement ||
      !!appState.newElement ||
      !!appState.editingLinearElement ||
      appState.isResizing ||
      appState.isRotating ||
      appState.selectedElementsAreBeingDragged;
    this.zoom = appState.zoom.value;
    this.pointingAt = pointedAt(elements, appState);
    if (this.interacting || !this.board) return;
    this.expirePending();
    const canvas = elements as unknown as CanvasElement[];
    for (const op of humanOps(canvas, this.board, new Set(this.pending.keys()), this.zoom)) {
      void this.dispatch(op);
    }
    if (this.queued && this.pending.size === 0) this.render();
  }

  async passTurn(message: string | undefined): Promise<boolean> {
    const turn = this.board?.turn ?? 0;
    const pointing_at = this.pointingAt.length > 0 ? this.pointingAt : undefined;
    const result = await send(this.location, { op: "pass_turn", client_id: `turn:${turn}#${nonce()}`, message, pointing_at });
    if (result.error) this.onError(result.error);
    return !result.error;
  }

  private async dispatch(op: Op) {
    const key = op.client_id;
    this.pending.set(key, Date.now());
    const result = await send(this.location, { ...op, client_id: `${key}#${nonce()}` } as Op);
    if (result.error) {
      this.pending.delete(key);
      this.onError(result.error);
      this.render();
    }
  }

  private render() {
    const board = this.board;
    if (!board) return;
    if (this.interacting || this.pending.size > 0) {
      this.queued = true;
      return;
    }
    this.queued = false;
    const now = Date.now();
    const skeletons = toSkeletons(board, this.recent.items).map((s) =>
      (this.flashUntil.get(s.id) ?? 0) > now ? { ...s, strokeColor: FLASH_COLOR, strokeWidth: 3 } : s,
    );
    const modelElements = convertToExcalidrawElements(skeletons as never, { regenerateIds: false }).map(
      (e) => ({ ...e, seed: seedFor(e.id) }),
    );
    const local = this.api.getSceneElementsIncludingDeleted().filter((e) => !e.isDeleted && this.isLocal(e));
    this.api.updateScene({ elements: [...modelElements, ...local], captureUpdate: CaptureUpdateAction.NEVER });
    if (!this.fitted && modelElements.length > 0) {
      this.fitted = true;
      this.api.scrollToContent(modelElements, { fitToContent: true });
    }
  }

  /** Elements the human drew that the board doesn't hold yet. */
  private isLocal(e: ExcalidrawElement): boolean {
    const container = "containerId" in e ? (e.containerId as string | null) : null;
    if (DERIVED.test(e.id) || (container && DERIVED.test(container))) return false;
    return !this.superseded.has(e.id) && !(container && this.superseded.has(container));
  }

  private announceMark(board: Board, id: string) {
    const mark = board.annotations[id];
    const last = mark?.stroke.at(-1);
    if (!last) return;
    const { scrollX, scrollY, zoom } = this.api.getAppState();
    this.onNewMark({ id, x: (last[0] + scrollX) * zoom.value, y: (last[1] + scrollY) * zoom.value });
  }

  private flash(event: WbEvent) {
    const ids = ["id", "from", "to"]
      .map((key) => event.payload[key])
      .filter((v): v is string => typeof v === "string");
    const until = Date.now() + FLASH_MS;
    for (const id of ids) this.flashUntil.set(id, until);
    if (ids.length > 0) setTimeout(() => this.render(), FLASH_MS + 20);
  }

  private expirePending() {
    const cutoff = Date.now() - PENDING_MS;
    for (const [key, at] of this.pending) if (at < cutoff) this.pending.delete(key);
  }
}

/** The board items selected on the canvas; a selected label counts as the thing it labels. */
function pointedAt(elements: readonly ExcalidrawElement[], appState: AppState): string[] {
  const byId = new Map(elements.map((e) => [e.id, e]));
  const ids = new Set<string>();
  for (const id of Object.keys(appState.selectedElementIds)) {
    const element = byId.get(id);
    const container = element && "containerId" in element ? element.containerId : null;
    const target = container ?? id;
    if (MODEL_ID.test(target)) ids.add(target);
  }
  return [...ids];
}

function nonce(): string {
  return Math.random().toString(36).slice(2, 10);
}

function seedFor(id: string): number {
  let hash = 2166136261;
  for (const c of id) hash = Math.imul(hash ^ c.charCodeAt(0), 16777619);
  return Math.abs(hash) % 2 ** 31 || 1;
}

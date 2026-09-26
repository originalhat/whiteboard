import { CaptureUpdateAction, convertToExcalidrawElements } from "@excalidraw/excalidraw";
import type { ExcalidrawElement } from "@excalidraw/excalidraw/element/types";
import type { AppState, ExcalidrawImperativeAPI } from "@excalidraw/excalidraw/types";
import { type Location, send } from "./api";
import { type CanvasElement, humanOps } from "./diff";
import type { Board, Op, WbEvent } from "./model";
import { toSkeletons } from "./scene";

const FLASH_COLOR = "#f08c00";
const FLASH_MS = 900;
const PENDING_MS = 5000;
const DERIVED = /^[nega]\d+$|~/;

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

  constructor(
    private api: ExcalidrawImperativeAPI,
    private location: Location,
    private onError: (message: string) => void,
  ) {}

  setBoard(board: Board, event?: WbEvent) {
    this.board = board;
    if (event?.client_id) {
      const key = event.client_id.split("#")[0];
      this.pending.delete(key);
      this.superseded.add(key);
    }
    if (event?.actor === "agent") this.flash(event);
    this.render();
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
    if (this.interacting || !this.board) return;
    this.expirePending();
    for (const op of humanOps(elements as unknown as CanvasElement[], this.board, new Set(this.pending.keys()))) {
      void this.dispatch(op);
    }
    if (this.queued && this.pending.size === 0) this.render();
  }

  async passTurn(message: string | undefined): Promise<boolean> {
    const turn = this.board?.turn ?? 0;
    const result = await send(this.location, { op: "pass_turn", client_id: `turn:${turn}#${nonce()}`, message });
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
    const skeletons = toSkeletons(board).map((s) =>
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

function nonce(): string {
  return Math.random().toString(36).slice(2, 10);
}

function seedFor(id: string): number {
  let hash = 2166136261;
  for (const c of id) hash = Math.imul(hash ^ c.charCodeAt(0), 16777619);
  return Math.abs(hash) % 2 ** 31 || 1;
}

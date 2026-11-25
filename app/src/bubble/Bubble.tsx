// The floating drop target: a small glass orb that stays on top of other
// windows. Drag it anywhere. Drag a file near it and a ring of four actions
// opens; release the file on the action you want, or on the centre for the
// full list.
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LogicalPosition, LogicalSize, currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { dirname, join } from "@tauri-apps/api/path";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Icon, type IconName } from "../components/Icon";
import { Mark } from "../components/Mark";
import { convertPdf, errorMessage, imagesToPdf, isImage } from "../lib/tauri";

/** Idle window size; must match BUBBLE_SIZE in src-tauri/src/shell.rs. */
const SIZE = 76;
/** Window size while the ring is open. */
const RING = 260;
const MENU_WIDTH = 280;

type OpenAction = "split" | "merge" | "redact" | "edit" | "convert";
type Dir = "n" | "w" | "e" | "s";

interface Action {
  label: string;
  icon: IconName;
  /** Opens the main window on a tool… */
  open?: OpenAction;
  /** …or runs right here and reports a result (null = cancelled)… */
  run?: () => Promise<string | null>;
  /** …or handles everything itself. */
  custom?: () => Promise<void>;
  ai?: boolean;
}

type State =
  | { kind: "idle" }
  | { kind: "ring"; slots: Record<Dir, Action | null>; hot: Dir | "centre" | null; orb: { x: number; y: number } }
  | { kind: "menu"; title: string; actions: Action[] }
  | { kind: "busy"; label: string; done?: number; total?: number }
  | { kind: "done"; ok: boolean; text: string };

const win = getCurrentWindow();
const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;
const stem = (p: string) => fileName(p).replace(/\.[^.]+$/, "");

// Ring segments (viewBox 240×240, centre 120,120; outer radius 110, inner 50).
const SEGMENTS: Record<Dir, { d: string; x: number; y: number }> = {
  n: { d: "M193.6 38.3 A110 110 0 0 0 46.4 38.3 L86.5 82.8 A50 50 0 0 1 153.5 82.8 Z", x: 120, y: 40 },
  w: { d: "M38.3 46.4 A110 110 0 0 0 38.3 193.6 L82.8 153.5 A50 50 0 0 1 82.8 86.5 Z", x: 40, y: 120 },
  s: { d: "M46.4 201.7 A110 110 0 0 0 193.6 201.7 L153.5 157.2 A50 50 0 0 1 86.5 157.2 Z", x: 120, y: 200 },
  e: { d: "M201.7 193.6 A110 110 0 0 0 201.7 46.4 L157.2 86.5 A50 50 0 0 1 157.2 153.5 Z", x: 200, y: 120 },
};

/** Which part of the ring a point (relative to the orb's centre) is over. */
function hitTest(dx: number, dy: number): Dir | "centre" | null {
  const r = Math.hypot(dx, dy);
  if (r < 46) return "centre";
  if (r > 125) return null;
  const angle = (Math.atan2(-dy, dx) * 180) / Math.PI; // 0 = right, 90 = up
  if (angle >= 45 && angle < 135) return "n";
  if (angle >= -45 && angle < 45) return "e";
  if (angle >= -135 && angle < -45) return "s";
  return "w";
}

export function Bubble() {
  const [state, setState] = useState<State>({ kind: "idle" });
  const [corner, setCorner] = useState({ right: false, below: false });
  const anchor = useRef<{ x: number; y: number } | null>(null);
  const press = useRef<{ x: number; y: number } | null>(null);
  const stateRef = useRef(state);
  stateRef.current = state;
  const pathsRef = useRef<string[]>([]);

  async function idlePosition() {
    const scale = await win.scaleFactor();
    return anchor.current ?? (await win.outerPosition()).toLogical(scale);
  }

  async function workArea() {
    const scale = await win.scaleFactor();
    const monitor = await currentMonitor();
    if (!monitor) return null;
    const a = monitor.workArea;
    return { x: a.position.x / scale, y: a.position.y / scale, w: a.size.width / scale, h: a.size.height / scale };
  }

  /** Opens the ring window around the orb, stepping inward if a screen edge is too close. */
  async function openRing(slots: Record<Dir, Action | null>) {
    const pos = await idlePosition();
    anchor.current = pos;
    const area = await workArea();
    const cx = pos.x + SIZE / 2;
    const cy = pos.y + SIZE / 2;
    let x = cx - RING / 2;
    let y = cy - RING / 2;
    if (area) {
      x = Math.min(Math.max(x, area.x), area.x + area.w - RING);
      y = Math.min(Math.max(y, area.y), area.y + area.h - RING);
    }
    setState({ kind: "ring", slots, hot: null, orb: { x: RING / 2, y: RING / 2 } });
    await win.setSize(new LogicalSize(RING, RING));
    await win.setPosition(new LogicalPosition(x, y));
  }

  /** Grows the window for a list menu, keeping the orb where it was on screen. */
  async function expand(height: number) {
    const pos = await idlePosition();
    anchor.current = pos;
    const area = await workArea();
    const right = area ? pos.x + SIZE / 2 > area.x + area.w / 2 : false;
    const below = area ? pos.y + SIZE / 2 > area.y + area.h / 2 : false;
    setCorner({ right, below });
    const width = MENU_WIDTH + SIZE;
    await win.setSize(new LogicalSize(width, height));
    await win.setPosition(new LogicalPosition(right ? pos.x + SIZE - width : pos.x, below ? pos.y + SIZE - height : pos.y));
    await win.setFocus(); // so clicking elsewhere closes the menu
  }

  async function collapse() {
    setState({ kind: "idle" });
    const pos = anchor.current;
    anchor.current = null;
    if (pos) await win.setPosition(new LogicalPosition(pos.x, pos.y));
    await win.setSize(new LogicalSize(SIZE, SIZE));
  }

  function showList(next: State, lines: number) {
    setState(next);
    void expand(Math.max(SIZE, 64 + lines * 44));
  }

  async function runQuick(action: Action) {
    try {
      const text = await action.run!();
      if (text == null) return collapse();
      setState({ kind: "done", ok: true, text });
    } catch (e) {
      setState({ kind: "done", ok: false, text: errorMessage(e) });
    }
  }

  async function choose(action: Action) {
    if (action.custom) return action.custom();
    if (action.open) {
      await invoke("open_in_main", { request: { action: action.open, paths: pathsRef.current } });
      return collapse();
    }
    await runQuick(action);
  }

  /** The actions for these files: four ring slots plus the full list. */
  function actionsFor(paths: string[]): { slots: Record<Dir, Action | null>; list: Action[]; title: string } {
    const pdfs = paths.filter((p) => /\.pdf$/i.test(p));
    const images = paths.filter(isImage);
    const to = (target: "docx" | "searchable", ext: string, suffix: string, label: string) => async () => {
      const src = pdfs[0];
      setState({ kind: "busy", label: "Choose where to save" });
      let output = await save({ defaultPath: await join(await dirname(src), `${stem(src)}${suffix}.${ext}`), filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
      if (!output) return null;
      if (!output.toLowerCase().endsWith(`.${ext}`)) output = `${output}.${ext}`;
      setState({ kind: "busy", label });
      const result = await convertPdf({ path: src, target, output, pages: null, useOcr: true, overwrite: true }, (done, total) => setState({ kind: "busy", label, done, total }));
      if (target === "searchable" && result.ocrPages.length === 0) return "Already searchable: every page has text.";
      return `Saved ${fileName(output)}`;
    };

    const redact: Action = { label: "Redact", icon: "redact", open: "redact", ai: true };
    const split: Action = { label: "Split", icon: "split", open: "split" };
    const edit: Action = { label: "Edit", icon: "edit", open: "edit" };
    const word: Action = { label: "To Word", icon: "convert", run: to("docx", "docx", "", "Converting to Word") };
    const searchable: Action = { label: "Make searchable", icon: "convert", run: to("searchable", "pdf", "_searchable", "Reading scanned pages") };
    const more: Action = { label: "More conversions…", icon: "convert", open: "convert" };

    if (pdfs.length === 1) {
      return { title: fileName(pdfs[0]), slots: { n: redact, w: split, e: word, s: edit }, list: [redact, split, edit, word, searchable, more] };
    }
    if (pdfs.length > 1) {
      const merge: Action = { label: `Merge ${pdfs.length}`, icon: "merge", open: "merge" };
      return { title: `${pdfs.length} PDFs`, slots: { n: null, w: merge, e: null, s: null }, list: [merge, { ...split, label: "Split or reorder" }] };
    }
    if (images.length > 0) {
      const create: Action = {
        label: "Create PDF",
        icon: "merge",
        run: async () => {
          setState({ kind: "busy", label: "Choose where to save" });
          let output = await save({ defaultPath: await join(await dirname(images[0]), `${stem(images[0])}.pdf`), filters: [{ name: "PDF", extensions: ["pdf"] }] });
          if (!output) return null;
          if (!/\.pdf$/i.test(output)) output = `${output}.pdf`;
          await imagesToPdf(images, output, "a4", (done, total) => setState({ kind: "busy", label: "Adding images", done, total }));
          return `Saved ${fileName(output)}`;
        },
      };
      const title = `${images.length} image${images.length === 1 ? "" : "s"}`;
      return { title, slots: { n: null, w: null, e: create, s: null }, list: [create, { label: "Open in Convert…", icon: "convert", open: "convert" }] };
    }
    return { title: "", slots: { n: null, w: null, e: null, s: null }, list: [] };
  }

  function showFullList(paths: string[]) {
    const { title, list } = actionsFor(paths);
    if (list.length === 0) return showList({ kind: "done", ok: false, text: "PaperKit works with PDFs and images (JPG, PNG, TIFF…)." }, 1);
    showList({ kind: "menu", title, actions: list }, list.length);
  }

  useEffect(() => {
    const toOrb = async (p: { x: number; y: number }) => {
      const s = stateRef.current;
      if (s.kind !== "ring") return null;
      const scale = await win.scaleFactor();
      return { dx: p.x / scale - s.orb.x, dy: p.y / scale - s.orb.y };
    };
    const unDrop = getCurrentWebview().onDragDropEvent(async (e) => {
      const p = e.payload;
      if (p.type === "enter") {
        if (stateRef.current.kind !== "idle") return;
        pathsRef.current = p.paths;
        await openRing(actionsFor(p.paths).slots);
      } else if (p.type === "over") {
        const rel = await toOrb(p.position);
        const s = stateRef.current;
        if (rel && s.kind === "ring") {
          const hot = hitTest(rel.dx, rel.dy);
          if (hot !== s.hot) setState({ ...s, hot });
        }
      } else if (p.type === "drop") {
        const s = stateRef.current;
        pathsRef.current = p.paths;
        const rel = await toOrb(p.position);
        const hot = rel ? hitTest(rel.dx, rel.dy) : "centre";
        const action = s.kind === "ring" && hot && hot !== "centre" ? s.slots[hot] : null;
        if (action) {
          if (action.open) return void choose(action);
          // Quick job: back to the small orb with a status panel beside it.
          await collapse();
          showList({ kind: "busy", label: action.label }, 1);
          return void runQuick(action);
        }
        // Centre (or an empty slot): the full list next to the orb.
        await collapse();
        showFullList(p.paths);
      } else if (stateRef.current.kind === "ring") {
        void collapse(); // dragged away again
      }
    });
    // Remember where the user puts the bubble (only while it is collapsed).
    let timer: number | undefined;
    const unMove = win.onMoved(({ payload }) => {
      if (anchor.current) return;
      window.clearTimeout(timer);
      timer = window.setTimeout(async () => {
        const p = payload.toLogical(await win.scaleFactor());
        void invoke("bubble_moved", { x: p.x, y: p.y });
      }, 400);
    });
    // Clicking elsewhere closes a menu or a result (not while a task or dialog runs).
    const unFocus = win.onFocusChanged(({ payload: focused }) => {
      const s = stateRef.current.kind;
      if (!focused && (s === "menu" || s === "done")) void collapse();
    });
    return () => {
      void unDrop.then((f) => f());
      void unMove.then((f) => f());
      void unFocus.then((f) => f());
    };
    // Subscriptions are set up once; handlers read current state through refs.
  }, []);

  useEffect(() => {
    if (state.kind !== "done") return;
    const t = window.setTimeout(() => void collapse(), state.ok ? 3500 : 6000);
    return () => window.clearTimeout(t);
  }, [state]);

  // Pressing and moving drags the window; a plain click opens the small menu.
  function onPointerDown(e: React.PointerEvent) {
    press.current = { x: e.screenX, y: e.screenY };
  }
  function onPointerMove(e: React.PointerEvent) {
    const p = press.current;
    if (p && state.kind === "idle" && Math.hypot(e.screenX - p.x, e.screenY - p.y) > 4) {
      press.current = null;
      void win.startDragging();
    }
  }
  function onClick() {
    if (!press.current) return; // it was a drag
    press.current = null;
    if (state.kind !== "idle") return void collapse();
    const actions: Action[] = [
      {
        label: "Open PaperKit",
        icon: "home",
        custom: async () => {
          await collapse();
          await invoke("show_main_window");
        },
      },
      {
        label: "Choose files…",
        icon: "upload",
        custom: async () => {
          setState({ kind: "busy", label: "Choose files" });
          const picked = await open({ multiple: true, filters: [{ name: "PDFs and images", extensions: ["pdf", "jpg", "jpeg", "png", "tif", "tiff", "bmp", "gif"] }] });
          const list = picked ? (Array.isArray(picked) ? picked : [picked]) : [];
          if (list.length === 0) return collapse();
          pathsRef.current = list;
          await collapse();
          showFullList(list);
        },
      },
      {
        label: "Hide the bubble",
        icon: "close",
        custom: async () => {
          await collapse();
          await invoke("bubble_hide");
        },
      },
    ];
    showList({ kind: "menu", title: "PaperKit", actions }, actions.length);
  }

  // ---- Rendering ----

  if (state.kind === "ring") {
    const hot = state.hot;
    const hotAction = hot && hot !== "centre" ? state.slots[hot] : null;
    return (
      <div className="bb ring">
        <div className="bb-ring" style={{ left: state.orb.x - 120, top: state.orb.y - 120 }}>
          <div className="bb-ring-glass" />
          <svg width="240" height="240" viewBox="0 0 240 240" aria-hidden="true">
            {(Object.keys(SEGMENTS) as Dir[]).map((d) => (
              <path key={d} d={SEGMENTS[d].d} className={`bb-seg${state.slots[d] ? "" : " empty"}${hot === d && state.slots[d] ? " hot" : ""}`} />
            ))}
          </svg>
          {(Object.keys(SEGMENTS) as Dir[]).map((d) => {
            const a = state.slots[d];
            return (
              <span key={d} className={`bb-seg-icon${a ? "" : " empty"}${hot === d && a ? " hot" : ""}`} style={{ left: SEGMENTS[d].x - 11, top: SEGMENTS[d].y - 11 }}>
                {a ? <Icon name={a.icon} size={22} /> : <span className="bb-dash" />}
              </span>
            );
          })}
          <div className={`bb-orb bb-orb-centre${hotAction ? " hot" : ""}`}>
            {hotAction ? (
              <span className="bb-orb-text">
                <strong>{hotAction.label}</strong>
                <span>release to start</span>
              </span>
            ) : hot === "centre" ? (
              <span className="bb-orb-text">
                <strong>All actions</strong>
                <span>release here</span>
              </span>
            ) : (
              <Mark size={28} />
            )}
          </div>
        </div>
      </div>
    );
  }

  const opened = state.kind !== "idle";
  return (
    <div className={`bb${opened ? " open" : ""}${corner.right ? " right" : ""}${corner.below ? " below" : ""}`}>
      <button
        type="button"
        className={`bb-orb bb-ball${state.kind === "busy" ? " working" : ""}`}
        aria-label="PaperKit: drag a file here, drag to move, click for options"
        title="Drag a PDF or image here · drag to move · click for options"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onClick={onClick}
      >
        {state.kind === "busy" && state.total ? (
          <svg className="bb-progress" width="64" height="64" viewBox="0 0 64 64" aria-hidden="true">
            <circle cx="32" cy="32" r="29" className="bb-progress-track" />
            <circle cx="32" cy="32" r="29" className="bb-progress-bar" strokeDasharray={`${(182 * (state.done ?? 0)) / state.total} 182`} transform="rotate(-90 32 32)" />
          </svg>
        ) : null}
        <Mark size={28} />
      </button>

      {opened && (
        <div className="bb-panel" role="dialog" aria-label="PaperKit">
          {state.kind === "menu" && (
            <>
              <div className="bb-title" title={state.title}>
                {state.title}
              </div>
              {state.actions.map((a) => (
                <button key={a.label} type="button" className="bb-action" onClick={() => void choose(a)}>
                  <Icon name={a.icon} size={16} />
                  <span>{a.label}</span>
                  {a.ai && <span className="pill">AI</span>}
                  {a.open && <span className="bb-open">↗</span>}
                </button>
              ))}
            </>
          )}
          {state.kind === "busy" && (
            <div className="bb-status">
              <span>
                {state.label}
                {state.total ? ` · page ${state.done ?? 0} of ${state.total}` : "…"}
              </span>
              {!state.total && <span className="bb-spinner" aria-hidden="true" />}
            </div>
          )}
          {state.kind === "done" && (
            <div className={`bb-status ${state.ok ? "ok" : "err"}`} role="status">
              {state.text}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

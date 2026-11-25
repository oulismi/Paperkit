import { useEffect, useRef, useState } from "react";
import { pageImage } from "../../lib/pdf";
import type { Area, EditOp, PageSize } from "../../lib/tauri";
import { useInView } from "../../lib/useInView";

export type Tool = "select" | "text" | "image" | "whiteout" | "highlight";

export interface Item {
  id: string;
  op: EditOp;
  /** Object URL of the image, for image items. */
  preview?: string;
}

interface Props {
  path: string;
  size: PageSize;
  items: Item[];
  tool: Tool;
  selectedId: string | null;
  /** Width ÷ height of the image waiting to be placed, if any. */
  imageAspect: number | null;
  onCreateText: (page: number, x: number, y: number) => void;
  onCreateBox: (area: Area) => void;
  onSelect: (id: string | null) => void;
  onMove: (id: string, dx: number, dy: number) => void;
  onText: (id: string, text: string) => void;
  onTextDone: (id: string) => void;
}

const RENDER_WIDTH = 1400;

export const opPage = (op: EditOp) => (op.kind === "text" ? op.page : op.area.page);

export function EditPage(props: Props) {
  const { path, size, items, tool, selectedId, imageAspect } = props;
  const [viewRef, inView] = useInView<HTMLDivElement>("800px");
  const [src, setSrc] = useState<string>();
  const [scale, setScale] = useState(1); // css px per PDF point
  const [drag, setDrag] = useState<{ x: number; y: number; w: number; h: number } | null>(null);
  const start = useRef<{ x: number; y: number } | null>(null);
  const moving = useRef<{ id: string; x: number; y: number } | null>(null);

  useEffect(() => {
    if (!inView) return;
    let live = true;
    pageImage(path, size.page, RENDER_WIDTH).then((u) => live && setSrc(u), () => {});
    return () => {
      live = false;
    };
  }, [inView, path, size.page]);

  useEffect(() => {
    const el = viewRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setScale(el.clientWidth / size.width));
    ro.observe(el);
    return () => ro.disconnect();
  }, [viewRef, size.width]);

  // Pointer position in PDF points (page space).
  const point = (e: React.PointerEvent) => {
    const r = viewRef.current!.getBoundingClientRect();
    return {
      x: Math.min(size.width, Math.max(0, ((e.clientX - r.left) / r.width) * size.width)),
      y: Math.min(size.height, Math.max(0, ((e.clientY - r.top) / r.height) * size.height)),
    };
  };

  function onPointerDown(e: React.PointerEvent<HTMLDivElement>) {
    if (e.target !== e.currentTarget && !(e.target as HTMLElement).classList.contains("ed-pageimg")) return;
    const p = point(e);
    if (tool === "select") return props.onSelect(null);
    if (tool === "text") {
      e.preventDefault(); // keep focus for the new text box
      return props.onCreateText(size.page, p.x, p.y);
    }
    if (tool === "image" && imageAspect == null) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    start.current = p;
    setDrag({ ...p, w: 0, h: 0 });
  }

  function onPointerMove(e: React.PointerEvent<HTMLDivElement>) {
    if (moving.current) {
      const p = point(e);
      props.onMove(moving.current.id, p.x - moving.current.x, p.y - moving.current.y);
      moving.current = { ...moving.current, ...p };
      return;
    }
    if (!start.current) return;
    const p = point(e);
    const s = start.current;
    let w = Math.abs(p.x - s.x);
    let h = Math.abs(p.y - s.y);
    if (tool === "image" && imageAspect) h = w / imageAspect; // keep the image's proportions
    setDrag({ x: p.x < s.x ? s.x - w : s.x, y: p.y < s.y && tool !== "image" ? s.y - h : s.y, w, h });
  }

  function onPointerUp() {
    moving.current = null;
    if (drag && drag.w > 3 && drag.h > 3) {
      props.onCreateBox({ page: size.page, x0: drag.x, y0: drag.y, x1: drag.x + drag.w, y1: drag.y + drag.h });
    }
    start.current = null;
    setDrag(null);
  }

  function startMove(e: React.PointerEvent, id: string) {
    e.stopPropagation();
    props.onSelect(id);
    if ((e.target as HTMLElement).tagName === "TEXTAREA") return; // let the caret move inside text
    viewRef.current!.setPointerCapture(e.pointerId);
    moving.current = { id, ...point(e) };
  }

  const pct = (v: number, of: number) => `${(v / of) * 100}%`;
  const box = (a: Area) => ({ left: pct(a.x0, size.width), top: pct(a.y0, size.height), width: pct(a.x1 - a.x0, size.width), height: pct(a.y1 - a.y0, size.height) });

  return (
    <div
      ref={viewRef}
      className={`rd-page ed-page tool-${tool}`}
      style={{ aspectRatio: `${size.width} / ${size.height}` }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      data-page={size.page}
    >
      {src ? <img className="ed-pageimg" src={src} alt={`Page ${size.page}`} draggable={false} /> : <span className="rd-loading">Page {size.page}</span>}
      {items.map((item) => {
        const selected = item.id === selectedId;
        const cls = `ed-item ed-${item.op.kind}${selected ? " selected" : ""}`;
        const op = item.op;
        if (op.kind === "text") {
          const lines = op.text.split("\n");
          return (
            <div key={item.id} className={cls} style={{ left: pct(op.x, size.width), top: pct(op.y, size.height) }} onPointerDown={(e) => startMove(e, item.id)}>
              <textarea
                aria-label="Added text"
                autoFocus={selected && op.text === ""}
                value={op.text}
                rows={lines.length}
                cols={Math.max(4, ...lines.map((l) => l.length + 1))}
                spellCheck={false}
                style={{ fontSize: op.size * scale, color: `rgb(${op.color.map((c) => c * 255).join(",")})` }}
                onChange={(e) => props.onText(item.id, e.target.value)}
                onBlur={() => props.onTextDone(item.id)}
              />
            </div>
          );
        }
        return (
          <div key={item.id} className={cls} style={box(op.area)} onPointerDown={(e) => startMove(e, item.id)} role="img" aria-label={op.kind}>
            {op.kind === "image" && item.preview && <img src={item.preview} alt="" draggable={false} />}
          </div>
        );
      })}
      {drag && <span className="rd-drag" style={box({ page: size.page, x0: drag.x, y0: drag.y, x1: drag.x + drag.w, y1: drag.y + drag.h })} />}
    </div>
  );
}

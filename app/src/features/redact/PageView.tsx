import { useEffect, useRef, useState } from "react";
import { pageImage } from "../../lib/pdf";
import type { Area, PageSize } from "../../lib/tauri";
import { useInView } from "../../lib/useInView";
import type { Item } from "./items";

const RENDER_WIDTH = 1400; // px; sharp at the ~700 css px the page is shown at, on 2x screens

interface Props {
  path: string;
  size: PageSize;
  items: Item[];
  focusedId: string | null;
  drawing: boolean;
  onToggle: (itemId: string) => void;
  onDraw: (area: Area) => void;
}

interface Drag {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function PageView({ path, size, items, focusedId, drawing, onToggle, onDraw }: Props) {
  const [viewRef, inView] = useInView<HTMLDivElement>("800px");
  const [src, setSrc] = useState<string>();
  const [drag, setDrag] = useState<Drag | null>(null);
  const start = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    if (!inView) return;
    let live = true;
    pageImage(path, size.page, RENDER_WIDTH).then((url) => live && setSrc(url), () => {});
    return () => {
      live = false;
    };
  }, [inView, path, size.page]);

  // Pointer position as a fraction of the page (0–1).
  const frac = (e: React.PointerEvent<HTMLDivElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    return { x: Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)), y: Math.min(1, Math.max(0, (e.clientY - r.top) / r.height)) };
  };

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!drawing) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    start.current = frac(e);
    setDrag({ ...start.current, w: 0, h: 0 });
  };
  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!start.current) return;
    const p = frac(e);
    const s = start.current;
    setDrag({ x: Math.min(s.x, p.x), y: Math.min(s.y, p.y), w: Math.abs(p.x - s.x), h: Math.abs(p.y - s.y) });
  };
  const onPointerUp = () => {
    if (drag && drag.w * size.width > 3 && drag.h * size.height > 3) {
      onDraw({
        page: size.page,
        x0: drag.x * size.width,
        y0: drag.y * size.height,
        x1: (drag.x + drag.w) * size.width,
        y1: (drag.y + drag.h) * size.height,
      });
    }
    start.current = null;
    setDrag(null);
  };

  const pct = (v: number, of: number) => `${(v / of) * 100}%`;

  return (
    <div
      ref={viewRef}
      className={`rd-page${drawing ? " drawing" : ""}`}
      style={{ aspectRatio: `${size.width} / ${size.height}` }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      data-page={size.page}
    >
      {src ? <img src={src} alt={`Page ${size.page}`} draggable={false} /> : <span className="rd-loading">Page {size.page}</span>}
      {items.flatMap((item) =>
        item.areas
          .filter((a) => a.page === size.page)
          .map((a, i) => (
            <button
              key={`${item.id}-${i}`}
              type="button"
              className={`rd-box ${item.status}${item.id === focusedId ? " focused" : ""}`}
              style={{
                left: pct(a.x0, size.width),
                top: pct(a.y0, size.height),
                width: pct(a.x1 - a.x0, size.width),
                height: pct(a.y1 - a.y0, size.height),
              }}
              aria-label={`${item.label}: ${item.text} — ${item.status === "approved" ? "will be redacted, click to undo" : "click to approve"}`}
              title={`${item.label} · ${item.status === "approved" ? "approved" : "click to approve"}`}
              tabIndex={-1}
              onClick={(e) => {
                e.stopPropagation();
                if (!drawing) onToggle(item.id);
              }}
            />
          )),
      )}
      {drag && (
        <span className="rd-drag" style={{ left: `${drag.x * 100}%`, top: `${drag.y * 100}%`, width: `${drag.w * 100}%`, height: `${drag.h * 100}%` }} />
      )}
    </div>
  );
}

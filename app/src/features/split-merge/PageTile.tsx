import { useEffect, useState } from "react";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Icon } from "../../components/Icon";
import { intrinsicRotation, thumbnail } from "../../lib/pdf";
import { useInView } from "../../lib/useInView";
import type { PageItem, SourceFile } from "../../app/model";

interface Props {
  item: PageItem;
  file: SourceFile;
  position: number;
  selected: boolean;
  note?: string;
  onSelect: (e: React.MouseEvent) => void;
  onRotate: (delta: number) => void;
  onRemove: () => void;
  onMove: (delta: number) => void;
}

export function PageTile({ item, file, position, selected, note, onSelect, onRotate, onRemove, onMove }: Props) {
  const { setNodeRef, attributes, listeners, transform, transition, isDragging } = useSortable({ id: item.key });
  const [viewRef, inView] = useInView<HTMLDivElement>();
  const [src, setSrc] = useState<string>();
  const [baseRotation, setBaseRotation] = useState(0);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    if (!inView) return;
    let live = true;
    Promise.all([thumbnail(file.path, item.page), intrinsicRotation(file.path, item.page)])
      .then(([url, rot]) => live && (setSrc(url), setBaseRotation(rot)))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [inView, file.path, item.page]);

  const label = `Page ${position}: ${file.name}, page ${item.page}`;
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.altKey && e.key === "ArrowLeft") onMove(-1);
    else if (e.altKey && e.key === "ArrowRight") onMove(1);
    else if (e.key === "Delete") onRemove();
    else if (e.key === "r" || e.key === "R") onRotate(e.shiftKey ? -90 : 90);
    else return;
    e.preventDefault();
  };

  return (
    <div
      ref={setNodeRef}
      className={`tile${isDragging ? " dragging" : ""}`}
      style={{ transform: CSS.Translate.toString(transform), transition }}
    >
      <div ref={viewRef} className="page" aria-pressed={selected}>
        <button
          type="button"
          className="page-select"
          aria-label={label}
          aria-pressed={selected}
          aria-roledescription={attributes["aria-roledescription"]}
          title="Click to select · drag to reorder · R rotates · Delete removes · Alt+arrows move"
          onPointerDown={listeners?.onPointerDown as React.PointerEventHandler | undefined}
          onClick={onSelect}
          onKeyDown={onKeyDown}
        />
        {src ? (
          <img src={src} alt="" draggable={false} style={{ transform: `rotate(${baseRotation + item.rotate}deg)` }} />
        ) : (
          <span className="placeholder" aria-hidden="true">
            {failed && <span className="error">!</span>}
          </span>
        )}
        <span className="actions">
          <button type="button" aria-label={`Rotate page ${position} left`} onClick={() => onRotate(-90)}>
            <Icon name="rotateLeft" size={15} />
          </button>
          <button type="button" aria-label={`Rotate page ${position} right`} onClick={() => onRotate(90)}>
            <Icon name="rotateRight" size={15} />
          </button>
          <button type="button" className="danger" aria-label={`Remove page ${position}`} onClick={onRemove}>
            <Icon name="trash" size={15} />
          </button>
        </span>
        {selected && (
          <span className="check">
            <Icon name="check" size={12} strokeWidth={3} />
          </span>
        )}
        <span className="strip" style={{ background: file.color.strip }} />
      </div>
      <span className="tile-label">
        <span className="pos">{position}</span> · {file.name.replace(/\.pdf$/i, "")} p.{item.page}
        {note && (
          <>
            <br />
            <span className="cut-marker">{note}</span>
          </>
        )}
      </span>
    </div>
  );
}

import { useMemo, useRef, useState } from "react";
import { DndContext, PointerSensor, closestCenter, useSensor, useSensors, type DragEndEvent } from "@dnd-kit/core";
import { SortableContext, arrayMove, rectSortingStrategy } from "@dnd-kit/sortable";
import { Icon } from "../../components/Icon";
import type { Mode, PageItem, SourceFile } from "../../app/model";
import { PageTile } from "./PageTile";
import { SettingsPanel } from "./SettingsPanel";
import { planSplit, type SplitRule } from "./splitPlan";

interface Props {
  mode: Mode;
  onMode: (mode: Mode) => void;
  files: SourceFile[];
  pages: PageItem[];
  onPages: (update: (pages: PageItem[]) => PageItem[]) => void;
  onAddFiles: () => void;
  onRemoveFile: (fileId: string) => void;
  onMoveFile: (fileId: string, delta: number) => void;
  onCloseAll: () => void;
}

const TITLES: Record<Mode, string> = { split: "Split", merge: "Merge", extract: "Extract pages" };

export function Organizer({ mode, onMode, files, pages, onPages, onAddFiles, onRemoveFile, onMoveFile, onCloseAll }: Props) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [splitRule, setSplitRule] = useState<SplitRule>({ kind: "every", n: 1 });
  const anchor = useRef<string | null>(null);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));
  const fileById = useMemo(() => new Map(files.map((f) => [f.id, f])), [files]);

  // Selection limited to pages that still exist.
  const live = useMemo(() => new Set(pages.filter((p) => selected.has(p.key)).map((p) => p.key)), [pages, selected]);

  const plan = useMemo(() => {
    if (mode !== "split") return null;
    const rule: SplitRule =
      splitRule.kind === "selected"
        ? { kind: "selected", starts: pages.flatMap((p, i) => (live.has(p.key) ? [i + 1] : [])) }
        : splitRule;
    return planSplit(rule, pages.length);
  }, [mode, splitRule, pages, live]);

  // Under each tile: which output file the page goes to (split mode only).
  const notes = useMemo(() => {
    const out = new Map<string, string>();
    if (!plan?.ok) return out;
    plan.groups.forEach((group, i) => {
      group.forEach((pos, j) => {
        const key = pages[pos - 1].key;
        const tag = j === 0 ? `▸ File ${i + 1}` : `File ${i + 1}`;
        out.set(key, out.has(key) ? `${out.get(key)}, ${i + 1}` : tag);
      });
    });
    return out;
  }, [plan, pages]);

  function select(key: string, e: React.MouseEvent) {
    const next = new Set(live);
    const a = anchor.current ? pages.findIndex((p) => p.key === anchor.current) : -1;
    const b = pages.findIndex((p) => p.key === key);
    if (e.shiftKey && a >= 0 && b >= 0) {
      for (let i = Math.min(a, b); i <= Math.max(a, b); i++) next.add(pages[i].key);
    } else {
      if (next.has(key)) next.delete(key);
      else next.add(key);
      anchor.current = key;
    }
    setSelected(next);
  }

  const rotate = (keys: Set<string>, delta: number) =>
    onPages((ps) => ps.map((p) => (keys.has(p.key) ? { ...p, rotate: (p.rotate + delta + 360) % 360 } : p)));
  const remove = (keys: Set<string>) => onPages((ps) => ps.filter((p) => !keys.has(p.key)));
  const move = (key: string, delta: number) =>
    onPages((ps) => {
      const i = ps.findIndex((p) => p.key === key);
      const j = i + delta;
      return j < 0 || j >= ps.length ? ps : arrayMove(ps, i, j);
    });

  function onDragEnd({ active, over }: DragEndEvent) {
    if (!over || active.id === over.id) return;
    onPages((ps) => arrayMove(ps, ps.findIndex((p) => p.key === active.id), ps.findIndex((p) => p.key === over.id)));
  }

  const pageCountBy = (fileId: string) => pages.filter((p) => p.fileId === fileId).length;
  const title = mode === "merge" && files.length > 1 ? `Merge ${files.length} files` : TITLES[mode];

  return (
    <div className="organizer">
      <div className="workspace">
        <div className="toolbar">
          <h1>{title}</h1>
          {files.map((f) => (
            <span key={f.id} className="chip" style={{ background: f.color.bg, color: f.color.text }}>
              <span className="dot" style={{ background: f.color.strip }} />
              {f.name} · {pageCountBy(f.id)}
              <button type="button" aria-label={`Close ${f.name}`} onClick={() => onRemoveFile(f.id)}>
                <Icon name="close" size={12} strokeWidth={2} />
              </button>
            </span>
          ))}
          <button type="button" className="btn-ghost" onClick={onAddFiles}>
            + Add files
          </button>
          <button type="button" className="btn-ghost solid" style={{ marginLeft: "auto" }} onClick={onCloseAll}>
            Close all
          </button>
        </div>

        {live.size > 0 ? (
          <div className="toolbar" role="toolbar" aria-label="Selected pages">
            <span style={{ fontSize: 13, fontWeight: 600 }}>{live.size} selected</span>
            <button type="button" className="btn-ghost solid" onClick={() => rotate(live, -90)}>
              Rotate left
            </button>
            <button type="button" className="btn-ghost solid" onClick={() => rotate(live, 90)}>
              Rotate right
            </button>
            <button type="button" className="btn-ghost solid" onClick={() => remove(live)}>
              Remove
            </button>
            <button type="button" className="btn-ghost solid" onClick={() => setSelected(new Set())}>
              Clear selection
            </button>
          </div>
        ) : (
          <p className="muted" style={{ fontSize: 13 }}>
            Drag pages to reorder · click to select, Shift-click for a range · hover for rotate and remove · drop more PDFs
            anywhere
          </p>
        )}

        {pages.length === 0 ? (
          <p className="muted">All pages were removed. Add files, or close all to start over.</p>
        ) : (
          <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={onDragEnd}>
            <SortableContext items={pages.map((p) => p.key)} strategy={rectSortingStrategy}>
              <div className="grid">
                {pages.map((p, i) => (
                  <PageTile
                    key={p.key}
                    item={p}
                    file={fileById.get(p.fileId)!}
                    position={i + 1}
                    selected={live.has(p.key)}
                    note={notes.get(p.key)}
                    onSelect={(e) => select(p.key, e)}
                    onRotate={(d) => rotate(new Set([p.key]), d)}
                    onRemove={() => remove(new Set([p.key]))}
                    onMove={(d) => move(p.key, d)}
                  />
                ))}
              </div>
            </SortableContext>
          </DndContext>
        )}
      </div>

      <SettingsPanel
        mode={mode}
        onMode={onMode}
        files={files}
        pages={pages}
        selected={live}
        onMoveFile={onMoveFile}
        plan={plan}
        splitRule={splitRule}
        onSplitRule={setSplitRule}
      />
    </div>
  );
}

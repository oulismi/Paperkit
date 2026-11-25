import { useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { dirname, join } from "@tauri-apps/api/path";
import { Icon } from "../../components/Icon";
import { editApply, editFont, editInfo, errorMessage, IMAGE_EXTENSIONS, readImage, type Area, type FormField, type PageSize } from "../../lib/tauri";
import { newId } from "../../app/model";
import { EditPage, opPage, type Item, type Tool } from "./EditPage";

interface Props {
  path: string;
  name: string;
  onChangeFile: () => void;
  /** Demo mode: changes already placed. */
  initialItems?: Item[];
}

type Status = { kind: "ok" | "err"; text: string } | null;
type Rgb = [number, number, number];

const COLORS: { name: string; rgb: Rgb }[] = [
  { name: "Black", rgb: [0, 0, 0] },
  { name: "Blue", rgb: [0.12, 0.37, 0.82] },
  { name: "Red", rgb: [0.71, 0.14, 0.09] },
];
const HIGHLIGHT: Rgb = [1, 0.85, 0.2];

const TOOLS: { id: Tool; label: string; hint: string }[] = [
  { id: "select", label: "Select", hint: "Click an item to select it, drag to move it" },
  { id: "text", label: "Text", hint: "Click on the page where the text should start" },
  { id: "image", label: "Image / signature", hint: "Drag a box on the page to place the image" },
  { id: "whiteout", label: "Whiteout", hint: "Drag a box to cover content in white (it hides it; use Redact to remove it)" },
  { id: "highlight", label: "Highlight", hint: "Drag a box over text to highlight it" },
];

const describe = (item: Item) => {
  const op = item.op;
  switch (op.kind) {
    case "text":
      return { title: op.text.split("\n")[0] || "(empty text)", meta: `Text · p.${op.page}` };
    case "image":
      return { title: op.path.split(/[\\/]/).pop() ?? "Image", meta: `Image · p.${op.area.page}` };
    case "whiteout":
      return { title: "Whiteout box", meta: `Hides content · p.${op.area.page}` };
    case "highlight":
      return { title: "Highlight", meta: `p.${op.area.page}` };
  }
};

export function EditScreen({ path, name, onChangeFile, initialItems }: Props) {
  const [pages, setPages] = useState<PageSize[]>([]);
  const [fields, setFields] = useState<FormField[]>([]);
  const [values, setValues] = useState<Record<string, string>>({});
  const [items, setItems] = useState<Item[]>(initialItems ?? []);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tool, setTool] = useState<Tool>("text");
  const [textSize, setTextSize] = useState(12);
  const [textColor, setTextColor] = useState<Rgb>([0, 0, 0]);
  const [image, setImage] = useState<{ path: string; url: string; aspect: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<Status>(null);
  const [loadError, setLoadError] = useState("");

  useEffect(() => {
    editInfo(path).then(
      (info) => {
        setPages(info.pages);
        setFields(info.fields);
        setValues(Object.fromEntries(info.fields.map((f) => [f.name, f.value])));
      },
      (e) => setLoadError(errorMessage(e)),
    );
  }, [path]);

  // Preview added text in the exact font the PDF will use.
  useEffect(() => {
    if (document.fonts.check("12px PaperKitSans")) return;
    editFont().then((bytes) => {
      const face = new FontFace("PaperKitSans", bytes);
      face.load().then((f) => document.fonts.add(f), () => {});
    }, () => {});
  }, []);

  // Delete removes the selected item (unless typing in a field).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
      if (e.key === "Delete" && selectedId && !typing) setItems((list) => list.filter((i) => i.id !== selectedId));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selectedId]);

  const changedFields = useMemo(() => Object.fromEntries(Object.entries(values).filter(([k, v]) => fields.find((f) => f.name === k)?.value !== v)), [values, fields]);
  const changes = items.filter((i) => i.op.kind !== "text" || i.op.text.trim() !== "").length + Object.keys(changedFields).length;

  const add = (item: Item) => {
    setItems((list) => [...list, item]);
    setSelectedId(item.id);
  };
  const update = (id: string, f: (item: Item) => Item) => setItems((list) => list.map((i) => (i.id === id ? f(i) : i)));

  async function chooseImage() {
    const picked = await open({ multiple: false, filters: [{ name: "Images", extensions: IMAGE_EXTENSIONS }] });
    if (!picked || Array.isArray(picked)) return;
    try {
      const url = URL.createObjectURL(new Blob([await readImage(picked)]));
      const img = new Image();
      img.src = url;
      await img.decode();
      setImage({ path: picked, url, aspect: img.naturalWidth / img.naturalHeight });
      setTool("image");
    } catch (e) {
      setStatus({ kind: "err", text: `Could not load that image: ${errorMessage(e)}` });
    }
  }

  function onCreateBox(area: Area) {
    if (tool === "whiteout") add({ id: newId(), op: { kind: "whiteout", area } });
    else if (tool === "highlight") add({ id: newId(), op: { kind: "highlight", area, color: HIGHLIGHT } });
    else if (tool === "image" && image) add({ id: newId(), op: { kind: "image", area, path: image.path }, preview: image.url });
  }

  function onMove(id: string, dx: number, dy: number) {
    update(id, (item) => {
      const op = item.op;
      if (op.kind === "text") return { ...item, op: { ...op, x: op.x + dx, y: op.y + dy } };
      const a = op.area;
      return { ...item, op: { ...op, area: { ...a, x0: a.x0 + dx, x1: a.x1 + dx, y0: a.y0 + dy, y1: a.y1 + dy } } };
    });
  }

  async function apply() {
    setBusy(true);
    setStatus(null);
    try {
      const suggested = `${name.replace(/\.pdf$/i, "")}_edited.pdf`;
      let output = await save({ defaultPath: await join(await dirname(path), suggested), filters: [{ name: "PDF", extensions: ["pdf"] }] });
      if (!output) return;
      if (!/\.pdf$/i.test(output)) output = `${output}.pdf`;
      const ops = items.map((i) => i.op).filter((op) => op.kind !== "text" || op.text.trim() !== "");
      await editApply(path, output, ops, changedFields);
      setStatus({ kind: "ok", text: `Saved ${output.split(/[\\/]/).pop()} with ${changes} change${changes === 1 ? "" : "s"}.` });
    } catch (e) {
      setStatus({ kind: "err", text: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  }

  const selected = items.find((i) => i.id === selectedId);
  const hint = TOOLS.find((t) => t.id === tool)?.hint;

  return (
    <div className="rd">
      <header className="rd-header">
        <div className="rd-file">
          <span className="rd-name">{name}</span>
          <span className="muted">{hint}</span>
        </div>
        <div className="rd-actions" role="toolbar" aria-label="Edit tools">
          {TOOLS.map((t) => (
            <button
              key={t.id}
              type="button"
              className="rd-tool"
              aria-pressed={tool === t.id}
              onClick={() => (t.id === "image" && !image ? void chooseImage() : setTool(t.id))}
            >
              {t.label}
            </button>
          ))}
          <button type="button" className="rd-tool" onClick={onChangeFile}>
            Choose another file
          </button>
        </div>
      </header>

      <div className="rd-body">
        <div className="rd-doc">
          {loadError && <p className="status err">{loadError}</p>}
          {pages.map((p) => (
            <EditPage
              key={p.page}
              path={path}
              size={p}
              items={items.filter((i) => opPage(i.op) === p.page)}
              tool={tool}
              selectedId={selectedId}
              imageAspect={image?.aspect ?? null}
              onCreateText={(page, x, y) => add({ id: newId(), op: { kind: "text", page, x, y, text: "", size: textSize, color: textColor } })}
              onCreateBox={onCreateBox}
              onSelect={setSelectedId}
              onMove={onMove}
              onText={(id, text) => update(id, (i) => (i.op.kind === "text" ? { ...i, op: { ...i.op, text } } : i))}
              onTextDone={(id) => setItems((list) => list.filter((i) => i.id !== id || i.op.kind !== "text" || i.op.text.trim() !== ""))}
            />
          ))}
        </div>

        <aside className="panel rd-panel" aria-label="Edit settings">
          <section className="field">
            <h2 className="rd-title">Text</h2>
            <div className="ai-row" style={{ justifyContent: "flex-start", gap: 12 }}>
              <label className="field" style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
                <span className="result">Size</span>
                <select
                  className="input"
                  style={{ height: 34 }}
                  value={selected?.op.kind === "text" ? selected.op.size : textSize}
                  onChange={(e) => {
                    const size = Number(e.target.value);
                    setTextSize(size);
                    if (selected?.op.kind === "text") update(selected.id, (i) => (i.op.kind === "text" ? { ...i, op: { ...i.op, size } } : i));
                  }}
                >
                  {[8, 10, 11, 12, 14, 16, 20, 24, 32].map((s) => (
                    <option key={s} value={s}>
                      {s} pt
                    </option>
                  ))}
                </select>
              </label>
              <div role="group" aria-label="Text colour" style={{ display: "flex", gap: 6 }}>
                {COLORS.map((c) => (
                  <button
                    key={c.name}
                    type="button"
                    className="ed-swatch"
                    aria-label={c.name}
                    aria-pressed={textColor.join() === c.rgb.join()}
                    style={{ background: `rgb(${c.rgb.map((v) => v * 255).join(",")})` }}
                    onClick={() => {
                      setTextColor(c.rgb);
                      if (selected?.op.kind === "text") update(selected.id, (i) => (i.op.kind === "text" ? { ...i, op: { ...i.op, color: c.rgb } } : i));
                    }}
                  />
                ))}
              </div>
            </div>
            {image && (
              <div className="file-row" style={{ marginTop: 6 }}>
                <img src={image.url} alt="" style={{ height: 28, maxWidth: 60, objectFit: "contain" }} />
                <span className="name">{image.path.split(/[\\/]/).pop()}</span>
                <button type="button" className="btn-ghost solid" onClick={chooseImage}>
                  Change image
                </button>
              </div>
            )}
          </section>

          <section className="field" style={{ minHeight: 0 }}>
            <h2 className="rd-title">Changes ({items.length})</h2>
            <div className="rd-list" style={{ minHeight: 60 }}>
              {items.length === 0 && <p className="muted rd-empty">Pick a tool above and click or drag on the page.</p>}
              {items.map((item) => {
                const d = describe(item);
                return (
                  <div key={item.id} className={`rd-item${item.id === selectedId ? " focused" : ""}`}>
                    <button type="button" className="rd-item-text" onClick={() => setSelectedId(item.id)}>
                      <span className="rd-item-main">{d.title}</span>
                      <span className="muted rd-item-meta">{d.meta}</span>
                    </button>
                    <button type="button" className="icon-btn" aria-label={`Remove ${d.title}`} onClick={() => setItems((l) => l.filter((i) => i.id !== item.id))}>
                      <Icon name="trash" size={14} />
                    </button>
                  </div>
                );
              })}
            </div>
          </section>

          {fields.length > 0 && (
            <section className="field">
              <h2 className="rd-title">Form fields ({fields.length})</h2>
              <div className="ed-fields">
                {fields
                  .filter((f) => f.kind !== "button" && f.kind !== "signature")
                  .map((f) => (
                    <label key={`${f.page}-${f.name}`} className="field">
                      <span className="result">
                        {f.name} · p.{f.page}
                      </span>
                      {f.kind === "checkbox" ? (
                        <input
                          type="checkbox"
                          disabled={f.readOnly}
                          checked={values[f.name] !== "" && values[f.name] !== "Off"}
                          onChange={(e) => setValues((v) => ({ ...v, [f.name]: e.target.checked ? "Yes" : "Off" }))}
                        />
                      ) : (
                        <input className="input" disabled={f.readOnly} value={values[f.name] ?? ""} onChange={(e) => setValues((v) => ({ ...v, [f.name]: e.target.value }))} />
                      )}
                    </label>
                  ))}
              </div>
            </section>
          )}

          <div className="panel-footer">
            <p className="result" style={{ lineHeight: 1.5 }}>
              Saves a <strong>new copy</strong>; your original stays untouched. Whiteout only covers content: to remove sensitive
              text for good, use Redact.
            </p>
            {status && (
              <p className={`status ${status.kind}`} role="status">
                {status.text}
              </p>
            )}
            <button type="button" className="btn" disabled={busy || changes === 0} onClick={apply}>
              {busy ? "Saving…" : `Save copy (${changes} change${changes === 1 ? "" : "s"})`}
            </button>
          </div>
        </aside>
      </div>
    </div>
  );
}

import { useEffect, useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { dirname, join } from "@tauri-apps/api/path";
import { Icon } from "../../components/Icon";
import {
  errorMessage,
  redactApply,
  redactScan,
  redactSearch,
  type Area,
  type Category,
  type PageSize,
} from "../../lib/tauri";
import { CATEGORY_NAMES, SOURCE_NAMES, mergeSuggestions, pagesText, type Item } from "./items";
import { PageView } from "./PageView";

interface Props {
  path: string;
  name: string;
  onChangeFile: () => void;
}

type Status = { kind: "ok" | "err"; text: string } | null;
type Filter = "all" | Category;

const ORDER: Category[] = ["names", "addresses", "contact", "ids", "dates", "custom"];

export function RedactScreen({ path, name, onChangeFile }: Props) {
  const [pages, setPages] = useState<PageSize[]>([]);
  const [items, setItems] = useState<Item[]>([]);
  const [noText, setNoText] = useState<number[]>([]);
  const [scan, setScan] = useState<{ state: "running" } | { state: "done"; ms: number } | { state: "failed"; error: string }>({ state: "running" });
  const [filter, setFilter] = useState<Filter>("all");
  const [focusedId, setFocusedId] = useState<string | null>(null);
  const [drawing, setDrawing] = useState(false);
  const [query, setQuery] = useState("");
  const [searchNote, setSearchNote] = useState("");
  const [removeMetadata, setRemoveMetadata] = useState(true);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<Status>(null);

  useEffect(() => {
    let live = true;
    const t0 = performance.now();
    setScan({ state: "running" });
    setItems([]);
    setStatus(null);
    redactScan(path).then(
      (r) => {
        if (!live) return;
        setPages(r.pages);
        setNoText(r.pagesWithoutText);
        setItems(mergeSuggestions([], r.suggestions, "pending"));
        setScan({ state: "done", ms: performance.now() - t0 });
      },
      (e) => live && setScan({ state: "failed", error: errorMessage(e) }),
    );
    return () => {
      live = false;
    };
  }, [path]);

  const counts = useMemo(() => {
    const c = new Map<Filter, number>([["all", items.length]]);
    for (const i of items) c.set(i.category, (c.get(i.category) ?? 0) + 1);
    return c;
  }, [items]);
  const visible = items.filter((i) => filter === "all" || i.category === filter);
  const approved = items.filter((i) => i.status === "approved");
  const approvedAreas = approved.flatMap((i) => i.areas);

  const setStatusOf = (ids: Set<string>, to: Item["status"]) =>
    setItems((list) => list.map((i) => (ids.has(i.id) ? { ...i, status: to } : i)));
  const toggle = (id: string) => setItems((list) => list.map((i) => (i.id === id ? { ...i, status: i.status === "approved" ? "pending" : "approved" } : i)));
  const dismiss = (id: string) => setItems((list) => list.filter((i) => i.id !== id));

  function focus(item: Item) {
    setFocusedId(item.id);
    const first = item.areas[0];
    if (!first) return;
    document.querySelector(`[data-page="${first.page}"]`)?.scrollIntoView({ behavior: "smooth", block: "center" });
  }

  async function find(e: React.FormEvent) {
    e.preventDefault();
    const q = query.trim();
    if (!q) return;
    setSearchNote("");
    try {
      const hits = await redactSearch(path, q);
      if (hits.length === 0) {
        setSearchNote(`No matches for “${q}”.`);
        return;
      }
      setItems((list) => mergeSuggestions(list, hits, "approved"));
      setSearchNote(`Added ${hits.length} match${hits.length === 1 ? "" : "es"} for “${q}”, approved.`);
      setQuery("");
      setFilter("all");
    } catch (err) {
      setSearchNote(errorMessage(err));
    }
  }

  function draw(area: Area) {
    setItems((list) =>
      mergeSuggestions(list, [{ page: area.page, text: `Box on page ${area.page}`, label: "Drawn box", category: "custom", source: "manual", areas: [area] }], "approved"),
    );
  }

  async function apply() {
    setBusy(true);
    setStatus(null);
    try {
      const suggested = `${name.replace(/\.pdf$/i, "")}_redacted.pdf`;
      const target = await save({ defaultPath: await join(await dirname(path), suggested), filters: [{ name: "PDF", extensions: ["pdf"] }] });
      if (!target) return;
      const output = /\.pdf$/i.test(target) ? target : `${target}.pdf`;
      const report = await redactApply(path, output, approvedAreas, removeMetadata);
      setStatus({
        kind: "ok",
        text: `Saved ${output.split(/[\\/]/).pop()}. ${report.areas} box${report.areas === 1 ? "" : "es"} on ${report.pages} page${report.pages === 1 ? "" : "s"}; checked: no text left under them.`,
      });
    } catch (err) {
      setStatus({ kind: "err", text: errorMessage(err) });
    } finally {
      setBusy(false);
    }
  }

  const scanLine =
    scan.state === "running"
      ? "Scanning…"
      : scan.state === "failed"
        ? "Scan failed"
        : `${pages.length} page${pages.length === 1 ? "" : "s"} · scanned in ${(scan.ms / 1000).toFixed(1)} s`;

  return (
    <div className="rd">
      <header className="rd-header">
        <div className="rd-file">
          <span className="rd-name">{name}</span>
          <span className="muted">{scanLine}</span>
        </div>
        <div className="rd-actions">
          <button type="button" className="rd-tool" aria-pressed={drawing} onClick={() => setDrawing((d) => !d)}>
            <Icon name="box" size={16} />
            {drawing ? "Drawing — click to stop" : "Draw box"}
          </button>
          <button type="button" className="rd-tool" onClick={onChangeFile}>
            Choose another file
          </button>
        </div>
      </header>

      <div className="rd-body">
        <div className="rd-doc">
          {scan.state === "failed" && <p className="status err">{scan.error}</p>}
          {noText.length > 0 && (
            <p className="status warn">
              {noText.length === pages.length ? "This PDF has no text layer" : `Pages ${noText.join(", ")} have no text layer`} — probably scanned
              images. Automatic detection can't read them yet (OCR is coming); draw boxes over what must go.
            </p>
          )}
          {pages.map((p) => (
            <PageView
              key={p.page}
              path={path}
              size={p}
              items={items}
              focusedId={focusedId}
              drawing={drawing}
              onToggle={toggle}
              onDraw={draw}
            />
          ))}
        </div>

        <aside className="panel rd-panel" aria-label="Suggestions">
          <div>
            <h2 className="rd-title">
              {scan.state === "running" ? "Looking for sensitive data…" : `Found ${items.length} item${items.length === 1 ? "" : "s"}`}
            </h2>
            <p className="muted rd-sub">Review each one. Nothing is removed until you apply.</p>
          </div>

          <div className="rd-filters" role="group" aria-label="Filter by category">
            {(["all", ...ORDER] as Filter[])
              .filter((f) => f === "all" || counts.get(f))
              .map((f) => (
                <button key={f} type="button" className="rd-filter" aria-pressed={filter === f} onClick={() => setFilter(f)}>
                  {f === "all" ? "All" : CATEGORY_NAMES[f]} {counts.get(f) ?? 0}
                </button>
              ))}
          </div>

          <div className="rd-list">
            {visible.length === 0 && (
              <p className="muted rd-empty">
                {scan.state === "running" ? "Scanning…" : "Nothing here. Search for text or draw a box to add your own."}
              </p>
            )}
            {visible.map((item) => (
              <div key={item.id} className={`rd-item ${item.status}${item.id === focusedId ? " focused" : ""}`}>
                <input
                  type="checkbox"
                  checked={item.status === "approved"}
                  aria-label={`Redact ${item.text}`}
                  onChange={() => toggle(item.id)}
                />
                <button type="button" className="rd-item-text" onClick={() => focus(item)}>
                  <span className="rd-item-main">{item.text}</span>
                  <span className="muted rd-item-meta">
                    {item.label} · {pagesText(item.pages)} · {SOURCE_NAMES[item.source]}
                    {item.status === "approved" && " · approved"}
                  </span>
                </button>
                <button type="button" className="icon-btn" aria-label={`Dismiss ${item.text}`} title="Not sensitive — remove from list" onClick={() => dismiss(item.id)}>
                  <Icon name="close" size={14} />
                </button>
              </div>
            ))}
          </div>

          <form className="field" onSubmit={find}>
            <label htmlFor="rd-find" className="field-label">Find &amp; redact text</label>
            <div style={{ display: "flex", gap: 8 }}>
              <input id="rd-find" className="input" style={{ flex: 1, minWidth: 0 }} value={query} placeholder="e.g. a name or case number" onChange={(e) => setQuery(e.target.value)} />
              <button type="submit" className="btn-ghost solid" style={{ height: 40, borderRadius: 8 }} disabled={!query.trim()}>
                Add
              </button>
            </div>
            {searchNote && <span className="result">{searchNote}</span>}
            <span className="result">Names and addresses will be found by the on-device AI model, coming next.</span>
          </form>

          <div className="panel-footer">
            <label className="options" style={{ flexDirection: "row", alignItems: "center", gap: 10, fontSize: 13 }}>
              <input type="checkbox" checked={removeMetadata} onChange={(e) => setRemoveMetadata(e.target.checked)} />
              Also remove document metadata (author, title…)
            </label>
            <p className="result" style={{ lineHeight: 1.5 }}>
              Applying removes the text, images and drawings under each box and saves a <strong>new copy</strong>. Your original stays untouched.
            </p>
            {status && (
              <p className={`status ${status.kind}`} role="status">
                {status.text}
              </p>
            )}
            <div style={{ display: "flex", gap: 8 }}>
              <button
                type="button"
                className="btn btn-secondary"
                style={{ flex: 1 }}
                disabled={visible.every((i) => i.status === "approved")}
                onClick={() => setStatusOf(new Set(visible.map((i) => i.id)), "approved")}
              >
                Approve {filter === "all" ? "all" : "shown"}
              </button>
              <button type="button" className="btn btn-dark" style={{ flex: 1.4 }} disabled={busy || approvedAreas.length === 0} onClick={apply}>
                {busy ? "Applying…" : `Apply & save copy (${approved.length})`}
              </button>
            </div>
          </div>
        </aside>
      </div>
    </div>
  );
}

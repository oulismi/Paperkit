import { useState } from "react";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { dirname, join } from "@tauri-apps/api/path";
import { Icon } from "../../components/Icon";
import { convertPdf, errorMessage, IMAGE_EXTENSIONS, imagesToPdf, type ConvertTarget } from "../../lib/tauri";
import type { SourceFile } from "../../app/model";
import { parseRanges } from "../split-merge/splitPlan";

interface Props {
  /** The PDF to convert, if one is chosen. */
  file: { path: string; name: string; pageCount: number } | null;
  openFiles: SourceFile[];
  onPick: (file: SourceFile) => void;
  onBrowse: () => void;
  /** Images dropped or picked to turn into a PDF. */
  images: string[];
  onImages: (images: string[]) => void;
}

const TARGETS: { id: ConvertTarget | "images"; badge: string; color: string; title: string; desc: string }[] = [
  { id: "docx", badge: "DOCX", color: "#2B579A", title: "Word document", desc: "Editable text, layout kept roughly" },
  { id: "images", badge: "PNG · JPG", color: "#0F766E", title: "Images", desc: "One image per page" },
  { id: "txt", badge: "TXT", color: "#4B5563", title: "Plain text", desc: "Text only, with OCR for scans" },
  { id: "searchable", badge: "OCR", color: "#7C2D12", title: "Searchable PDF", desc: "Adds a text layer to scanned pages" },
];

type Status = { kind: "ok" | "err"; text: string } | null;
const name = (p: string) => p.split(/[\\/]/).pop() ?? p;
const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;

export function ConvertScreen({ file, openFiles, onPick, onBrowse, images, onImages }: Props) {
  const [target, setTarget] = useState<ConvertTarget | "images">("docx");
  const [imageFormat, setImageFormat] = useState<"png" | "jpg">("png");
  const [dpi, setDpi] = useState(150);
  const [range, setRange] = useState("");
  const [useOcr, setUseOcr] = useState(true);
  const [fit, setFit] = useState<"a4" | "image">("a4");
  const [progress, setProgress] = useState<{ done: number; total: number; label: string } | null>(null);
  const [status, setStatus] = useState<Status>(null);

  const pagePlan = file && range.trim() ? parseRanges(range, file.pageCount) : null;
  const pages = pagePlan?.ok ? [...new Set(pagePlan.groups.flat())] : null;
  const rangeError = pagePlan && !pagePlan.ok ? pagePlan.error : "";

  async function run(task: () => Promise<string | null>) {
    setStatus(null);
    try {
      const text = await task();
      if (text) setStatus({ kind: "ok", text });
    } catch (e) {
      setStatus({ kind: "err", text: errorMessage(e) });
    } finally {
      setProgress(null);
    }
  }

  const convert = () =>
    run(async () => {
      if (!file) return null;
      const stem = file.name.replace(/\.pdf$/i, "");
      const dir = await dirname(file.path);
      const actual: ConvertTarget = target === "images" ? imageFormat : target;
      let output: string | null;
      if (target === "images") {
        output = (await open({ directory: true, defaultPath: dir, title: "Choose a folder for the images" })) as string | null;
      } else {
        // Images were handled above; the remaining targets each write one file.
        const ext = ({ docx: "docx", txt: "txt", searchable: "pdf" } as const)[target as "docx" | "txt" | "searchable"];
        const suggested = target === "searchable" ? `${stem}_searchable.pdf` : `${stem}.${ext}`;
        output = await save({ defaultPath: await join(dir, suggested), filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
        if (output && !output.toLowerCase().endsWith(`.${ext}`)) output = `${output}.${ext}`;
      }
      if (!output) return null;

      const label = target === "images" ? "Rendering" : target === "docx" ? "Converting" : "Reading";
      const options = { path: file.path, target: actual, output, pages, dpi, useOcr, overwrite: false };
      const onProgress = (done: number, total: number) => setProgress({ done, total, label });
      setProgress({ done: 0, total: pages?.length ?? file.pageCount, label });
      let result = await convertPdf(options, onProgress);
      if (result.existing.length > 0) {
        const replace = await ask(`${plural(result.existing.length, "image")} already exist in that folder (${name(result.existing[0])}…). Replace them?`, {
          title: "Replace existing files?",
          kind: "warning",
          okLabel: "Replace",
        });
        if (!replace) return null;
        result = await convertPdf({ ...options, overwrite: true }, onProgress);
      }
      if (target === "searchable" && result.ocrPages.length === 0) return "Every page already has text, so the PDF is already searchable. Nothing was saved.";
      if (target === "searchable") return `Saved ${name(output)}: text added to ${plural(result.ocrPages.length, "scanned page")}.`;
      if (target === "images") return `Saved ${plural(result.written.length, "image")} (${name(result.written[0])}…).`;
      return `Saved ${name(output)}.`;
    });

  const makePdf = () =>
    run(async () => {
      if (images.length === 0) return null;
      const dir = await dirname(images[0]);
      const stem = name(images[0]).replace(/\.[^.]+$/, "");
      let output = await save({ defaultPath: await join(dir, `${stem}.pdf`), filters: [{ name: "PDF", extensions: ["pdf"] }] });
      if (!output) return null;
      if (!/\.pdf$/i.test(output)) output = `${output}.pdf`;
      setProgress({ done: 0, total: images.length, label: "Adding images" });
      await imagesToPdf(images, output, fit, (done, total) => setProgress({ done, total, label: "Adding images" }));
      onImages([]);
      return `Saved ${name(output)} · ${plural(images.length, "page")}.`;
    });

  async function pickImages() {
    const picked = await open({ multiple: true, filters: [{ name: "Images", extensions: IMAGE_EXTENSIONS }] });
    if (picked) onImages([...images, ...(Array.isArray(picked) ? picked : [picked])]);
  }

  const moveImage = (i: number, d: number) => {
    const j = i + d;
    if (j < 0 || j >= images.length) return;
    const next = [...images];
    [next[i], next[j]] = [next[j], next[i]];
    onImages(next);
  };

  return (
    <div className="cv">
      <div className="toolbar">
        <h1 className="cv-title">Convert</h1>
        {file ? (
          <span className="cv-file">
            <span className="cv-pdf" aria-hidden="true">PDF</span>
            <strong>{file.name}</strong>
            <span className="muted">{plural(file.pageCount, "page")}</span>
            <button type="button" className="btn-ghost solid" onClick={onBrowse}>
              Change
            </button>
          </span>
        ) : (
          <button type="button" className="btn" onClick={onBrowse}>
            Choose a PDF
          </button>
        )}
      </div>

      {!file && openFiles.length > 0 && (
        <div className="cv-open">
          <span className="muted">Or use an open file:</span>
          {openFiles.map((f) => (
            <button key={f.id} type="button" className="btn-ghost solid" onClick={() => onPick(f)}>
              {f.name}
            </button>
          ))}
        </div>
      )}

      <section aria-labelledby="cv-to" className="cv-section">
        <h2 id="cv-to" className="cv-h2">Convert PDF to</h2>
        <div className="cv-targets">
          {TARGETS.map((t) => (
            <button key={t.id} type="button" className="cv-target" aria-pressed={target === t.id} onClick={() => setTarget(t.id)}>
              <span className="cv-badge" style={{ background: t.color }}>
                {t.badge}
              </span>
              <span className="cv-target-title">{t.title}</span>
              <span className="muted cv-target-desc">{t.desc}</span>
            </button>
          ))}
        </div>
      </section>

      <section aria-labelledby="cv-opts" className="cv-options">
        <h2 id="cv-opts" className="cv-h2" style={{ flexBasis: "100%" }}>
          Options
        </h2>
        <label className="field">
          <span className="field-label">Pages</span>
          <input
            className="input"
            placeholder={file ? `All pages (${file.pageCount})` : "All pages"}
            value={range}
            onChange={(e) => setRange(e.target.value)}
            aria-describedby="cv-range-help"
          />
          <span id="cv-range-help" className={rangeError ? "error" : "result"}>
            {rangeError || "Empty = all, or e.g. 1-3, 7"}
          </span>
        </label>
        {target === "images" && (
          <>
            <label className="field">
              <span className="field-label">Format</span>
              <select className="input" value={imageFormat} onChange={(e) => setImageFormat(e.target.value as "png" | "jpg")}>
                <option value="png">PNG (sharp, larger)</option>
                <option value="jpg">JPG (smaller)</option>
              </select>
            </label>
            <label className="field">
              <span className="field-label">Resolution</span>
              <select className="input" value={dpi} onChange={(e) => setDpi(Number(e.target.value))}>
                <option value={96}>96 dpi (screen)</option>
                <option value={150}>150 dpi</option>
                <option value={300}>300 dpi (print)</option>
              </select>
            </label>
          </>
        )}
        {(target === "docx" || target === "txt") && (
          <label className="options" style={{ flexDirection: "row", alignItems: "center", gap: 10, alignSelf: "center", fontSize: 13 }}>
            <input type="checkbox" checked={useOcr} onChange={(e) => setUseOcr(e.target.checked)} />
            Run OCR on scanned pages
          </label>
        )}
      </section>

      <div className="cv-run">
        <div className="cv-progress">
          {progress ? (
            <>
              <div className="ai-row">
                <span style={{ fontWeight: 500, fontSize: 13 }}>{progress.label}…</span>
                <span className="result">
                  page {progress.done} of {progress.total}
                </span>
              </div>
              <progress className="cv-bar" max={progress.total} value={progress.done} aria-label="Conversion progress" />
            </>
          ) : status ? (
            <p className={`status ${status.kind}`} role="status">
              {status.text}
            </p>
          ) : (
            <span className="result">
              {target === "searchable" || (useOcr && target !== "images")
                ? "Scanned pages are read with OCR (English, French, Dutch, Greek); that takes a few seconds per page."
                : "Your original file is never changed."}
            </span>
          )}
        </div>
        <button type="button" className="btn" disabled={!file || !!progress || !!rangeError} onClick={convert}>
          Convert
        </button>
      </div>

      <section aria-labelledby="cv-from" className="cv-section">
        <h2 id="cv-from" className="cv-h2">Create PDF from images</h2>
        {images.length === 0 ? (
          <div className="cv-open">
            <button type="button" className="btn-ghost solid" style={{ height: 40, borderRadius: 8 }} onClick={pickImages}>
              <Icon name="upload" size={14} /> Choose images (JPG, PNG, TIFF…)
            </button>
            <span className="muted">or drop them here. Word, Excel and PowerPoint → PDF will be an optional add-on later.</span>
          </div>
        ) : (
          <div className="cv-images">
            {images.map((img, i) => (
              <div key={`${img}-${i}`} className="file-row" style={{ background: "var(--surface)" }}>
                <span className="muted">{i + 1}.</span>
                <span className="name" title={img}>
                  {name(img)}
                </span>
                <button type="button" className="icon-btn" aria-label={`Move ${name(img)} up`} disabled={i === 0} onClick={() => moveImage(i, -1)}>
                  <Icon name="up" size={14} />
                </button>
                <button type="button" className="icon-btn" aria-label={`Move ${name(img)} down`} disabled={i === images.length - 1} onClick={() => moveImage(i, 1)}>
                  <Icon name="down" size={14} />
                </button>
                <button type="button" className="icon-btn" aria-label={`Remove ${name(img)}`} onClick={() => onImages(images.filter((_, k) => k !== i))}>
                  <Icon name="close" size={14} />
                </button>
              </div>
            ))}
            <div className="cv-open">
              <button type="button" className="btn-ghost" onClick={pickImages}>
                + Add images
              </button>
              <label className="field" style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
                <span className="field-label">Page size</span>
                <select className="input" value={fit} onChange={(e) => setFit(e.target.value as "a4" | "image")}>
                  <option value="a4">A4, image fitted</option>
                  <option value="image">Same as each image</option>
                </select>
              </label>
              <button type="button" className="btn" style={{ marginLeft: "auto" }} disabled={!!progress} onClick={makePdf}>
                Create PDF ({plural(images.length, "page")})
              </button>
            </div>
          </div>
        )}
      </section>
    </div>
  );
}

import { useMemo, useState } from "react";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import { dirname, join } from "@tauri-apps/api/path";
import { Icon } from "../../components/Icon";
import { assemble, errorMessage, type OutputSpec } from "../../lib/tauri";
import type { Mode, PageItem, SourceFile } from "../../app/model";
import { baseName, partNames, type Plan, type SplitRule } from "./splitPlan";

interface Props {
  mode: Mode;
  onMode: (mode: Mode) => void;
  files: SourceFile[];
  pages: PageItem[];
  selected: Set<string>;
  onMoveFile: (fileId: string, delta: number) => void;
  /** Lets the grid show which output file each page goes to. */
  plan: Plan | null;
  splitRule: SplitRule;
  onSplitRule: (rule: SplitRule) => void;
}

type Status = { kind: "ok" | "err"; text: string } | null;

const MODES: { id: Mode; label: string }[] = [
  { id: "split", label: "Split" },
  { id: "merge", label: "Merge" },
  { id: "extract", label: "Extract" },
];

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
const withPdf = (name: string) => (/\.pdf$/i.test(name) ? name : `${name}.pdf`);
const PDF_FILTER = [{ name: "PDF", extensions: ["pdf"] }];

export function SettingsPanel(props: Props) {
  const { mode, onMode, files, pages, selected, plan, splitRule, onSplitRule } = props;
  const [onlySelected, setOnlySelected] = useState(false);
  const [extractEach, setExtractEach] = useState(false);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<Status>(null);
  const [everyN, setEveryN] = useState("1");
  const [rangesText, setRangesText] = useState("");

  const first = files[0];
  const defaultBase = first ? baseName(first.name) : "document";
  const [mergeName, setMergeName] = useState("");
  const [prefix, setPrefix] = useState("");
  const mergeFileName = withPdf(mergeName.trim() || (files.length > 1 ? `${defaultBase}_merged` : `${defaultBase}_edited`));
  const splitPrefix = prefix.trim() || defaultBase;

  const selectedPages = useMemo(() => pages.filter((p) => selected.has(p.key)), [pages, selected]);

  /** Positions (1-based) → PageRefs over only the files those pages use. */
  function buildOutputs(groups: PageItem[][], paths: string[]): { sources: string[]; outputs: OutputSpec[] } {
    const sources: string[] = [];
    const index = new Map<string, number>();
    const outputs = groups.map((group, i) => ({
      path: paths[i],
      pages: group.map((p) => {
        const file = files.find((f) => f.id === p.fileId)!;
        if (!index.has(file.id)) {
          index.set(file.id, sources.length);
          sources.push(file.path);
        }
        return { source: index.get(file.id)!, page: p.page, rotate: p.rotate };
      }),
    }));
    return { sources, outputs };
  }

  async function run(groups: PageItem[][], paths: string[]) {
    const { sources, outputs } = buildOutputs(groups, paths);
    const existing = await assemble(sources, outputs, false);
    if (existing.length > 0) {
      const names = existing.map((p) => p.split(/[\\/]/).pop()).join(", ");
      const replace = await ask(`${plural(existing.length, "file")} already exist: ${names}.\n\nReplace them?`, {
        title: "Replace existing files?",
        kind: "warning",
        okLabel: "Replace",
        cancelLabel: "Cancel",
      });
      if (!replace) return false;
      await assemble(sources, outputs, true);
    }
    return true;
  }

  async function act(task: () => Promise<string | null>) {
    setBusy(true);
    setStatus(null);
    try {
      const done = await task();
      if (done) setStatus({ kind: "ok", text: done });
    } catch (e) {
      setStatus({ kind: "err", text: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  }

  const startDir = async () => (first ? dirname(first.path) : undefined);

  const mergePages = onlySelected ? selectedPages : pages;
  const doMerge = () =>
    act(async () => {
      const dir = await startDir();
      const target = await save({ defaultPath: dir ? await join(dir, mergeFileName) : mergeFileName, filters: PDF_FILTER });
      if (!target) return null;
      const path = withPdf(target);
      // The save dialog already asked about replacing, so overwrite here.
      const { sources, outputs } = buildOutputs([mergePages], [path]);
      await assemble(sources, outputs, true);
      return `Saved ${path.split(/[\\/]/).pop()} · ${plural(mergePages.length, "page")}`;
    });

  const doSplit = () =>
    act(async () => {
      if (!plan?.ok) return null;
      const dir = await open({ directory: true, defaultPath: await startDir(), title: "Choose a folder for the split files" });
      if (!dir) return null;
      const names = partNames(splitPrefix, plan.groups.length);
      const paths = await Promise.all(names.map((n) => join(dir, n)));
      const groups = plan.groups.map((g) => g.map((pos) => pages[pos - 1]));
      if (!(await run(groups, paths))) return null;
      return `Saved ${plural(names.length, "file")}: ${names[0]}${names.length > 1 ? ` … ${names[names.length - 1]}` : ""}`;
    });

  const doExtract = () =>
    act(async () => {
      if (extractEach) {
        const dir = await open({ directory: true, defaultPath: await startDir(), title: "Choose a folder for the pages" });
        if (!dir) return null;
        const names = partNames(`${splitPrefix}_page`, selectedPages.length);
        const paths = await Promise.all(names.map((n) => join(dir, n)));
        if (!(await run(selectedPages.map((p) => [p]), paths))) return null;
        return `Saved ${plural(names.length, "file")}`;
      }
      const name = `${defaultBase}_extract.pdf`;
      const dir = await startDir();
      const target = await save({ defaultPath: dir ? await join(dir, name) : name, filters: PDF_FILTER });
      if (!target) return null;
      const path = withPdf(target);
      const { sources, outputs } = buildOutputs([selectedPages], [path]);
      await assemble(sources, outputs, true);
      return `Saved ${path.split(/[\\/]/).pop()} · ${plural(selectedPages.length, "page")}`;
    });

  return (
    <aside className="panel" aria-label={`${mode} settings`}>
      <div role="tablist" aria-label="Mode" className="tabs">
        {MODES.map((m) => (
          <button
            key={m.id}
            type="button"
            role="tab"
            aria-selected={mode === m.id}
            onClick={() => {
              onMode(m.id);
              setStatus(null);
            }}
          >
            {m.label}
          </button>
        ))}
      </div>

      {mode === "merge" && (
        <>
          <div className="field">
            <span className="field-label">File order</span>
            {files.map((f, i) => (
              <div key={f.id} className="file-row">
                <span className="dot" style={{ background: f.color.strip }} />
                <span className="name" title={f.path}>{f.name}</span>
                <span className="muted">{f.pageCount} p.</span>
                <button type="button" className="icon-btn" aria-label={`Move ${f.name} up`} disabled={i === 0} onClick={() => props.onMoveFile(f.id, -1)}>
                  <Icon name="up" size={14} />
                </button>
                <button type="button" className="icon-btn" aria-label={`Move ${f.name} down`} disabled={i === files.length - 1} onClick={() => props.onMoveFile(f.id, 1)}>
                  <Icon name="down" size={14} />
                </button>
              </div>
            ))}
            <span className="result">Moving a file regroups the pages by file.</span>
          </div>
          <div className="field">
            <label htmlFor="outname" className="field-label">Output file name</label>
            <input id="outname" className="input" value={mergeName} placeholder={mergeFileName} onChange={(e) => setMergeName(e.target.value)} />
          </div>
          <div className="options">
            <label>
              <input type="checkbox" checked={onlySelected} disabled={selected.size === 0} onChange={(e) => setOnlySelected(e.target.checked)} />
              Only selected pages ({selected.size})
            </label>
          </div>
          <Footer
            result={`Result: 1 file · ${plural(mergePages.length, "page")}`}
            label={files.length > 1 ? "Merge & save" : "Save as new PDF"}
            disabled={busy || mergePages.length === 0}
            onClick={doMerge}
            status={status}
          />
        </>
      )}

      {mode === "split" && (
        <>
          <fieldset className="options" style={{ border: 0, padding: 0, margin: 0 }}>
            <legend className="field-label" style={{ marginBottom: 10 }}>Split</legend>
            <label>
              <input type="radio" name="rule" checked={splitRule.kind === "every"} onChange={() => onSplitRule({ kind: "every", n: Number(everyN) })} />
              Every
              <input
                className="input small"
                type="number"
                min={1}
                aria-label="Pages per file"
                value={everyN}
                onChange={(e) => {
                  setEveryN(e.target.value);
                  onSplitRule({ kind: "every", n: Number(e.target.value) });
                }}
              />
              pages
            </label>
            <label>
              <input type="radio" name="rule" checked={splitRule.kind === "ranges"} onChange={() => onSplitRule({ kind: "ranges", text: rangesText })} />
              Custom ranges
            </label>
            {splitRule.kind === "ranges" && (
              <input
                className="input"
                aria-label="Page ranges"
                placeholder="e.g. 1-3, 4-8, 9-"
                value={rangesText}
                onChange={(e) => {
                  setRangesText(e.target.value);
                  onSplitRule({ kind: "ranges", text: e.target.value });
                }}
              />
            )}
            <label>
              <input type="radio" name="rule" checked={splitRule.kind === "selected"} onChange={() => onSplitRule({ kind: "selected", starts: [] })} />
              Start a new file at each selected page
            </label>
          </fieldset>
          <div className="field">
            <label htmlFor="prefix" className="field-label">File name prefix</label>
            <input id="prefix" className="input" value={prefix} placeholder={defaultBase} onChange={(e) => setPrefix(e.target.value)} />
          </div>
          {plan && !plan.ok && <p className="error">{plan.error}</p>}
          <Footer
            result={plan?.ok ? `Result: ${plural(plan.groups.length, "file")} · ${splitPrefix}_1.pdf …` : "Result: —"}
            label="Split & save"
            disabled={busy || !plan?.ok}
            onClick={doSplit}
            status={status}
          />
        </>
      )}

      {mode === "extract" && (
        <>
          <p className="muted" style={{ fontSize: 13, lineHeight: 1.5 }}>
            Select pages in the grid (Shift-click selects a range). They are saved in the order shown.
          </p>
          <div className="options">
            <label>
              <input type="radio" name="extract" checked={!extractEach} onChange={() => setExtractEach(false)} />
              As one file
            </label>
            <label>
              <input type="radio" name="extract" checked={extractEach} onChange={() => setExtractEach(true)} />
              One file per page
            </label>
          </div>
          <Footer
            result={`Selected: ${plural(selectedPages.length, "page")}`}
            label="Extract & save"
            disabled={busy || selectedPages.length === 0}
            onClick={doExtract}
            status={status}
          />
        </>
      )}
    </aside>
  );
}

function Footer(props: { result: string; label: string; disabled: boolean; onClick: () => void; status: Status }) {
  return (
    <div className="panel-footer">
      {props.status && (
        <p className={`status ${props.status.kind}`} role="status">
          {props.status.text}
        </p>
      )}
      <span className="result">{props.result}</span>
      <button type="button" className="btn" disabled={props.disabled} onClick={props.onClick}>
        {props.label}
      </button>
    </div>
  );
}


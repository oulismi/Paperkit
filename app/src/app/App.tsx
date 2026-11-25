import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { message, open } from "@tauri-apps/plugin-dialog";
import { Home } from "../features/home/Home";
import { Organizer } from "../features/split-merge/Organizer";
import { forget } from "../lib/pdf";
import { errorMessage, openPdf } from "../lib/tauri";
import { colorFor, newId, type Mode, type PageItem, type SourceFile } from "./model";
import { Sidebar } from "./Sidebar";
import { TOOLS, type ToolId } from "./tools";

type View = "home" | "organize" | "redact" | "edit" | "convert";

export function App() {
  const [view, setView] = useState<View>("home");
  const [mode, setMode] = useState<Mode>("split");
  const [files, setFiles] = useState<SourceFile[]>([]);
  const [pages, setPages] = useState<PageItem[]>([]);
  const [dragging, setDragging] = useState(false);
  const colorIndex = useRef(0);
  const filesRef = useRef(files);
  filesRef.current = files;

  const addPaths = useCallback(async (paths: string[]) => {
    const pdfs = paths.filter((p) => /\.pdf$/i.test(p));
    const problems: string[] = [];
    if (pdfs.length < paths.length) {
      problems.push(`Skipped ${paths.length - pdfs.length} file(s): only PDF files can be opened.`);
    }

    const added: SourceFile[] = [];
    for (const path of pdfs) {
      if (filesRef.current.some((f) => f.path === path) || added.some((f) => f.path === path)) continue;
      try {
        const info = await openPdf(path);
        added.push({ id: newId(), path: info.path, name: info.name, pageCount: info.pageCount, color: colorFor(colorIndex.current++) });
      } catch (e) {
        problems.push(errorMessage(e));
      }
    }

    if (added.length > 0) {
      const wasOpen = filesRef.current.length > 0;
      const total = filesRef.current.length + added.length;
      setFiles((fs) => [...fs, ...added]);
      setPages((ps) => [
        ...ps,
        ...added.flatMap((f) => Array.from({ length: f.pageCount }, (_, i) => ({ key: newId(), fileId: f.id, page: i + 1, rotate: 0 }))),
      ]);
      // Several files dropped at once most likely means "merge these".
      if (!wasOpen && total > 1) setMode("merge");
      setView("organize");
    }
    if (problems.length > 0) await message(problems.join("\n"), { title: "Some files could not be opened", kind: "warning" });
  }, []);

  const browse = useCallback(async () => {
    const picked = await open({ multiple: true, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (picked) await addPaths(Array.isArray(picked) ? picked : [picked]);
  }, [addPaths]);

  // Files dragged in from the OS. HTML drop events carry no file paths in Tauri, so use its own event.
  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "enter" || e.payload.type === "over") setDragging(true);
      else {
        setDragging(false);
        if (e.payload.type === "drop") void addPaths(e.payload.paths);
      }
    });
    return () => void unlisten.then((f) => f());
  }, [addPaths]);

  const removeFile = (fileId: string) => {
    const file = files.find((f) => f.id === fileId);
    if (file) forget(file.path);
    const left = files.filter((f) => f.id !== fileId);
    setFiles(left);
    setPages((ps) => ps.filter((p) => p.fileId !== fileId));
    if (left.length === 0) setView("home");
  };

  const closeAll = () => {
    files.forEach((f) => forget(f.path));
    setFiles([]);
    setPages([]);
    setView("home");
  };

  const moveFile = (fileId: string, delta: number) => {
    const i = files.findIndex((f) => f.id === fileId);
    const j = i + delta;
    if (i < 0 || j < 0 || j >= files.length) return;
    const order = [...files];
    [order[i], order[j]] = [order[j], order[i]];
    setFiles(order);
    const rank = new Map(order.map((f, k) => [f.id, k]));
    setPages((ps) => [...ps].sort((a, b) => rank.get(a.fileId)! - rank.get(b.fileId)!));
  };

  const goTo = (to: "home" | ToolId) => {
    if (to === "home") return setView("home");
    if (to === "split" || to === "merge") {
      setMode(to);
      if (files.length > 0) setView("organize");
      else void browse();
      return;
    }
    setView(to);
  };

  const current: "home" | ToolId = view === "organize" ? (mode === "merge" ? "merge" : "split") : view;
  const soon = TOOLS.find((t) => t.id === view);

  return (
    <div className="shell">
      <Sidebar current={current} onNavigate={goTo} />
      <main className="main">
        {view === "home" && <Home onBrowse={browse} onTool={goTo} />}
        {view === "organize" && (
          <Organizer
            mode={mode}
            onMode={setMode}
            files={files}
            pages={pages}
            onPages={setPages}
            onAddFiles={browse}
            onRemoveFile={removeFile}
            onMoveFile={moveFile}
            onCloseAll={closeAll}
          />
        )}
        {soon && (
          <div className="soon-screen">
            <h1>{soon.label} is coming soon</h1>
            <p>{soon.description}.</p>
            <p>
              This build covers splitting, merging, extracting, reordering and rotating pages. {soon.label} is next on the
              roadmap in docs/ARCHITECTURE.md.
            </p>
          </div>
        )}
      </main>
      {dragging && <div className="drop-overlay">Drop PDF files to open them</div>}
    </div>
  );
}

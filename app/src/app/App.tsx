import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { message, open } from "@tauri-apps/plugin-dialog";
import { Home } from "../features/home/Home";
import { Organizer } from "../features/split-merge/Organizer";
import { RedactScreen } from "../features/redact/RedactScreen";
import { RedactStart } from "../features/redact/RedactStart";
import { ConvertScreen } from "../features/convert/ConvertScreen";
import { EditScreen } from "../features/edit/EditScreen";
import { PickFile } from "../components/PickFile";
import { forget } from "../lib/pdf";
import { errorMessage, isImage, openPdf } from "../lib/tauri";
import { colorFor, newId, type Mode, type PageItem, type SourceFile } from "./model";
import { Sidebar } from "./Sidebar";
import { TOOLS, type ToolId } from "./tools";
import { UpdateBanner } from "./UpdateBanner";
import { LogicalSize, getCurrentWindow } from "@tauri-apps/api/window";
import type { Item as EditItem } from "../features/edit/EditPage";

type View = "home" | "organize" | "redact" | "edit" | "convert";

export function App() {
  const [view, setView] = useState<View>("home");
  const [mode, setMode] = useState<Mode>("split");
  const [files, setFiles] = useState<SourceFile[]>([]);
  const [pages, setPages] = useState<PageItem[]>([]);
  const [dragging, setDragging] = useState(false);
  const [redactTarget, setRedactTarget] = useState<{ path: string; name: string } | null>(null);
  const [convertTarget, setConvertTarget] = useState<{ path: string; name: string; pageCount: number } | null>(null);
  const [convertImages, setConvertImages] = useState<string[]>([]);
  const [editTarget, setEditTarget] = useState<{ path: string; name: string } | null>(null);
  const [demo, setDemo] = useState<{ screen: string } | null>(null);
  const [editDemoItems, setEditDemoItems] = useState<EditItem[] | undefined>(undefined);
  const viewRef = useRef(view);
  viewRef.current = view;
  const colorIndex = useRef(0);
  const filesRef = useRef(files);
  filesRef.current = files;

  const addPaths = useCallback(async (paths: string[]) => {
    const pdfs = paths.filter((p) => /\.pdf$/i.test(p));
    const problems: string[] = [];

    // On the Convert screen: images go to "Create PDF from images", a PDF becomes the file to convert.
    if (viewRef.current === "convert") {
      const images = paths.filter(isImage);
      if (images.length > 0) setConvertImages((list) => [...list, ...images]);
      if (pdfs.length > 0) {
        try {
          const info = await openPdf(pdfs[0]);
          setConvertTarget({ path: info.path, name: info.name, pageCount: info.pageCount });
        } catch (e) {
          await message(errorMessage(e), { title: "Could not open the file", kind: "warning" });
        }
      }
      if (images.length + pdfs.length < paths.length) {
        await message("Only PDFs and images (JPG, PNG, TIFF, BMP, GIF) can be used here.", { title: "Some files were skipped", kind: "warning" });
      }
      return;
    }

    // On the Redact and Edit screens, a dropped or picked file is the one to work on.
    if (viewRef.current === "redact" || viewRef.current === "edit") {
      if (pdfs.length === 0) return void (await message("Only PDF files can be opened here.", { title: "Not a PDF", kind: "warning" }));
      try {
        const info = await openPdf(pdfs[0]);
        (viewRef.current === "redact" ? setRedactTarget : setEditTarget)({ path: info.path, name: info.name });
      } catch (e) {
        await message(errorMessage(e), { title: "Could not open the file", kind: "warning" });
      }
      return;
    }

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
    const single = viewRef.current === "redact" || viewRef.current === "convert" || viewRef.current === "edit";
    const picked = await open({ multiple: !single, filters: [{ name: "PDF", extensions: ["pdf"] }] });
    if (picked) await addPaths(Array.isArray(picked) ? picked : [picked]);
  }, [addPaths]);

  // Demo mode (screenshots): open one screen with the sample files, at a fixed size.
  const demoStarted = useRef(false);
  useEffect(() => {
    if (demoStarted.current) return; // React's dev mode runs effects twice
    demoStarted.current = true;
    void invoke<{ screen: string; files: string } | null>("demo_request").then(async (d) => {
      if (!d) return;
      setDemo(d);
      // Fixed size for consistent shots; never let this stop the demo.
      await getCurrentWindow().setSize(new LogicalSize(1280, 800)).catch(() => {});
      await getCurrentWindow().center().catch(() => {});
      const f = (name: string) => `${d.files}\\${name}`;
      const pdf = async (name: string) => openPdf(f(name));
      switch (d.screen) {
        case "organize":
          viewRef.current = "organize";
          setView("organize");
          await addPaths([f("Report_Q1.pdf"), f("Annex_A.pdf")]);
          setMode("merge");
          break;
        case "redact": {
          const info = await pdf("Employment_Contract.pdf");
          setRedactTarget({ path: info.path, name: info.name });
          setView("redact");
          break;
        }
        case "convert": {
          const info = await pdf("Scanned_Contract.pdf");
          setConvertTarget({ path: info.path, name: info.name, pageCount: info.pageCount });
          setView("convert");
          break;
        }
        case "edit": {
          const info = await pdf("Employment_Contract.pdf");
          setEditDemoItems([
            { id: "demo-stamp", op: { kind: "text", page: 1, x: 330, y: 32, text: "Approved for signature · 25 Nov 2026", size: 12, color: [0.12, 0.37, 0.82] } },
            { id: "demo-mark", op: { kind: "highlight", area: { page: 1, x0: 58, y0: 192, x1: 436, y1: 208 }, color: [1, 0.85, 0.2] } },
          ]);
          setEditTarget({ path: info.path, name: info.name });
          setView("edit");
          break;
        }
        default:
          setView("home");
      }
    });
    // Runs once at startup.
  }, []);

  // Files handed over from the floating drop bubble: open the chosen tool with them.
  useEffect(() => {
    const take = async () => {
      const request = await invoke<{ action: string; paths: string[] } | null>("take_open_request");
      if (!request) return;
      const organize = request.action === "split" || request.action === "merge";
      const next = (organize ? "organize" : request.action) as View;
      setView(next);
      viewRef.current = next; // addPaths routes by the current view
      await addPaths(request.paths);
      if (organize) setMode(request.action as Mode);
    };
    void take(); // a request may have arrived before this window finished loading
    const unlisten = listen("paperkit://open", () => void take());
    return () => void unlisten.then((f) => f());
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
  const soon = TOOLS.find((t) => t.id === view && !t.ready);

  return (
    <div className="shell">
      <Sidebar current={current} onNavigate={goTo} />
      <main className="main">
        <UpdateBanner enabled={!import.meta.env.DEV && !demo} />
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
        {view === "redact" &&
          (redactTarget ? (
            <RedactScreen
              key={redactTarget.path}
              path={redactTarget.path}
              name={redactTarget.name}
              onChangeFile={() => setRedactTarget(null)}
              autoApprove={demo ? (source) => source === "rule" : undefined}
            />
          ) : (
            <RedactStart openFiles={files} onPick={(f) => setRedactTarget({ path: f.path, name: f.name })} onBrowse={browse} />
          ))}
        {view === "edit" &&
          (editTarget ? (
            <EditScreen key={editTarget.path} path={editTarget.path} name={editTarget.name} onChangeFile={() => setEditTarget(null)} initialItems={editDemoItems} />
          ) : (
            <PickFile
              title="Edit a PDF"
              intro="Add text, images or a signature, cover areas in white, highlight, and fill in forms. A new copy is saved; the original stays as it is."
              icon="edit"
              openFiles={files}
              onPick={(f) => setEditTarget({ path: f.path, name: f.name })}
              onBrowse={browse}
            />
          ))}
        {view === "convert" && (
          <ConvertScreen
            file={convertTarget}
            openFiles={files}
            onPick={(f) => setConvertTarget({ path: f.path, name: f.name, pageCount: f.pageCount })}
            onBrowse={browse}
            images={convertImages}
            onImages={setConvertImages}
          />
        )}
        {soon && (
          <div className="soon-screen">
            <h1>{soon.label} is coming soon</h1>
            <p>{soon.description}.</p>
            <p>
              This build covers splitting, merging, extracting, reordering and rotating pages, redaction and conversion. {soon.label} is on
              the roadmap in docs/ARCHITECTURE.md.
            </p>
          </div>
        )}
      </main>
      {dragging && <div className="drop-overlay">Drop PDF files to open them</div>}
    </div>
  );
}

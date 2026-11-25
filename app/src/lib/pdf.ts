// Page previews with pdf.js. Each file is parsed once and each thumbnail drawn once.
import * as pdfjs from "pdfjs-dist";
import type { PDFDocumentLoadingTask, PDFDocumentProxy } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { readPdf } from "./tauri";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

const THUMB_WIDTH = 260; // px; tiles are ~130 css px wide, so this stays sharp on 2x screens

const docs = new Map<string, Promise<PDFDocumentProxy>>();
const tasks = new Map<string, PDFDocumentLoadingTask>();
const thumbs = new Map<string, Promise<string>>();

function loadDocument(path: string) {
  let doc = docs.get(path);
  if (!doc) {
    doc = readPdf(path).then((data) => {
      const task = pdfjs.getDocument({
        data: new Uint8Array(data),
        cMapUrl: "/pdfjs/cmaps/",
        standardFontDataUrl: "/pdfjs/standard_fonts/",
        wasmUrl: "/pdfjs/wasm/",
        iccUrl: "/pdfjs/iccs/",
      });
      tasks.set(path, task);
      return task.promise;
    });
    doc.catch(() => docs.delete(path));
    docs.set(path, doc);
  }
  return doc;
}

/** Renders page `page` (1-based) without the page's own rotation; returns an object URL. */
export function thumbnail(path: string, page: number): Promise<string> {
  const key = `${path}#${page}`;
  let url = thumbs.get(key);
  if (!url) {
    url = (async () => {
      const pdfPage = await (await loadDocument(path)).getPage(page);
      const base = pdfPage.getViewport({ scale: 1, rotation: 0 });
      const viewport = pdfPage.getViewport({ scale: THUMB_WIDTH / base.width, rotation: 0 });
      const canvas = document.createElement("canvas");
      canvas.width = Math.ceil(viewport.width);
      canvas.height = Math.ceil(viewport.height);
      await pdfPage.render({ canvas, viewport }).promise;
      const blob = await new Promise<Blob | null>((ok) => canvas.toBlob(ok));
      if (!blob) throw new Error("could not draw page preview");
      return URL.createObjectURL(blob);
    })();
    url.catch(() => thumbs.delete(key));
    thumbs.set(key, url);
  }
  return url;
}

const pageImages = new Map<string, Promise<string>>();

/**
 * Renders page `page` (1-based) with its own rotation, `width` px wide, for the
 * redaction view. Its layout matches MuPDF's page space, so boxes line up.
 */
export function pageImage(path: string, page: number, width: number): Promise<string> {
  const key = `${path}#${page}@${width}`;
  let url = pageImages.get(key);
  if (!url) {
    url = (async () => {
      const pdfPage = await (await loadDocument(path)).getPage(page);
      const base = pdfPage.getViewport({ scale: 1 });
      const viewport = pdfPage.getViewport({ scale: width / base.width });
      const canvas = document.createElement("canvas");
      canvas.width = Math.ceil(viewport.width);
      canvas.height = Math.ceil(viewport.height);
      await pdfPage.render({ canvas, viewport }).promise;
      const blob = await new Promise<Blob | null>((ok) => canvas.toBlob(ok));
      if (!blob) throw new Error("could not draw page");
      return URL.createObjectURL(blob);
    })();
    url.catch(() => pageImages.delete(key));
    pageImages.set(key, url);
  }
  return url;
}

/** The rotation a page already has inside its file, so previews can show it the way readers will. */
export async function intrinsicRotation(path: string, page: number): Promise<number> {
  return (await (await loadDocument(path)).getPage(page)).rotate;
}

/** Frees a closed file's parsed document and previews. */
export function forget(path: string) {
  tasks.get(path)?.destroy().catch(() => {});
  tasks.delete(path);
  docs.delete(path);
  for (const cache of [thumbs, pageImages]) {
    for (const [key, url] of cache) {
      if (key.startsWith(`${path}#`)) {
        url.then(URL.revokeObjectURL).catch(() => {});
        cache.delete(key);
      }
    }
  }
}

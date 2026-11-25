// Typed wrappers around the Rust commands in src-tauri/src/commands.
import { Channel, invoke } from "@tauri-apps/api/core";

export interface PdfInfo {
  path: string;
  name: string;
  pageCount: number;
}

/** One output page: page `page` (1-based) of `sources[source]`, turned by `rotate` degrees. */
export interface PageRef {
  source: number;
  page: number;
  rotate: number;
}

export interface OutputSpec {
  path: string;
  pages: PageRef[];
}

export const openPdf = (path: string) => invoke<PdfInfo>("open_pdf", { path });

export const readPdf = (path: string) => invoke<ArrayBuffer>("read_pdf", { path });

/** Returns the output paths that already exist and were left alone; empty when everything was written. */
export const assemble = (sources: string[], outputs: OutputSpec[], overwrite: boolean) =>
  invoke<string[]>("assemble", { sources, outputs, overwrite });

export const errorMessage = (e: unknown) => (e instanceof Error ? e.message : String(e));

// ---- Redaction ----

export type Category = "names" | "addresses" | "contact" | "ids" | "dates" | "custom";

/** A box in PDF points: origin top-left of the visible page, y downwards. */
export interface Area {
  page: number;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

export interface Suggestion {
  page: number;
  text: string;
  label: string;
  category: Category;
  source: "rule" | "search" | "manual" | "ai";
  /** The model's probability, for AI findings. */
  confidence?: number | null;
  areas: Area[];
}

export interface PageSize {
  page: number;
  width: number;
  height: number;
}

export interface ScanResult {
  pages: PageSize[];
  suggestions: Suggestion[];
  /** Pages read with OCR (no text layer, probably scanned). */
  pagesOcr: number[];
  /** Pages with no text even after OCR. */
  pagesWithoutText: number[];
}

export interface RedactReport {
  areas: number;
  pages: number;
  verifiedCharsLeft: number;
}

export const redactScan = (path: string) => invoke<ScanResult>("redact_scan", { path });

export const redactSearch = (path: string, query: string) => invoke<Suggestion[]>("redact_search", { path, query });

export const redactApply = (path: string, output: string, areas: Area[], removeMetadata: boolean) =>
  invoke<RedactReport>("redact_apply", { path, output, areas, removeMetadata });

/** Runs the on-device model. `defaults` = people and addresses; `extra` = the user's own descriptions. */
export const redactAiScan = (path: string, defaults: boolean, extra: string[]) =>
  invoke<Suggestion[]>("redact_ai_scan", { path, defaults, extra });

// ---- AI model ----

export interface ModelStatus {
  state: "bundled" | "downloaded" | "missing";
  downloadBytes: number;
  name: string;
}

export const modelStatus = () => invoke<ModelStatus>("model_status");

export function modelDownload(onProgress: (downloaded: number, total: number) => void) {
  const progress = new Channel<{ downloaded: number; total: number }>();
  progress.onmessage = (p) => onProgress(p.downloaded, p.total);
  return invoke<void>("model_download", { progress });
}

export const modelDownloadCancel = () => invoke<void>("model_download_cancel");

// ---- Convert ----

export type ConvertTarget = "docx" | "png" | "jpg" | "txt" | "searchable";

export interface ConvertResult {
  written: string[];
  /** Not empty = nothing written: these files exist and overwrite was false. */
  existing: string[];
  ocrPages: number[];
}

export interface ConvertOptions {
  path: string;
  target: ConvertTarget;
  /** The file to write, or for images the folder. */
  output: string;
  /** null = all pages */
  pages: number[] | null;
  dpi?: number;
  useOcr: boolean;
  overwrite: boolean;
}

export function convertPdf(options: ConvertOptions, onProgress: (done: number, total: number) => void) {
  const progress = new Channel<{ done: number; total: number }>();
  progress.onmessage = (p) => onProgress(p.done, p.total);
  return invoke<ConvertResult>("convert_pdf", { ...options, progress });
}

export function imagesToPdf(images: string[], output: string, fit: "image" | "a4", onProgress: (done: number, total: number) => void) {
  const progress = new Channel<{ done: number; total: number }>();
  progress.onmessage = (p) => onProgress(p.done, p.total);
  return invoke<void>("images_to_pdf", { images, output, fit, progress });
}

// Formats MuPDF decodes.
export const IMAGE_EXTENSIONS = ["jpg", "jpeg", "png", "tif", "tiff", "bmp", "gif"];
export const isImage = (path: string) => IMAGE_EXTENSIONS.includes(path.split(".").pop()?.toLowerCase() ?? "");

// ---- Edit ----

export type EditOp =
  | { kind: "text"; page: number; x: number; y: number; text: string; size: number; color: [number, number, number] }
  | { kind: "image"; area: Area; path: string }
  | { kind: "whiteout"; area: Area }
  | { kind: "highlight"; area: Area; color: [number, number, number] };

export interface FormField {
  page: number;
  name: string;
  kind: "text" | "checkbox" | "radio" | "choice" | "signature" | "button";
  value: string;
  readOnly: boolean;
  area: Area;
}

export const editInfo = (path: string) => invoke<{ pages: PageSize[]; fields: FormField[] }>("edit_info", { path });
export const editApply = (path: string, output: string, ops: EditOp[], fields: Record<string, string>) =>
  invoke<void>("edit_apply", { path, output, ops, fields });
export const readImage = (path: string) => invoke<ArrayBuffer>("read_image", { path });
export const editFont = () => invoke<ArrayBuffer>("edit_font");

// ---- First-launch AI offer ----

export const aiOffer = () => invoke<{ show: boolean; startDownload: boolean }>("ai_offer");
export const aiOfferDecline = () => invoke<void>("ai_offer_decline");

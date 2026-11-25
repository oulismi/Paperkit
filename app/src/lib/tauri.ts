// Typed wrappers around the Rust commands in src-tauri/src/commands.
import { invoke } from "@tauri-apps/api/core";

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
  source: "rule" | "search" | "manual";
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

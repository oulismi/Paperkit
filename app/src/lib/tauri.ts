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

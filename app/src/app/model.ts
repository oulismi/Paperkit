// What the organizer works on: open files and the ordered list of output pages.

export interface SourceFile {
  id: string;
  path: string;
  name: string;
  pageCount: number;
  color: FileColor;
}

/** A page in the organizer: page `page` (1-based) of a source file, plus rotation added by the user. */
export interface PageItem {
  key: string;
  fileId: string;
  page: number;
  rotate: number;
}

export interface FileColor {
  strip: string;
  bg: string;
  text: string;
}

// Each pair passes 4.5:1 for the chip text on its background.
const COLORS: FileColor[] = [
  { strip: "#1F5FD1", bg: "#E8EEFB", text: "#174AA6" },
  { strip: "#C2570C", bg: "#FDF1E4", text: "#8A3B0A" },
  { strip: "#0E7C66", bg: "#E3F4EF", text: "#0A5A4A" },
  { strip: "#B4237A", bg: "#FBE8F2", text: "#86195A" },
  { strip: "#5B6270", bg: "#EEF0F3", text: "#3A404C" },
  { strip: "#8F6B00", bg: "#FBF3DC", text: "#664C00" },
];

export const colorFor = (index: number) => COLORS[index % COLORS.length];

let counter = 0;
export const newId = () => `${Date.now().toString(36)}-${(counter++).toString(36)}`;

export type Mode = "split" | "merge" | "extract";

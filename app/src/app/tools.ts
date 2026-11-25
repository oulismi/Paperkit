import type { IconName } from "../components/Icon";

export type ToolId = "split" | "merge" | "redact" | "edit" | "convert";

export interface Tool {
  id: ToolId;
  label: string;
  icon: IconName;
  description: string;
  ready: boolean;
  ai?: boolean;
}

export const TOOLS: Tool[] = [
  { id: "split", label: "Split", icon: "split", description: "By range, every N pages or at selected pages", ready: true },
  { id: "merge", label: "Merge", icon: "merge", description: "Combine files, reorder pages by dragging", ready: true },
  { id: "redact", label: "Redact", icon: "redact", description: "AI finds names, IDs, emails, IBANs — you approve", ready: false, ai: true },
  { id: "edit", label: "Edit", icon: "edit", description: "Add text, images, signatures, fill forms", ready: false },
  { id: "convert", label: "Convert", icon: "convert", description: "PDF ↔ Word, images, text, PDF/A", ready: false },
];

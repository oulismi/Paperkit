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
  { id: "redact", label: "Redact", icon: "redact", description: "Finds emails, phones, IBANs, IDs, dates — you approve", ready: true, ai: true },
  { id: "edit", label: "Edit", icon: "edit", description: "Add text, images, signatures, fill forms", ready: true },
  { id: "convert", label: "Convert", icon: "convert", description: "PDF → Word, images, text, searchable PDF; images → PDF", ready: true },
];

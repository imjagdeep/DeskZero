// Display helpers.

import { CATEGORIES, type Category, type Condition, type Destination, type Rule } from "./types";

export function humanSize(bytes: number): string {
  if (bytes >= 1e9) return `${(bytes / 1e9).toFixed(1)} GB`;
  if (bytes >= 1e6) return `${(bytes / 1e6).toFixed(1)} MB`;
  if (bytes >= 1e3) return `${(bytes / 1e3).toFixed(1)} KB`;
  return `${bytes} B`;
}

export function fileName(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

export function parentDir(path: string): string {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i > 0 ? path.slice(0, i) : path;
}

/** Path relative to `root` when inside it, else the full path. */
export function relTo(path: string, root: string | null): string {
  if (!root) return path;
  const norm = (p: string) => p.replace(/\\/g, "/").toLowerCase();
  const r = norm(root).replace(/\/$/, "");
  return norm(path).startsWith(r + "/") ? path.slice(r.length + 1) : path;
}

export function categoryLabel(c: Category): string {
  return CATEGORIES.find((x) => x.value === c)?.label ?? c;
}

export function dateTime(iso: string | null): string {
  if (!iso) return "?";
  const d = new Date(iso);
  return isNaN(d.getTime()) ? iso : d.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

export function dayLabel(iso: string): string {
  const d = new Date(iso);
  const today = new Date();
  const y = new Date();
  y.setDate(today.getDate() - 1);
  if (d.toDateString() === today.toDateString()) return "Today";
  if (d.toDateString() === y.toDateString()) return "Yesterday";
  return d.toLocaleDateString(undefined, { dateStyle: "full" });
}

const FIELD_LABEL: Record<Condition["field"], string> = {
  extension: "extension",
  filename: "file name",
  type: "file type",
  size_mb: "size (MB)",
  created: "created",
  modified: "modified",
};

const OP_LABEL: Record<Condition["op"], string> = {
  is: "is",
  contains: "contains",
  in: "is one of",
  lt: "<",
  lte: "≤",
  gt: ">",
  gte: "≥",
};

export function conditionText(c: Condition): string {
  const v = Array.isArray(c.value)
    ? c.value.join(", ")
    : c.field === "type" && typeof c.value === "string"
      ? categoryLabel(c.value as Category)
      : String(c.value);
  return `${FIELD_LABEL[c.field]} ${OP_LABEL[c.op]} ${c.field === "filename" ? `"${v}"` : v}`;
}

export function destinationText(d: Destination): string {
  return d.type === "category" ? categoryLabel(d.category) : d.path;
}

export function ruleText(r: Rule): string {
  return `IF ${r.conditions.map(conditionText).join(" AND ")} THEN move to ${destinationText(r.destination)}`;
}

import { ask, open, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { PageHeader, pickFolder } from "../components/Common";
import { api } from "../lib/api";
import { categoryLabel, conditionText, destinationText } from "../lib/format";
import { refreshConfig, toast, toastError, useConfig } from "../lib/store";
import { CATEGORIES, type Category, type Condition, type ConditionField, type Op, type Rule, type RuleKind } from "../lib/types";

const TIERS: { kind: RuleKind; title: string; help: string }[] = [
  { kind: "filename", title: "1. File name rules", help: "Checked first, e.g. name contains \"invoice\"." },
  { kind: "custom", title: "2. Custom rules", help: "Any mix of conditions; all must match." },
  { kind: "extension", title: "3. Extension rules", help: "e.g. extension is pdf." },
];

// Type conditions use the engine's singular names ("video", not "videos").
const TYPE_VALUE: Record<Category, string> = {
  images: "image", videos: "video", audio: "audio", documents: "document", spreadsheets: "spreadsheet",
  presentations: "presentation", archives: "archive", applications: "application", installers: "installer",
  disk_images: "disk_image", code: "code", other: "other",
};
const TYPE_FROM_VALUE = Object.fromEntries(Object.entries(TYPE_VALUE).map(([k, v]) => [v, k])) as Record<string, Category>;

const OPS: Record<ConditionField, Op[]> = {
  extension: ["is", "in", "contains"],
  filename: ["contains", "is"],
  type: ["is"],
  size_mb: ["gt", "gte", "lt", "lte"],
  created: ["gt", "lt", "is"],
  modified: ["gt", "lt", "is"],
};
const OP_TEXT: Record<Op, string> = { is: "is", contains: "contains", in: "is one of", lt: "less than", lte: "at most", gt: "greater than", gte: "at least" };
const DATE_OP_TEXT: Partial<Record<Op, string>> = { gt: "after", lt: "before", is: "on" };
const FIELD_TEXT: Record<ConditionField, string> = {
  extension: "Extension", filename: "File name", type: "File type", size_mb: "Size (MB)", created: "Created", modified: "Modified",
};

function defaultValue(field: ConditionField, op: Op): Condition["value"] {
  if (field === "size_mb") return 100;
  if (field === "type") return "document";
  if (field === "created" || field === "modified") return new Date().toISOString().slice(0, 10);
  return op === "in" ? [] : "";
}

function newRule(kind: RuleKind): Rule {
  const field: ConditionField = kind === "filename" ? "filename" : "extension";
  const op: Op = kind === "filename" ? "contains" : "is";
  return {
    id: `rule_${Date.now().toString(36)}`,
    name: "",
    enabled: true,
    kind,
    conditions: [{ field, op, value: "" }],
    destination: { type: "category", category: "documents" },
  };
}

function shownCondition(c: Condition): string {
  if (c.field === "type" && typeof c.value === "string") {
    return `file type is ${categoryLabel(TYPE_FROM_VALUE[c.value] ?? "other")}`;
  }
  return conditionText(c);
}

export function Rules() {
  const cfg = useConfig();
  const [editing, setEditing] = useState<Rule | null>(null);
  if (!cfg) return null;
  const rules = cfg.rules;

  async function save(next: Rule[]) {
    try {
      await api.saveRules(next);
      await refreshConfig();
      return true;
    } catch (e) {
      toastError(e);
      return false;
    }
  }

  async function exportFile() {
    try {
      const path = await saveDialog({ title: "Export rules", defaultPath: "rules.json", filters: [{ name: "Rules", extensions: ["json"] }] });
      if (!path) return;
      const n = await api.exportRules(path);
      toast(`Exported ${n} rule${n === 1 ? "" : "s"}`);
    } catch (e) {
      toastError(e);
    }
  }

  async function importFile() {
    try {
      const path = await open({ title: "Import rules", multiple: false, filters: [{ name: "Rules", extensions: ["json"] }] });
      if (typeof path !== "string") return;
      const replace =
        rules.length > 0 &&
        (await ask("Replace your current rules with the imported ones?\n\nChoose No to add them after your current rules.", {
          title: "Import rules",
          kind: "info",
          okLabel: "Replace",
          cancelLabel: "No, add them",
        }));
      const n = await api.importRules(path, replace);
      await refreshConfig();
      toast(`Imported ${n} rule${n === 1 ? "" : "s"}`);
    } catch (e) {
      toastError(e);
    }
  }

  function move(rule: Rule, dir: -1 | 1) {
    const same = rules.filter((r) => r.kind === rule.kind);
    const pos = same.indexOf(rule);
    const other = same[pos + dir];
    if (!other) return;
    const next = [...rules];
    const a = next.indexOf(rule);
    const b = next.indexOf(other);
    [next[a], next[b]] = [next[b], next[a]];
    void save(next);
  }

  return (
    <div className="page">
      <PageHeader title="Rules">
        <button onClick={importFile}>Import…</button>
        <button onClick={exportFile} disabled={rules.length === 0}>Export…</button>
        <button className="primary" onClick={() => setEditing(newRule("custom"))}>Add rule</button>
      </PageHeader>
      <p className="muted">Rules are checked in this order; the first match wins. Files no rule matches go to their file-type folder.</p>
      {TIERS.map((t) => {
        const list = rules.filter((r) => r.kind === t.kind);
        return (
          <section key={t.kind} className="card">
            <div className="card-head">
              <div>
                <h2>{t.title}</h2>
                <span className="muted small">{t.help}</span>
              </div>
              <button className="link" onClick={() => setEditing(newRule(t.kind))}>+ Add</button>
            </div>
            {list.length === 0 && <div className="muted small">None yet.</div>}
            {list.map((r, i) => (
              <div key={r.id} className={r.enabled ? "rule-row" : "rule-row dim"}>
                <input
                  type="checkbox"
                  title={r.enabled ? "Enabled" : "Disabled"}
                  checked={r.enabled}
                  onChange={(e) => void save(rules.map((x) => (x.id === r.id ? { ...x, enabled: e.target.checked } : x)))}
                />
                <div className="rule-text">
                  <strong>{r.name}</strong>
                  <span className="muted small">
                    IF {r.conditions.map(shownCondition).join(" AND ")} → {destinationText(r.destination)}
                  </span>
                </div>
                <button className="icon" disabled={i === 0} onClick={() => move(r, -1)} title="Move up">↑</button>
                <button className="icon" disabled={i === list.length - 1} onClick={() => move(r, 1)} title="Move down">↓</button>
                <button className="link" onClick={() => setEditing(r)}>Edit</button>
                <button className="link danger" onClick={() => void save(rules.filter((x) => x.id !== r.id))}>Delete</button>
              </div>
            ))}
          </section>
        );
      })}
      <section className="card">
        <h2>4. Default: by file type</h2>
        <span className="muted small">{CATEGORIES.map((c) => c.label).join(" · ")}</span>
      </section>

      {editing && (
        <RuleEditor
          rule={editing}
          onCancel={() => setEditing(null)}
          onSave={async (r) => {
            const exists = rules.some((x) => x.id === r.id);
            const ok = await save(exists ? rules.map((x) => (x.id === r.id ? r : x)) : [...rules, r]);
            if (ok) {
              setEditing(null);
              toast(`Rule "${r.name}" saved`);
            }
          }}
        />
      )}
    </div>
  );
}

function RuleEditor({ rule, onSave, onCancel }: { rule: Rule; onSave: (r: Rule) => void; onCancel: () => void }) {
  const [r, setR] = useState<Rule>(rule);
  const [error, setError] = useState("");

  function setCond(i: number, c: Condition) {
    setR({ ...r, conditions: r.conditions.map((x, j) => (j === i ? c : x)) });
  }

  function submit() {
    if (!r.name.trim()) return setError("Give the rule a name.");
    if (r.conditions.length === 0) return setError("Add at least one condition.");
    for (const c of r.conditions) {
      const empty = Array.isArray(c.value) ? c.value.length === 0 : c.value === "";
      if (empty) return setError(`Fill in the value for "${FIELD_TEXT[c.field]}".`);
    }
    if (r.destination.type === "custom" && !r.destination.path.trim()) return setError("Choose a destination folder.");
    onSave({ ...r, name: r.name.trim() });
  }

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true">
      <div className="modal">
        <h2>{rule.name ? "Edit rule" : "New rule"}</h2>
        <label className="field">
          <span>Name</span>
          <input value={r.name} onChange={(e) => setR({ ...r, name: e.target.value })} placeholder="e.g. Invoices" autoFocus />
        </label>
        <label className="field">
          <span>Priority group</span>
          <select value={r.kind} onChange={(e) => setR({ ...r, kind: e.target.value as RuleKind })}>
            <option value="filename">File name rule (checked first)</option>
            <option value="custom">Custom rule</option>
            <option value="extension">Extension rule</option>
          </select>
        </label>

        <h3>If</h3>
        {r.conditions.map((c, i) => (
          <ConditionRow
            key={i}
            c={c}
            onChange={(nc) => setCond(i, nc)}
            onRemove={r.conditions.length > 1 ? () => setR({ ...r, conditions: r.conditions.filter((_, j) => j !== i) }) : undefined}
          />
        ))}
        <button
          className="link"
          onClick={() => setR({ ...r, conditions: [...r.conditions, { field: "extension", op: "is", value: "" }] })}
        >
          + And another condition
        </button>

        <h3>Then move to</h3>
        <div className="row">
          <select
            value={r.destination.type === "category" ? r.destination.category : "__custom"}
            onChange={(e) =>
              setR({
                ...r,
                destination:
                  e.target.value === "__custom"
                    ? { type: "custom", path: "" }
                    : { type: "category", category: e.target.value as Category },
              })
            }
          >
            {CATEGORIES.map((c) => <option key={c.value} value={c.value}>{c.label}</option>)}
            <option value="__custom">A folder of my choice…</option>
          </select>
          {r.destination.type === "custom" && (
            <>
              <input
                value={r.destination.path}
                placeholder="Documents/Invoices (inside the Super Folder) or a full path"
                onChange={(e) => setR({ ...r, destination: { type: "custom", path: e.target.value } })}
              />
              <button
                onClick={async () => {
                  const p = await pickFolder("Destination folder");
                  if (p) setR({ ...r, destination: { type: "custom", path: p } });
                }}
              >
                Browse…
              </button>
            </>
          )}
        </div>

        {error && <p className="error-text">{error}</p>}
        <div className="modal-actions">
          <button onClick={onCancel}>Cancel</button>
          <button className="primary" onClick={submit}>Save rule</button>
        </div>
      </div>
    </div>
  );
}

function ConditionRow({ c, onChange, onRemove }: { c: Condition; onChange: (c: Condition) => void; onRemove?: () => void }) {
  const isDate = c.field === "created" || c.field === "modified";
  return (
    <div className="row cond-row">
      <select
        value={c.field}
        onChange={(e) => {
          const field = e.target.value as ConditionField;
          const op = OPS[field][0];
          onChange({ field, op, value: defaultValue(field, op) });
        }}
      >
        {(Object.keys(FIELD_TEXT) as ConditionField[]).map((f) => <option key={f} value={f}>{FIELD_TEXT[f]}</option>)}
      </select>
      <select
        value={c.op}
        onChange={(e) => {
          const op = e.target.value as Op;
          onChange({ ...c, op, value: op === "in" || c.op === "in" ? defaultValue(c.field, op) : c.value });
        }}
      >
        {OPS[c.field].map((o) => <option key={o} value={o}>{(isDate && DATE_OP_TEXT[o]) || OP_TEXT[o]}</option>)}
      </select>
      {c.field === "type" ? (
        <select value={String(c.value)} onChange={(e) => onChange({ ...c, value: e.target.value })}>
          {CATEGORIES.map((cat) => <option key={cat.value} value={TYPE_VALUE[cat.value]}>{cat.label}</option>)}
        </select>
      ) : c.field === "size_mb" ? (
        <input
          type="number"
          min={0}
          step="any"
          value={typeof c.value === "number" ? c.value : 0}
          onChange={(e) => onChange({ ...c, value: Number(e.target.value) })}
        />
      ) : isDate ? (
        <input type="date" value={String(c.value)} onChange={(e) => onChange({ ...c, value: e.target.value })} />
      ) : c.op === "in" ? (
        <ListInput
          initial={Array.isArray(c.value) ? c.value.join(", ") : ""}
          onChange={(raw) =>
            onChange({ ...c, value: raw.split(",").map((v) => cleanText(c.field, v).trim()).filter(Boolean) })
          }
        />
      ) : (
        <input
          placeholder={c.field === "extension" ? "pdf" : "invoice"}
          value={String(c.value)}
          onChange={(e) => onChange({ ...c, value: cleanText(c.field, e.target.value) })}
        />
      )}
      {onRemove && <button className="icon" title="Remove condition" onClick={onRemove}>✕</button>}
    </div>
  );
}

/** Comma list kept as raw text while typing, so "jpg, " doesn't lose its comma. */
function ListInput({ initial, onChange }: { initial: string; onChange: (raw: string) => void }) {
  const [raw, setRaw] = useState(initial);
  return (
    <input
      placeholder="jpg, png, gif"
      value={raw}
      onChange={(e) => {
        setRaw(e.target.value);
        onChange(e.target.value);
      }}
    />
  );
}

/** Extensions are stored lowercase without the dot; names lowercase (matching is case-insensitive). */
function cleanText(field: ConditionField, v: string): string {
  const t = v.trimStart().toLowerCase();
  return field === "extension" ? t.replace(/^\*?\./, "").trim() : t;
}

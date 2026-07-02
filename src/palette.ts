// Command palette (Ctrl+Shift+P): search-and-run over the command registry.

import { allCommands, type CommandItem } from "./commands";

let overlay: HTMLElement | null = null;
let input: HTMLInputElement | null = null;
let listEl: HTMLElement | null = null;
let filtered: CommandItem[] = [];
let selected = 0;
let onCloseFocus: (() => void) | null = null;

export function isPaletteOpen(): boolean {
  return overlay !== null;
}

export function openPalette(restoreFocus?: () => void): void {
  if (overlay) return;
  onCloseFocus = restoreFocus ?? null;

  overlay = document.createElement("div");
  overlay.id = "palette-overlay";
  const box = document.createElement("div");
  box.id = "palette";
  input = document.createElement("input");
  input.id = "palette-input";
  input.placeholder = "Type a command…";
  input.spellcheck = false;
  listEl = document.createElement("div");
  listEl.id = "palette-list";
  box.append(input, listEl);
  overlay.appendChild(box);
  document.body.appendChild(overlay);

  overlay.addEventListener("mousedown", (e) => {
    if (e.target === overlay) closePalette();
  });
  input.addEventListener("input", () => {
    selected = 0;
    refresh();
  });
  input.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      closePalette();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      selected = Math.min(selected + 1, filtered.length - 1);
      refresh();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selected = Math.max(selected - 1, 0);
      refresh();
    } else if (e.key === "Enter") {
      e.preventDefault();
      runSelected();
    }
  });

  selected = 0;
  refresh();
  input.focus();
}

export function closePalette(): void {
  overlay?.remove();
  overlay = null;
  input = null;
  listEl = null;
  filtered = [];
  const restore = onCloseFocus;
  onCloseFocus = null;
  restore?.();
}

function matchScore(title: string, query: string): number | null {
  if (!query) return 0;
  const t = title.toLowerCase();
  let score = 0;
  for (const token of query.toLowerCase().split(/\s+/).filter(Boolean)) {
    const idx = t.indexOf(token);
    if (idx < 0) return null;
    score += idx;
  }
  return score;
}

function refresh(): void {
  if (!listEl || !input) return;
  const q = input.value.trim();
  // No result cap: the list scrolls (40vh). A cap silently hides commands
  // registered late (themes vanished once earlier providers grew past 12).
  filtered = allCommands()
    .map((c) => ({ c, s: matchScore(c.title, q) }))
    .filter((x): x is { c: CommandItem; s: number } => x.s !== null)
    .sort((a, b) => a.s - b.s)
    .map((x) => x.c);
  selected = Math.min(selected, Math.max(0, filtered.length - 1));

  listEl.replaceChildren();
  filtered.forEach((cmd, i) => {
    const row = document.createElement("div");
    row.className = "palette-item" + (i === selected ? " selected" : "");
    const title = document.createElement("span");
    title.textContent = cmd.title;
    row.appendChild(title);
    if (cmd.hint) {
      const hint = document.createElement("span");
      hint.className = "palette-hint";
      hint.textContent = cmd.hint;
      row.appendChild(hint);
    }
    row.addEventListener("mousedown", (e) => {
      e.preventDefault();
      selected = i;
      runSelected();
    });
    listEl!.appendChild(row);
    if (i === selected) row.scrollIntoView({ block: "nearest" });
  });
  if (filtered.length === 0) {
    const empty = document.createElement("div");
    empty.className = "palette-empty";
    empty.textContent = "No matching commands";
    listEl.appendChild(empty);
  }
}

function runSelected(): void {
  const cmd = filtered[selected];
  closePalette();
  if (cmd) void cmd.run();
}

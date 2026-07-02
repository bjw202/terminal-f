// Collapsible left sidebar for workspaces (Phase A, cmux-inspired).
// Collapsed: slim icon rail with activity dots. Expanded: names, badges,
// rename (dblclick), color label (right-click), drag reorder, width drag.

import { WORKSPACE_LABEL_COLORS } from "./themes";
import type { WorkspaceActivity, WorkspaceMeta } from "./types";

export interface SidebarProps {
  metas: WorkspaceMeta[];
  activity: Map<string, WorkspaceActivity>;
  currentId: string | null;
  collapsed: boolean;
  width: number;
  onSwitch(id: string): void;
  onCreate(): void;
  onDelete(id: string): void;
  onRename(id: string, name: string): void;
  onReorder(ids: string[]): void;
  onSetColor(id: string, color: string | null): void;
  onToggle(): void;
  onWidthChange(width: number): void;
  onOpenPalette(): void;
}

let dragSourceId: string | null = null;
/** True while a rename input or context menu is open; the caller should skip
 * re-renders (activity polling) to avoid destroying them. */
export let sidebarBusy = false;

export function renderSidebar(el: HTMLElement, props: SidebarProps): void {
  el.classList.toggle("collapsed", props.collapsed);
  el.style.width = props.collapsed ? "" : `${props.width}px`;
  el.replaceChildren();

  // header: toggle + title
  const header = document.createElement("div");
  header.className = "sb-header";
  const toggle = document.createElement("button");
  toggle.className = "sb-toggle";
  toggle.title = "Toggle sidebar (Ctrl+Shift+B)";
  toggle.textContent = "☰";
  toggle.addEventListener("click", props.onToggle);
  header.appendChild(toggle);
  if (!props.collapsed) {
    const title = document.createElement("span");
    title.className = "sb-title";
    title.textContent = "terminal-f";
    header.appendChild(title);
  }
  el.appendChild(header);

  // workspace list
  const list = document.createElement("div");
  list.className = "sb-list";
  props.metas.forEach((meta, index) => {
    list.appendChild(buildItem(meta, index, props));
  });
  el.appendChild(list);

  // footer: new workspace + palette
  const footer = document.createElement("div");
  footer.className = "sb-footer";
  const add = document.createElement("button");
  add.className = "sb-btn";
  add.title = "New workspace";
  add.textContent = props.collapsed ? "+" : "+ New workspace";
  add.addEventListener("click", props.onCreate);
  footer.appendChild(add);
  const palette = document.createElement("button");
  palette.className = "sb-btn";
  palette.title = "Command palette (Ctrl+Shift+P)";
  palette.textContent = props.collapsed ? "⌘" : "Commands…";
  palette.addEventListener("click", props.onOpenPalette);
  footer.appendChild(palette);
  el.appendChild(footer);

  // width drag handle (expanded only)
  if (!props.collapsed) {
    const handle = document.createElement("div");
    handle.className = "sb-resize";
    handle.addEventListener("mousedown", (down) => {
      down.preventDefault();
      const startX = down.clientX;
      const startW = props.width;
      const onMove = (move: MouseEvent) => {
        const w = Math.min(320, Math.max(160, startW + move.clientX - startX));
        el.style.width = `${w}px`;
      };
      const onUp = (up: MouseEvent) => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        const w = Math.min(320, Math.max(160, startW + up.clientX - startX));
        props.onWidthChange(w);
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    });
    el.appendChild(handle);
  }
}

function initials(name: string): string {
  return name.trim().slice(0, 2) || "?";
}

function buildItem(meta: WorkspaceMeta, index: number, props: SidebarProps): HTMLElement {
  const item = document.createElement("div");
  item.className = "sb-item" + (meta.id === props.currentId ? " active" : "");
  item.draggable = true;
  item.dataset.wsId = meta.id;
  item.title = props.collapsed ? `${meta.name} (Ctrl+${index + 1})` : `Ctrl+${index + 1}`;

  const act = props.activity.get(meta.id);
  const showUnseen = !!act?.unseenOutput && meta.id !== props.currentId;
  const showExited = (act?.exitedPanes ?? 0) > 0;

  // avatar: initials on color label
  const avatar = document.createElement("span");
  avatar.className = "sb-avatar";
  avatar.textContent = initials(meta.name);
  if (meta.color) {
    avatar.style.background = meta.color;
    avatar.classList.add("colored");
  }
  item.appendChild(avatar);

  if (!props.collapsed) {
    const name = document.createElement("span");
    name.className = "sb-name";
    name.textContent = meta.name;
    name.addEventListener("dblclick", (e) => {
      e.stopPropagation();
      beginRename(name, meta, props);
    });
    item.appendChild(name);
  }

  const badges = document.createElement("span");
  badges.className = "sb-badges";
  if (showUnseen) {
    const dot = document.createElement("span");
    dot.className = "sb-dot unseen";
    dot.title = "New output while inactive";
    badges.appendChild(dot);
  }
  if (showExited) {
    const dot = document.createElement("span");
    dot.className = "sb-dot exited";
    dot.title = `${act!.exitedPanes} pane(s) exited`;
    badges.appendChild(dot);
  }
  item.appendChild(badges);

  if (!props.collapsed) {
    const close = document.createElement("button");
    close.className = "sb-close";
    close.textContent = "×";
    close.title = "Delete workspace (terminates its sessions)";
    close.addEventListener("click", (e) => {
      e.stopPropagation();
      props.onDelete(meta.id);
    });
    item.appendChild(close);
  }

  item.addEventListener("click", () => {
    if (meta.id !== props.currentId) props.onSwitch(meta.id);
  });
  item.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    openColorMenu(e.clientX, e.clientY, meta, props);
  });

  // drag reorder
  item.addEventListener("dragstart", (e) => {
    dragSourceId = meta.id;
    e.dataTransfer?.setData("text/plain", meta.id);
    item.classList.add("dragging");
  });
  item.addEventListener("dragend", () => {
    dragSourceId = null;
    item.classList.remove("dragging");
    document.querySelectorAll(".sb-item.drop-before, .sb-item.drop-after").forEach((n) => {
      n.classList.remove("drop-before", "drop-after");
    });
  });
  item.addEventListener("dragover", (e) => {
    if (!dragSourceId || dragSourceId === meta.id) return;
    e.preventDefault();
    const rect = item.getBoundingClientRect();
    const before = e.clientY < rect.top + rect.height / 2;
    item.classList.toggle("drop-before", before);
    item.classList.toggle("drop-after", !before);
  });
  item.addEventListener("dragleave", () => {
    item.classList.remove("drop-before", "drop-after");
  });
  item.addEventListener("drop", (e) => {
    e.preventDefault();
    const source = dragSourceId;
    item.classList.remove("drop-before", "drop-after");
    if (!source || source === meta.id) return;
    const rect = item.getBoundingClientRect();
    const before = e.clientY < rect.top + rect.height / 2;
    const ids = props.metas.map((m) => m.id).filter((id) => id !== source);
    let at = ids.indexOf(meta.id);
    if (!before) at += 1;
    ids.splice(at, 0, source);
    props.onReorder(ids);
  });

  return item;
}

function beginRename(nameEl: HTMLElement, meta: WorkspaceMeta, props: SidebarProps): void {
  sidebarBusy = true;
  const input = document.createElement("input");
  input.className = "sb-rename";
  input.value = meta.name;
  nameEl.replaceWith(input);
  input.focus();
  input.select();
  let done = false;
  const finish = (commit: boolean) => {
    if (done) return;
    done = true;
    sidebarBusy = false;
    const value = input.value.trim();
    if (commit && value && value !== meta.name) {
      props.onRename(meta.id, value);
    } else {
      input.replaceWith(nameEl);
    }
  };
  input.addEventListener("blur", () => finish(true));
  input.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter") finish(true);
    if (e.key === "Escape") finish(false);
  });
}

function openColorMenu(x: number, y: number, meta: WorkspaceMeta, props: SidebarProps): void {
  document.getElementById("sb-colormenu")?.remove();
  sidebarBusy = true;
  const menu = document.createElement("div");
  menu.id = "sb-colormenu";
  menu.style.left = `${x}px`;
  menu.style.top = `${y}px`;

  const label = document.createElement("div");
  label.className = "sb-colormenu-label";
  label.textContent = "Color label";
  menu.appendChild(label);

  const row = document.createElement("div");
  row.className = "sb-colormenu-row";
  for (const color of WORKSPACE_LABEL_COLORS) {
    const swatch = document.createElement("button");
    swatch.className = "sb-swatch";
    swatch.style.background = color;
    if (meta.color === color) swatch.classList.add("selected");
    swatch.addEventListener("click", () => {
      close();
      props.onSetColor(meta.id, color);
    });
    row.appendChild(swatch);
  }
  const none = document.createElement("button");
  none.className = "sb-swatch none";
  none.title = "No color";
  none.textContent = "∅";
  none.addEventListener("click", () => {
    close();
    props.onSetColor(meta.id, null);
  });
  row.appendChild(none);
  menu.appendChild(row);
  document.body.appendChild(menu);

  const close = () => {
    menu.remove();
    sidebarBusy = false;
    window.removeEventListener("mousedown", onAway, true);
  };
  const onAway = (e: MouseEvent) => {
    if (!menu.contains(e.target as Node)) close();
  };
  window.addEventListener("mousedown", onAway, true);
}

// Minimal modal dialogs (M2.0): text prompt with optional checkbox, and a
// read-only list view (audit log). Native window.prompt is avoided (WebView2
// dialog behavior varies and blocks the event loop).

interface PromptOpts {
  title: string;
  placeholder?: string;
  initial?: string;
  multiline?: boolean;
  checkboxLabel?: string;
  checkboxInitial?: boolean;
  okLabel?: string;
}

export interface PromptResult {
  value: string;
  checked: boolean;
}

export function promptModal(opts: PromptOpts): Promise<PromptResult | null> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "modal-overlay";
    const box = document.createElement("div");
    box.className = "modal-box";

    const title = document.createElement("div");
    title.className = "modal-title";
    title.textContent = opts.title;
    box.appendChild(title);

    const input = document.createElement(opts.multiline ? "textarea" : "input") as
      | HTMLInputElement
      | HTMLTextAreaElement;
    input.className = "modal-input";
    input.placeholder = opts.placeholder ?? "";
    input.value = opts.initial ?? "";
    input.spellcheck = false;
    if (opts.multiline) (input as HTMLTextAreaElement).rows = 5;
    box.appendChild(input);

    let checkbox: HTMLInputElement | null = null;
    if (opts.checkboxLabel) {
      const row = document.createElement("label");
      row.className = "modal-check";
      checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.checked = opts.checkboxInitial ?? true;
      row.append(checkbox, document.createTextNode(` ${opts.checkboxLabel}`));
      box.appendChild(row);
    }

    const buttons = document.createElement("div");
    buttons.className = "modal-buttons";
    const cancel = document.createElement("button");
    cancel.className = "modal-btn";
    cancel.textContent = "Cancel";
    const ok = document.createElement("button");
    ok.className = "modal-btn primary";
    ok.textContent = opts.okLabel ?? "OK";
    buttons.append(cancel, ok);
    box.appendChild(buttons);

    overlay.appendChild(box);
    document.body.appendChild(overlay);
    input.focus();
    if (!opts.multiline) (input as HTMLInputElement).select();

    const close = (result: PromptResult | null) => {
      overlay.remove();
      resolve(result);
    };
    const submit = () => close({ value: input.value, checked: checkbox?.checked ?? false });

    cancel.addEventListener("click", () => close(null));
    ok.addEventListener("click", submit);
    overlay.addEventListener("mousedown", (e) => {
      if (e.target === overlay) close(null);
    });
    input.addEventListener("keydown", (e: Event) => {
      const ke = e as KeyboardEvent;
      ke.stopPropagation();
      if (ke.key === "Escape") close(null);
      // Enter submits; in multiline mode require Ctrl+Enter
      if (ke.key === "Enter" && (!opts.multiline || ke.ctrlKey)) {
        ke.preventDefault();
        submit();
      }
    });
  });
}

/** Confirm dialog showing a multi-line body (e.g. a snippet + file path) with
 * Cancel/OK. Resolves true only on explicit OK. Used before edits the user must
 * approve (e.g. writing to their $PROFILE). */
export function confirmModal(opts: {
  title: string;
  body: string[];
  okLabel?: string;
}): Promise<boolean> {
  return new Promise((resolve) => {
    const overlay = document.createElement("div");
    overlay.className = "modal-overlay";
    const box = document.createElement("div");
    box.className = "modal-box wide";

    const titleEl = document.createElement("div");
    titleEl.className = "modal-title";
    titleEl.textContent = opts.title;
    box.appendChild(titleEl);

    const list = document.createElement("div");
    list.className = "modal-list";
    for (const row of opts.body) {
      const el = document.createElement("div");
      el.className = "modal-list-row";
      el.textContent = row;
      list.appendChild(el);
    }
    box.appendChild(list);

    const buttons = document.createElement("div");
    buttons.className = "modal-buttons";
    const cancel = document.createElement("button");
    cancel.className = "modal-btn";
    cancel.textContent = "Cancel";
    const ok = document.createElement("button");
    ok.className = "modal-btn primary";
    ok.textContent = opts.okLabel ?? "OK";
    buttons.append(cancel, ok);
    box.appendChild(buttons);

    overlay.appendChild(box);
    document.body.appendChild(overlay);
    ok.focus();

    const close = (result: boolean) => {
      overlay.remove();
      resolve(result);
    };
    cancel.addEventListener("click", () => close(false));
    ok.addEventListener("click", () => close(true));
    overlay.addEventListener("mousedown", (e) => {
      if (e.target === overlay) close(false);
    });
    overlay.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Escape") close(false);
      if (e.key === "Enter") close(true);
    });
  });
}

export function listModal(title: string, rows: string[]): void {
  const overlay = document.createElement("div");
  overlay.className = "modal-overlay";
  const box = document.createElement("div");
  box.className = "modal-box wide";

  const titleEl = document.createElement("div");
  titleEl.className = "modal-title";
  titleEl.textContent = title;
  box.appendChild(titleEl);

  const list = document.createElement("div");
  list.className = "modal-list";
  if (rows.length === 0) {
    const empty = document.createElement("div");
    empty.className = "modal-list-row muted";
    empty.textContent = "(empty)";
    list.appendChild(empty);
  }
  for (const row of rows) {
    const el = document.createElement("div");
    el.className = "modal-list-row";
    el.textContent = row;
    list.appendChild(el);
  }
  box.appendChild(list);

  const buttons = document.createElement("div");
  buttons.className = "modal-buttons";
  const ok = document.createElement("button");
  ok.className = "modal-btn primary";
  ok.textContent = "Close";
  buttons.appendChild(ok);
  box.appendChild(buttons);

  overlay.appendChild(box);
  document.body.appendChild(overlay);
  ok.focus();

  const close = () => overlay.remove();
  ok.addEventListener("click", close);
  overlay.addEventListener("mousedown", (e) => {
    if (e.target === overlay) close();
  });
  overlay.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape" || e.key === "Enter") close();
  });
}

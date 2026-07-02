// Command registry (Phase A). Every user-facing action registers here; the
// command palette (and future menus/macros) are thin UIs over this list.

export interface CommandItem {
  id: string;
  title: string;
  /** Optional hint shown right-aligned (e.g. shortcut). */
  hint?: string;
  run(): void | Promise<void>;
}

type Provider = () => CommandItem[];

const providers: Provider[] = [];

export function registerCommandProvider(provider: Provider): void {
  providers.push(provider);
}

export function allCommands(): CommandItem[] {
  return providers.flatMap((p) => {
    try {
      return p();
    } catch {
      return [];
    }
  });
}

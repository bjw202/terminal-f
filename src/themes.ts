// Theme token system (Phase A).
//
// One token set is the single source for both the app chrome (CSS variables)
// and the terminal (xterm ITheme incl. 16 ANSI colors), so UI and terminal
// colors always match. Selection persists in config.json `ui.theme`.

import type { ITheme } from "@xterm/xterm";

export interface Theme {
  id: string;
  label: string;
  /** CSS variable name (without leading --) -> value */
  ui: Record<string, string>;
  term: ITheme;
}

const mocha: Theme = {
  id: "catppuccin-mocha",
  label: "Catppuccin Mocha (dark)",
  ui: {
    "app-bg": "#11111b",
    "panel-bg": "#181825",
    "panel-border": "#313244",
    fg: "#cdd6f4",
    muted: "#a6adc8",
    faint: "#6c7086",
    accent: "#89b4fa",
    divider: "#313244",
    hover: "#313244",
    active: "#45475a",
    focus: "#89b4fa",
    error: "#f38ba8",
    warn: "#f9e2af",
  },
  term: {
    background: "#11111b",
    foreground: "#cdd6f4",
    cursor: "#f5e0dc",
    selectionBackground: "#585b70",
    black: "#45475a",
    red: "#f38ba8",
    green: "#a6e3a1",
    yellow: "#f9e2af",
    blue: "#89b4fa",
    magenta: "#f5c2e7",
    cyan: "#94e2d5",
    white: "#bac2de",
    brightBlack: "#585b70",
    brightRed: "#f38ba8",
    brightGreen: "#a6e3a1",
    brightYellow: "#f9e2af",
    brightBlue: "#89b4fa",
    brightMagenta: "#f5c2e7",
    brightCyan: "#94e2d5",
    brightWhite: "#a6adc8",
  },
};

const latte: Theme = {
  id: "catppuccin-latte",
  label: "Catppuccin Latte (light)",
  ui: {
    "app-bg": "#eff1f5",
    "panel-bg": "#e6e9ef",
    "panel-border": "#ccd0da",
    fg: "#4c4f69",
    muted: "#6c6f85",
    faint: "#9ca0b0",
    accent: "#1e66f5",
    divider: "#ccd0da",
    hover: "#dce0e8",
    active: "#bcc0cc",
    focus: "#1e66f5",
    error: "#d20f39",
    warn: "#df8e1d",
  },
  term: {
    background: "#eff1f5",
    foreground: "#4c4f69",
    cursor: "#dc8a78",
    selectionBackground: "#acb0be",
    black: "#5c5f77",
    red: "#d20f39",
    green: "#40a02b",
    yellow: "#df8e1d",
    blue: "#1e66f5",
    magenta: "#ea76cb",
    cyan: "#179299",
    white: "#acb0be",
    brightBlack: "#6c6f85",
    brightRed: "#d20f39",
    brightGreen: "#40a02b",
    brightYellow: "#df8e1d",
    brightBlue: "#1e66f5",
    brightMagenta: "#ea76cb",
    brightCyan: "#179299",
    brightWhite: "#bcc0cc",
  },
};

const oneDark: Theme = {
  id: "one-dark",
  label: "One Dark",
  ui: {
    "app-bg": "#21252b",
    "panel-bg": "#282c34",
    "panel-border": "#3e4451",
    fg: "#abb2bf",
    muted: "#828997",
    faint: "#5c6370",
    accent: "#61afef",
    divider: "#3e4451",
    hover: "#2c313a",
    active: "#3e4451",
    focus: "#61afef",
    error: "#e06c75",
    warn: "#e5c07b",
  },
  term: {
    background: "#21252b",
    foreground: "#abb2bf",
    cursor: "#528bff",
    selectionBackground: "#3e4451",
    black: "#282c34",
    red: "#e06c75",
    green: "#98c379",
    yellow: "#e5c07b",
    blue: "#61afef",
    magenta: "#c678dd",
    cyan: "#56b6c2",
    white: "#abb2bf",
    brightBlack: "#5c6370",
    brightRed: "#e06c75",
    brightGreen: "#98c379",
    brightYellow: "#e5c07b",
    brightBlue: "#61afef",
    brightMagenta: "#c678dd",
    brightCyan: "#56b6c2",
    brightWhite: "#ffffff",
  },
};

const solarizedLight: Theme = {
  id: "solarized-light",
  label: "Solarized Light",
  ui: {
    "app-bg": "#fdf6e3",
    "panel-bg": "#eee8d5",
    "panel-border": "#d9d2c2",
    fg: "#657b83",
    muted: "#839496",
    faint: "#93a1a1",
    accent: "#268bd2",
    divider: "#d9d2c2",
    hover: "#e4ddc9",
    active: "#d9d2c2",
    focus: "#268bd2",
    error: "#dc322f",
    warn: "#b58900",
  },
  term: {
    background: "#fdf6e3",
    foreground: "#657b83",
    cursor: "#586e75",
    selectionBackground: "#eee8d5",
    black: "#073642",
    red: "#dc322f",
    green: "#859900",
    yellow: "#b58900",
    blue: "#268bd2",
    magenta: "#d33682",
    cyan: "#2aa198",
    white: "#eee8d5",
    brightBlack: "#002b36",
    brightRed: "#cb4b16",
    brightGreen: "#586e75",
    brightYellow: "#657b83",
    brightBlue: "#839496",
    brightMagenta: "#6c71c4",
    brightCyan: "#93a1a1",
    brightWhite: "#fdf6e3",
  },
};

// Windows Terminal's "Campbell Powershell" scheme — the classic navy PowerShell
// look (#012456 background) many Windows users expect. ANSI palette is the
// official Campbell set; app chrome is derived to sit on the navy background.
const campbellPowershell: Theme = {
  id: "campbell-powershell",
  label: "Campbell PowerShell (navy)",
  ui: {
    "app-bg": "#001b3d",
    "panel-bg": "#012456",
    "panel-border": "#0b3a70",
    fg: "#cccccc",
    muted: "#9db2cc",
    faint: "#5f7ba0",
    accent: "#3b78ff",
    divider: "#0b3a70",
    hover: "#0b3a70",
    active: "#164a86",
    focus: "#3b78ff",
    error: "#e74856",
    warn: "#f9f1a5",
  },
  term: {
    background: "#012456",
    foreground: "#cccccc",
    cursor: "#cccccc",
    selectionBackground: "#264f78",
    black: "#0c0c0c",
    red: "#c50f1f",
    green: "#13a10e",
    yellow: "#c19c00",
    blue: "#0037da",
    magenta: "#881798",
    cyan: "#3a96dd",
    white: "#cccccc",
    brightBlack: "#767676",
    brightRed: "#e74856",
    brightGreen: "#16c60c",
    brightYellow: "#f9f1a5",
    brightBlue: "#3b78ff",
    brightMagenta: "#b4009e",
    brightCyan: "#61d6d6",
    brightWhite: "#f2f2f2",
  },
};

export const THEMES: Theme[] = [mocha, latte, oneDark, solarizedLight, campbellPowershell];
export const DEFAULT_THEME_ID = mocha.id;

let current: Theme = mocha;

export function currentTheme(): Theme {
  return current;
}

export function themeById(id: string | undefined): Theme {
  return THEMES.find((t) => t.id === id) ?? mocha;
}

/** Apply CSS variables; terminal instances are updated by the caller
 * (they live in terms.ts) via `term.options.theme = currentTheme().term`. */
export function setTheme(theme: Theme): void {
  current = theme;
  const rootStyle = document.documentElement.style;
  for (const [key, value] of Object.entries(theme.ui)) {
    rootStyle.setProperty(`--${key}`, value);
  }
}

/** Preset colors offered for workspace color labels. */
export const WORKSPACE_LABEL_COLORS = [
  "#f38ba8",
  "#fab387",
  "#f9e2af",
  "#a6e3a1",
  "#89b4fa",
  "#cba6f7",
];

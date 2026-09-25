---
name: webui-theme
description: Create or edit a WebUI theme (colors, fonts, corner radius) as a CSS file. Load it when the user wants the WebUI to look different or asks for a theme.
compatibility: GQY built-in WebUI theming
---

# WebUI themes

A theme is one CSS file that overrides design tokens. It loads after the built-in styles, so the same selector wins. You only write CSS. Page scripts and markup are off limits.

## Where the file goes

- Run `gqy paths` and take the config directory. Themes live in `<config dir>/webui-themes/`.
- File name: `<name>.css`, where name is lowercase letters, digits and hyphens, at most 40 characters. Example: `sakura.css`.
- Keep it under 256 KB. Larger files are ignored.
- To edit a theme, rewrite its file. To remove one, delete the file (the user can also delete it in the settings).

## File layout

Start with a header comment. The settings page shows these two lines:

```css
/*
 * title: Sakura
 * description: Soft pink accents on a warm dark surface
 */
```

The dark theme is on `:root`. The light theme overrides it on `body[data-theme="linen"]`. Write both, because the user switches between them with the sun and moon button:

```css
:root {
  --md-sys-color-primary: #f2a7c3;
  /* … */
}

body[data-theme="linen"] {
  --md-sys-color-primary: #b0487a;
  /* … */
}
```

## Tokens

Change the color roles first. Surfaces, text, lines and accents are derived from them, so a full palette needs nothing else.

- Accent: `--md-sys-color-primary`, `--md-sys-color-on-primary`, `--md-sys-color-primary-container`, `--md-sys-color-on-primary-container`. Buttons, links, selection and the send button use these.
- Secondary and tertiary: the same four roles with `secondary` and `tertiary`. Tool activity uses secondary. The active-session marker and the stop button use tertiary.
- Error: `--md-sys-color-error`, `--md-sys-color-on-error`, `--md-sys-color-error-container`, `--md-sys-color-on-error-container`.
- Surfaces, from the page background up: `--md-sys-color-surface`, `--md-sys-color-surface-container-lowest`, `-low`, `--md-sys-color-surface-container`, `-high`, `-highest`.
- Text: `--md-sys-color-on-surface` (body text), `--md-sys-color-on-surface-variant` (secondary text).
- Lines: `--md-sys-color-outline`, `--md-sys-color-outline-variant`.
- Online dot: `--md-ext-color-online`.

Shape and type, set once on `:root`:

- `--font-ui`, `--font-mono`, `--serif`: font stacks. Only fonts installed on the user's machine work. Web fonts and `@import` from other sites are blocked by the page's security policy.
- `--radius` (buttons, cards, menus), `--radius-lg` (the message box), `--radius-sm` (tiny marks), `--radius-pill` (switch tracks).

Leave the `--fs-*`, `--z-*` and `--motion*` tokens alone. Font sizes are a per-user setting and layer order keeps menus on top.

## Quality bar

- Text must stay readable. Keep at least 4.5:1 contrast between `on-surface` and each surface, and between each `on-*` color and its fill.
- Keep the dark theme dark and the light theme light. The page tells the browser which one it is.
- Prefer tokens over selectors. Class names change between releases and a theme that styles them breaks silently.

## Turning it on

Tell the user the theme is ready and where to pick it: Settings → Interface → Color scheme. The theme appears there with its title and accent color, and "Default" switches back.

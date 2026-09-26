# GQY WebUI assets

These static assets are embedded into the GQY daemon at build time. Run the local WebUI with:

```sh
cargo run --bin gqy -- web              # --port <PORT> (default 8300), --bind <ADDR> (default 0.0.0.0)
```

The command starts the GQY daemon (the same `gqy` executable re-run in daemon mode) when needed, prints the access URLs, and exits. Use `gqy daemon status` or `gqy daemon stop` to inspect or stop the daemon. WebUI listens on all local network interfaces by default.

Login is always required. On first visit, sign in with the built-in account (username `gqy`, password `GQY520`) and create the admin account; the built-in account stops working after that. Further members join with invite codes the admin generates.

## Layout

- `app.js` is the ES module entry. Modules are layered and may only depend left to right:
  `core/` (request layer, DOM helpers, icons, toast, storage) → `state/` (store, element refs) →
  `widgets/` → `features/` (one directory or file per feature: composer, conversation, console,
  settings, sessions, artifacts, ...) → `app.js`. Features do not import each other unless the edge is
  listed with a reason in `test_scripts/web-deps.json`; `test_scripts/web_dep_check.py` enforces this.
- Files in the `web/` root other than `app.js` (`settings.js`, `settings-extensions.js`, `dashboards.js`,
  `dash-*.js`, ...) are legacy classic scripts that expose `window.GqyXxx`. They are not checked for direction
  yet.
- `css/*.css` are concatenated in file-name order into one `/styles.css`, and `settings-schema/*.js` are
  concatenated into one `/settings-schema.js` wrapped in an IIFE. Edit the parts, never a generated file.
- `vendor/` is served separately (pre-gzipped). `index.html` and `fence-frame.html` have their own handlers.
- Every other file is served at its path relative to `web/` (`web/a/b.js` → `/a/b.js`). `build.rs` scans the
  directory, so a new file needs no Rust change. The rules live in `src/web/asset_rules.rs`, shared by
  `build.rs` and the dev loader below. Reference new files from `index.html`, which gets `?v=<build id>`
  appended.
- The CSP is `script-src 'self'; style-src 'self'`, so inline scripts and styles are blocked.

Assets are compiled in, so a change needs a rebuild. For frontend-only work, run a **debug** build with
`GQY_WEB_DIR=<repo>/web`. The daemon then reads `web/` from disk on every request, so a browser refresh
picks up changes. `/api/health` reports the source in `web_assets`. Release builds ignore the variable
(`src/web/dev_assets.rs` exists only under `debug_assertions`).

The split design is in `docs/design/2026-09-24-webui-split.md`.

## Theming

All colors are built on MD3 system tokens (`--md-sys-color-*`) defined in
`css/00-tokens.css`, with the legacy variable names (`--accent`, `--gold`, …) kept as aliases.
Two built-in themes derive from the GQY logo:

- **晨光 / dawn** (`data-theme="linen"`): warm cream surface, wisteria primary `#7568b0`
- **夜阑 / dusk** (`data-theme="graphite"`, default): evening blue surface, mist-blue primary `#aebde8`

Accent roles: secondary = hair gold (tool activity, model badge), tertiary = ribbon
crimson (active session marker, stop button), plus a semantic online-green
(`--md-ext-color-online`) that never follows wallpaper colors.

Non-color tokens also live in `css/00-tokens.css`: font sizes `--fs-chat/ui/meta/micro`
(15/13/12/11 px), radii `--radius*`, stacking layers `--z-*`, and durations `--motion-fast/--motion/--motion-slow`
(120/160/200 ms). Use them instead of new literals. Breakpoints cannot be tokens in plain CSS; reuse 836px (desktop/mobile split) and 640px where possible.

Stylesheets live in `css/` and are concatenated in file-name order into one `/styles.css` at build time, so file order is cascade order.

`index.html` has one theme `<link>` that loads after `styles.css` and overrides tokens.
Settings → Interface → Color scheme points it at one of three sources, or disables it for the default look:

- `/theme.css`: the matugen wallpaper palette, served from `~/.gqy/config/webui-theme.css`.
- `/webui-themes/<name>.css`: the theme library in `~/.gqy/config/webui-themes/`. GQY writes these with the
  built-in `webui-theme` skill. A header comment gives each one a title and description, and the settings page
  lists and deletes them (`/api/webui-themes`).
- Nothing: the built-in palette.

Themes are CSS only. GQY cannot change page scripts or markup, because anyone who can write frontend JS can run
code in the admin's browser (docs/design/2026-09-25-webui-isolation.md §4).

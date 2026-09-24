# GQY WebUI assets

These static assets are embedded into the GQY daemon at build time. Run the local WebUI with:

```sh
cargo run --bin gqy -- web
```

The command starts the GQY daemon (the same `gqy` executable re-run in daemon mode) when needed, prints the access URLs, and exits. Use `gqy daemon status` or `gqy daemon stop` to inspect or stop the daemon. WebUI listens on all local network interfaces by default. Password protection is optional:

```sh
cargo run --bin gqy -- web -p secret
cargo run --bin gqy -- web -p
cargo run --bin gqy -- web --password-file /path/to/password.txt
```

With a password configured, the WebUI prompts for it and establishes a same-origin session after login.

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

`index.html` loads `/theme.css` after `styles.css`; a matugen-generated override can be
served there to recolor the whole UI from the desktop wallpaper (see `extra/matugen/`).
The 404 when no override exists is harmless. Serving `~/.gqy/config/webui-theme.css`
at `/theme.css` is a pending backend route.

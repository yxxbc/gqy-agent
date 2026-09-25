---
name: gqy-cli
description: Reference for GQY's own command line. Load it before running any `gqy` command in a shell, or when the user asks how to do something with gqy.
compatibility: GQY built-in command reference
---

# GQY command line

`gqy` is the program you run in. The user drives it from a terminal, and you can run it with your command tool. This page lists every real subcommand. Nothing else exists.

## Rules

- Never guess a subcommand. `gqy <word>` with an unknown word is not an error. It is sent to the daemon as a chat message, so you end up talking to yourself until the call times out. `gqy status`, `gqy doctor`, `gqy id` do not exist.
- For the exact flags of one command, run `gqy <command> --help` (or `gqy <command> <sub> --help`).
- Prefer your tools when one covers the job (memory, knowledge base, scripts, skills). Use the CLI for what tools cannot do, such as daemon control or listing sessions and models.
- Interactive commands need a real terminal and hang in your command tool: `gqy config`, `gqy dev`, `gqy oobe`, `gqy stt`, `gqy daemon logs` (follows forever), and bare `gqy`. Tell the user to run them instead.
- Ask the user first before anything destructive or disruptive: `wipe`, `memory reset`, `reset-all-memory`, `reset-memory`, `session delete`, `session clear`, `skills remove`, `pm remove`, `kb remove`, `remove-shell-hook`, `daemon stop`, `daemon restart`. Restarting or stopping the daemon also cuts off the turn you are in.

## Service

- `gqy daemon status` shows the daemon and each interface (WebUI, QQ, voice). This is the status command.
- `gqy daemon start` / `stop` / `restart`. `gqy daemon logs` follows the log (interactive).
- `gqy reload` reloads the config without a restart.
- `gqy web` prints the WebUI address.
- `gqy paths` shows where config, data and cache live.

## Sessions and chat

- `gqy session list [--json]`, `new`, `show`, `rename`, `delete`, `clear`, `pop`, `compact`.
- `gqy session models` shows or sets one session's model override. `default` follows the global pool.
- `gqy session sandbox` shows or binds the session's sandbox root. `--clear` unbinds.
- `gqy ask` sends one message as a one-shot chat. Use it only when a separate turn is really wanted.
- `gqy history`, `reset`, `pop`, `compact`, `models`, `variant` act on the terminal-integration session, not on your current one.

## Models

- `gqy list-models` lists every model with a number. `[*]` marks the current one.
- `gqy models` switches the terminal session's model. `-g` edits the global pool.
- `gqy variant` switches the thinking level of that model.

## Memory and knowledge base

- `gqy memory stats`, `search`, `remember`, `reset`.
- `gqy reset-memory` erases the memory the terminal session produced. `gqy reset-all-memory` erases this persona's whole long-term memory.
- `gqy kb add`, `list`, `search`, `find`, `read`, `remove`, `reindex`, `stats`, `embed`.
- `gqy update-default-kb` refreshes the built-in handbook from the project repository.
- `gqy embed status`, `models`, `reindex` cover semantic vectors for memory, memes and the knowledge base.

## Extensions

- `gqy skills list`, `show`, `enable`, `disable`, `remove`, `stats`, `prune`.
- `gqy pm list`, `search`, `install`, `upgrade`, `remove`, `tap`. Installing downloads outside code, so confirm the package with the user first.
- `gqy tool-call <tool> '<json>'` calls one of this session's tools from a shell. `--list` shows them and `--describe` prints one tool's contract.

## Voice

- `gqy voice status`, `say`, `reset`, `history`. `gqy listen` starts listening without the wake word. `gqy stt` records one sentence (interactive).

## Setup and data

- `gqy init`, `gqy oobe` and `gqy config` are first-run and settings flows (interactive).
- `gqy export` packs the config into an archive. `gqy import <archive>` restores it.
- `gqy wipe` erases all history, memory and group contexts.
- `gqy layout` shows or applies the home directory layout plan.
- `gqy fish-init`, `bash-init`, `zsh-init` install the shell hook. `gqy remove-shell-hook` removes it.
- `gqy github login`, `status`, `logout` manage your own GitHub bot account.
- `gqy stdio` is a protocol mode for host programs, not for you.

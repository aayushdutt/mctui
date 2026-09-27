# mctui — a Minecraft launcher for your terminal

A fast, mouse-friendly Minecraft launcher that runs in a terminal. Written in
Rust on [ratatui](https://ratatui.rs).

## Features

- **Instances**: create vanilla or **Fabric** instances for any Minecraft
  version (snapshots too), then play online with a Microsoft account or
  offline.
- **Launching**: downloads the game, its libraries and assets, and a Java
  that fits (from Adoptium) when none installed does. Shows progress for
  each step, then the game's log while you play. The log can be filtered,
  scrolled back and copied, and play time is recorded.
- **Mods**: search [Modrinth](https://modrinth.com) from the terminal and
  install mods into a Fabric instance. Required dependencies are resolved
  and downloaded for you. New Fabric instances can start with a starter set
  (Fabric API, Mod Menu, Sodium, Lithium).
- **Resource packs**: browse [Vanilla Tweaks](https://vanillatweaks.net),
  pick tweaks, and build and apply the pack to an instance in one step.
- **Accounts**: Microsoft sign-in with a device code. Sessions are renewed in
  the background, so you stay signed in. Offline profiles are supported too.
- **The interface**: keyboard-first and fully clickable. It includes a
  command palette (`:`) and a key reference on every screen (`?`). It follows
  your terminal's light or dark background, and several themes are built in.

## Install

### Homebrew (macOS and Linux)

```bash
brew install aayushdutt/tap/mctui
```

Upgrade later with `brew upgrade mctui`. Homebrew clears macOS's quarantine
flag for you, since the binary is unsigned.

### From a release

Download the archive for your system from
[Releases](https://github.com/aayushdutt/mctui/releases):

| System | Archive |
| --- | --- |
| macOS, Apple silicon | `mctui_Darwin_arm64.tar.gz` |
| macOS, Intel | `mctui_Darwin_x86_64.tar.gz` |
| Linux, x86-64 | `mctui_Linux_x86_64.tar.gz` |
| Linux, ARM64 | `mctui_Linux_arm64.tar.gz` |
| Windows, x86-64 | `mctui_Windows_x86_64.zip` |

Extract it and run `mctui` (or `mctui.exe`) from a terminal. Double-clicking
the binary usually won't show the interface properly.

### With Cargo

```bash
cargo install --locked --git https://github.com/aayushdutt/mctui mctui
```

## Using it

Start `mctui` in a terminal. Everything can be done with the keyboard or the
mouse: click a row to select it, double-click to open it, right-click for its
actions, and scroll anywhere.

| Key | Home |
| --- | --- |
| `↑` `↓` / `j` `k` | Move |
| `↵` | Play (online when signed in) |
| `o` | Play offline |
| `n` | New instance |
| `m` / `p` | Mods / resource packs |
| `e` / `d` / `f` | Edit / delete / open the folder |
| `x` | The instance's actions |
| `/` | Filter |
| `a` / `s` | Accounts / settings |
| `:` | Command palette |
| `?` | Every key, on any screen |
| `q` | Quit |

The same rules hold on every screen:

- `↵` confirms and never removes anything.
- `esc` steps back one layer, and asks before discarding unsaved edits.
- `←` `→` and `tab` switch panes.
- `d` removes, after asking.

While a game runs, `esc` leaves it running and goes home; `↵` on the instance
comes back to its log. `s` on the log stops the game (after asking), and `v`
changes how much of the log shows. `ctrl+z` suspends mctui, except while a
game runs.

## Data

mctui keeps its data under the first of these that applies:

1. `$MCTUI_DATA_DIR`
2. a `data` folder next to the binary (portable mode)
3. `$XDG_DATA_HOME/mctui`
4. `%APPDATA%\mctui` (Windows)
5. `~/.local/share/mctui`

| Path | What |
| --- | --- |
| `instances/` | Each instance: its settings, and its game folder (worlds, mods, packs) |
| `accounts.json` | Accounts (readable only by you) |
| `config.json` | Settings |
| `assets/`, `libraries/` | Game files shared by every instance |
| `cache/` | Version and mod metadata, for playing offline |

Java runtimes that mctui installs live in your config folder (e.g.
`~/Library/Application Support/mctui/java` on macOS), or under `data/java` in
portable mode.

The data layout is the same one the earlier Go version of mctui used, so
existing instances and accounts carry over.

### Themes

`auto` (the default) follows your terminal's background. The others are
`dark`, `light`, `gruvbox`, `catppuccin`, `dracula` and `nord`. Pick one in
Settings (`s`): moving through the list previews each theme.

## Development

You need Rust 1.93 or newer.

```bash
make run        # run from source
make build      # release build: target/release/mctui
make verify     # format check, clippy and every test: what CI runs
make test-live  # tests that talk to Mojang, Fabric, Modrinth and friends
make screenshots  # render every screen to target/screenshots/*.png
```

The workspace has two crates:

- `crates/mctui-core` holds everything that isn't the interface: instances,
  accounts, Microsoft sign-in, Mojang and Fabric metadata, Java, downloads,
  launching, Modrinth and Vanilla Tweaks. Its tests run against a local mock
  server, never the real services.
- `crates/mctui` is the terminal interface. Screen tests drive the whole app
  with keys and clicks and compare the frames to snapshots
  ([insta](https://insta.rs)).

[DESIGN.md](DESIGN.md) is the design system: colours, layout, components, and
how keys behave. New screens follow it.

Set `MCTUI_LOG=debug` to log to `mctui.log` in the data folder.

### Releasing

Set `version` in `Cargo.toml`, commit, then tag it `v<version>` and push the
tag. The release workflow builds every platform, publishes the release, and
updates the Homebrew cask (when the `HOMEBREW_TAP_GITHUB_TOKEN` secret is set).

## License

MIT

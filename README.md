# mctui — Minecraft TUI Launcher

A fast, terminal-based Minecraft launcher written in Go.

## Features

- **Instances** — Create vanilla or **Fabric** instances, pick Minecraft versions, and launch online (Microsoft account) or offline.
- **Fabric** — Loader resolution merges Mojang metadata with Fabric’s profile (`meta.fabricmc.net`), caches merged profiles, and keeps the classpath consistent when you change game version or loader.
- **Modrinth (Fabric)** — From the home screen, open an in-terminal **mod browser**: search Modrinth, install Fabric mods into the instance, remove jars, and see what was installed via mctui. Optional **starter bundle** (Fabric API, Mod Menu, Sodium, Lithium) when creating a Fabric instance.
- **Microsoft authentication** — Device code flow; **online launch** checks that your Minecraft session is still valid. The home screen shows session status; you can still **play offline** when supported.
- **Launch screen** — Progress while downloading and starting the game; press **`v`** to cycle **game log verbosity** (errors / warnings / all). The choice is saved in config (`launchLogVerbosity`).
- **TUI** — Keyboard-first workflow, mouse wheel where it helps, and a clear layout across home, wizard, launch, and mods.

## Install from releases (fastest)

You do **not** need Go installed to run a prebuilt binary.

1. Open **[Releases](https://github.com/aayushdutt/mctui/releases)** on GitHub.
2. Pick the latest release and download the **asset for your OS** under _Assets_:
   - **Windows (64-bit):** `.zip` containing `mctui.exe`
   - **macOS Apple Silicon:** `Darwin_arm64` `.tar.gz`
   - **macOS Intel:** `Darwin_x86_64` `.tar.gz`
   - **Linux (64-bit):** `Linux_x86_64` `.tar.gz`
3. Extract the archive to a folder of your choice.
4. Open a **terminal** (Command Prompt, PowerShell, Terminal, etc.), change into that folder (`cd`), and run:
   - **macOS / Linux:** `chmod +x mctui` once, then `./mctui`
   - **Windows:** `.\mctui.exe`
5. Always start mctui from a terminal so the full interface works. Double-clicking the binary often does not show the TUI correctly.

### Install with Go (`go install`)

If you have Go 1.21+, you can install the latest tagged release **without** downloading a release archive (builds on your machine—often simpler on **macOS** where prebuilt binaries may trigger Gatekeeper):

```bash
go install github.com/aayushdutt/mctui@latest
```

Then run `mctui`. If you get “command not found”, add Go’s bin to your `PATH`: `export PATH="$PATH:$(go env GOPATH)/bin"` (one-off), or add that line to `~/.zshrc` or `~/.bashrc` and open a new terminal — then run `mctui` again.

## Build from source

```bash
# Run directly
go run .

# Or build and run
make build
./mctui
```

See the `Makefile` for `test`, `lint`, cross-builds (`build-all`), and dev helpers.

## Project structure

```
mctui/
├── main.go
├── internal/
│   ├── app/          # Root app model, navigation
│   ├── ui/           # Screens (home, wizard, launch, mods, auth, …)
│   ├── core/         # Instances, accounts
│   ├── launch/       # Download, Java, game process, log verbosity
│   ├── loader/       # Version resolution (vanilla + Fabric merge)
│   ├── mods/         # Modrinth search/install, catalog, starter mods
│   ├── api/          # Mojang, Modrinth, Microsoft / Minecraft auth
│   └── config/       # Paths and settings
├── Makefile
└── BEST_PRACTICES.md
```

## Keybindings (home)

| Key                | Action                          |
| ------------------ | ------------------------------- |
| `↑` `↓` or `j` `k` | Move selection                  |
| `Enter` or `l`     | Launch (online if signed in)    |
| `o`                | Play offline                    |
| `n`                | New instance                    |
| `m`                | Mods browser (Fabric instances) |
| `s`                | Settings (placeholder)          |
| `a`                | Accounts                        |
| `f`                | Open instance folder            |
| `d`                | Delete instance                 |
| `/`                | Filter instances                |
| `q`                | Quit                            |

On the **launch** screen, **`v`** cycles log verbosity. On the **mods** screen, use **`Tab`** to move between installed list, search, and results; **`Esc`** returns home.

## Data and configuration

Data lives under `~/.local/share/mctui` (Linux/macOS) or `%APPDATA%\mctui` (Windows).

| Location                               | Purpose                                                                     |
| -------------------------------------- | --------------------------------------------------------------------------- |
| `instances/`                           | Per-instance configs and worlds                                             |
| `java/`                                | Downloaded Java runtimes (shared)                                           |
| `accounts.json`                        | Stored accounts                                                             |
| Global cache                           | Shared game assets and libraries                                            |
| `.minecraft/mods/.mctui-modrinth.json` | Per-instance catalog of mods installed via mctui (under each instance path) |

Config (same data directory) can include **`launchLogVerbosity`**: `error` (default), `warn`, or `all`.

## License

MIT

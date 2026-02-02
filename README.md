# mctui - Minecraft TUI Launcher

A fast, terminal-based Minecraft launcher built with Go and Bubbletea.

## Features

- 🚀 **Fast** - Native binary, instant startup
- 🎮 **Instance Management** - Create, configure, and launch Minecraft instances
- 📦 **Modrinth Integration** - Browse and install mods directly  
- 🔐 **Microsoft Auth** - Secure login with device code flow
- 🖥️ **Beautiful TUI** - Modern terminal interface with mouse support

## Quick Start

```bash
# Run directly
go run .

# Or build and run
make build
./mctui
```

## Development

```bash
# Install dependencies
go mod tidy

# Run with hot reload (install air first)
go install github.com/air-verse/air@latest
make dev

# Run tests
make test

# Build for all platforms
make build-all
```

## Project Structure

```
mctui/
├── main.go                 # Entry point
├── internal/
│   ├── app/               # Main Bubbletea application
│   ├── ui/                # TUI views and components
│   ├── core/              # Business logic (instances, versions)
│   ├── api/               # HTTP clients (Mojang, Modrinth, MSA)
│   └── config/            # Configuration management
├── Makefile               # Build commands
└── BEST_PRACTICES.md      # Architecture guide
```

## Keybindings

| Key | Action |
|-----|--------|
| `↑/↓` or `j/k` | Navigate |
| `Enter` or `l` | Launch/Select |
| `n` | New instance |
| `m` | Open mods |
| `s` | Settings |
| `/` | Search |
| `q` | Quit |

## License

MIT

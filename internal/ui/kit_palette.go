package ui

import (
	"strings"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/sahilm/fuzzy"
)

// kit_palette.go is the shared `:` command palette: a centered modal with a
// themed textinput over a fuzzy-filtered list of entries. Every screen embeds a
// [CommandPalette], registers its own verbs (and, on home, instance jumps),
// delegates keys to it while Active, and renders it over its own view. The
// plumbing — overlay flag, key interception before the screen's own switch,
// centered render — is identical everywhere, so it lives here once.

// PaletteKind tags an entry's leading swatch: a quiet verb vs. an accent
// jump-to-target.
type PaletteKind int

const (
	PaletteVerb PaletteKind = iota
	PaletteJump
)

// PaletteItem is one selectable palette row. Run is invoked when the row is
// chosen; the palette closes first, then Run's returned cmd (may be nil) is
// propagated to bubbletea.
type PaletteItem struct {
	Label string
	Kind  PaletteKind
	// Key is the screen shortcut that does the same thing (e.g. "m"), shown
	// right-aligned so the palette teaches the keys. Optional.
	Key string
	Run func() tea.Cmd
}

// CommandPalette is the reusable command-palette overlay. Construct it once per
// screen with [NewCommandPalette]; the provider is re-invoked on every Open so
// the entry list always reflects fresh state (selection, instances). While
// Active, feed keys to Update and render View over the screen.
type CommandPalette struct {
	Active bool

	title       string
	placeholder string
	provider    func() []PaletteItem

	input  textinput.Model
	all    []PaletteItem // snapshot taken at Open
	items  []PaletteItem // fuzzy-filtered view of all
	cursor int
}

// NewCommandPalette builds a palette with a modal title, input placeholder, and
// an entry provider (called on each Open).
func NewCommandPalette(title, placeholder string, provider func() []PaletteItem) CommandPalette {
	return CommandPalette{title: title, placeholder: placeholder, provider: provider}
}

// Open arms the overlay and snapshots the current entries.
func (p *CommandPalette) Open() {
	ti := textinput.New()
	ti.Placeholder = p.placeholder
	ti.CharLimit = 100
	ti.Width = 40
	ThemeTextInput(&ti)
	ti.Focus()
	p.input = ti
	p.Active = true
	p.cursor = 0
	if p.provider != nil {
		p.all = p.provider()
	} else {
		p.all = nil
	}
	p.refilter()
}

// Close tears the overlay down.
func (p *CommandPalette) Close() {
	p.Active = false
	p.input.Blur()
	p.all = nil
	p.items = nil
	p.cursor = 0
}

// refilter re-ranks the snapshot against the current query. An empty query shows
// everything in provider order; the cursor is clamped to the result.
func (p *CommandPalette) refilter() {
	query := strings.TrimSpace(p.input.Value())
	if query == "" {
		p.items = p.all
	} else {
		labels := make([]string, len(p.all))
		for i, c := range p.all {
			labels[i] = c.Label
		}
		matches := fuzzy.Find(query, labels) // sorted by descending score
		filtered := make([]PaletteItem, 0, len(matches))
		for _, mt := range matches {
			filtered = append(filtered, p.all[mt.Index])
		}
		p.items = filtered
	}
	if p.cursor >= len(p.items) {
		p.cursor = max(0, len(p.items)-1)
	}
}

// Update handles one key while the palette owns input. It manages its own Active
// flag (esc and enter close it) and returns the cmd to run, if any.
func (p *CommandPalette) Update(msg tea.KeyMsg) tea.Cmd {
	switch msg.String() {
	case "esc":
		p.Close()
		return nil
	case "up", "ctrl+p":
		if p.cursor > 0 {
			p.cursor--
		}
		return nil
	case "down", "ctrl+n":
		if p.cursor < len(p.items)-1 {
			p.cursor++
		}
		return nil
	case "enter":
		if p.cursor < len(p.items) {
			run := p.items[p.cursor].Run
			p.Close()
			if run != nil {
				return run()
			}
			return nil
		}
		p.Close()
		return nil
	}
	var cmd tea.Cmd
	before := p.input.Value()
	p.input, cmd = p.input.Update(msg)
	if p.input.Value() != before {
		// A new query re-ranks the matches: highlight the best one again, so
		// Enter runs what's on top rather than whatever row the cursor was on.
		p.cursor = 0
	}
	p.refilter()
	return cmd
}

// View renders the palette as a [Card] for a width×height screen (composite it
// over the screen with [Overlay]).
func (p *CommandPalette) View(width, height int) string {
	cw := cardWidth(width, 56)
	inner := cw - 4

	// Cap the visible list so the card can't overrun a short terminal.
	maxRows := 8
	if avail := height - 10; avail < maxRows {
		maxRows = max(1, avail)
	}

	parts := []string{p.input.View(), ""}
	if len(p.items) == 0 {
		parts = append(parts, lipgloss.NewStyle().Foreground(Active.TextMuted).Render("  No matches"))
	} else {
		// Window the list around the cursor so the highlighted row stays visible.
		start := 0
		if p.cursor >= maxRows {
			start = p.cursor - maxRows + 1
		}
		end := min(len(p.items), start+maxRows)
		for i := start; i < end; i++ {
			parts = append(parts, p.row(p.items[i], i == p.cursor, inner))
		}
	}
	parts = append(parts, "", KeyHints(inner,
		KeyHint{"↑↓", "move"},
		KeyHint{"↵", "run"},
		KeyHint{"esc", "close"},
	))

	title := p.title
	if title == "" {
		title = "Command palette"
	}
	return Card(title, strings.Join(parts, "\n"), cw, Active.Primary)
}

// row renders one entry: selection gutter, a dot for jump-to-instance rows, the
// label, and its shortcut key flush right.
func (p *CommandPalette) row(e PaletteItem, selected bool, width int) string {
	tag := "  "
	if e.Kind == PaletteJump {
		tag = lipgloss.NewStyle().Foreground(Active.TextMuted).Render(GlyphDot) + " "
	}
	lbl := lipgloss.NewStyle().Foreground(Active.Text)
	if selected {
		lbl = lbl.Foreground(Active.TextStrong).Bold(true)
	}
	row := rowMarker(selected) + tag + lbl.Render(e.Label)
	if e.Key != "" {
		row = StatusBar(row, lipgloss.NewStyle().Foreground(Active.TextMuted).Render(e.Key), width)
	}
	return row
}

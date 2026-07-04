package ui

import (
	"regexp"
	"strings"
	"testing"

	"github.com/charmbracelet/bubbles/list"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/muesli/termenv"
)

// Filtering a list of styled-title rows must not leak the bodies of the rows'
// own SGR sequences onto the screen. bubbles highlights matches with
// lipgloss.StyleRunes, which is not ANSI-aware; before styledTitleDelegate it
// tore the embedded truecolor codes apart, leaving garbage like
// "38;2;241;250;140m" in the rendered output. This reproduces only when color
// is actually emitted, so force a truecolor profile.
func TestRPFilterNoAnsiLeak(t *testing.T) {
	prof := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(prof) })

	m := rpTreeModel(t)
	m.enterSearch() // flatten every pack into the filterable list

	// Drive the bubbles list filter directly: "/", type a query, then pump the
	// async filter command so matches are actually computed. Typing only queues
	// a filterItems Cmd (wrapped in a BatchMsg alongside a cursor blink);
	// discarding it leaves the list unfiltered. Feed only the FilterMatchesMsg
	// back — running the blink cmd would sleep and loop.
	applyFilter := func(cmd tea.Cmd) {
		if cmd == nil {
			return
		}
		msgs := []tea.Msg{cmd()}
		if batch, ok := msgs[0].(tea.BatchMsg); ok {
			msgs = nil
			for _, c := range batch {
				if c != nil {
					msgs = append(msgs, c())
				}
			}
		}
		for _, msg := range msgs {
			if _, ok := msg.(list.FilterMatchesMsg); ok {
				m.packs, _ = m.packs.Update(msg)
			}
		}
	}
	m.packs, _ = m.packs.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune("/")})
	var cmd tea.Cmd
	for _, r := range "glass" {
		m.packs, cmd = m.packs.Update(tea.KeyMsg{Type: tea.KeyRunes, Runes: []rune{r}})
		applyFilter(cmd)
	}

	if got := m.packs.FilterValue(); got != "glass" {
		t.Fatalf("filter value = %q, want %q", got, "glass")
	}
	if n := len(m.packs.VisibleItems()); n == 0 || n == 6 {
		t.Fatalf("filter did not narrow the list: %d of 6 visible", n)
	}
	if m.packs.MatchesForItem(0) == nil {
		t.Fatal("no matched runes computed; highlight path not exercised")
	}

	out := m.packs.View()
	stripped := ansi.Strip(out)
	if !strings.Contains(stripped, "Glass") {
		t.Fatalf("filter on %q matched no Glass pack; rendered:\n%s", "glass", stripped)
	}

	// Real escape sequences are removed by ansi.Strip; an orphaned SGR body (ESC
	// byte torn off by StyleRunes) survives as literal text like ";120;113;108m".
	// Detect the generic torn-SGR shape so the check is theme-color independent.
	if loc := sgrBodyLeak.FindString(stripped); loc != "" {
		t.Errorf("filtered list leaked torn SGR body %q:\n%s", loc, stripped)
	}
}

// sgrBodyLeak matches the body of a truncated SGR sequence (digits joined by
// ';' ending in 'm') that survives ansi.Strip only when its ESC prefix was torn
// off — i.e. the corruption this fix prevents.
var sgrBodyLeak = regexp.MustCompile(`\d+;\d+m`)

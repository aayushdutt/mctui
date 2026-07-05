package ui

import (
	"strings"
	"testing"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/muesli/termenv"
)

// Compositing keeps the backdrop context visible but dimmed behind the overlay
// box (spec M4.1).
func TestComposite(t *testing.T) {
	restoreDark(t)
	// Force a color profile so the faint dimming actually emits SGR (the test
	// binary is not a TTY, which otherwise downgrades to no-color).
	prev := lipgloss.ColorProfile()
	lipgloss.SetColorProfile(termenv.TrueColor)
	t.Cleanup(func() { lipgloss.SetColorProfile(prev) })

	const w, h = 60, 16

	// A recognizable background and a centered overlay box.
	background := strings.Repeat("INSTANCE-ALPHA INSTANCE-BETA INSTANCE-GAMMA\n", h)
	box := Panel("Delete instance?", "Delete \"Survival World\"?", 34, Active.Error)
	overlay := lipgloss.Place(w, h, lipgloss.Center, lipgloss.Center, box)

	out := Composite(background, overlay, w, h)
	plain := ansi.Strip(out)

	// Backdrop content survives (context not erased) …
	if !strings.Contains(plain, "INSTANCE-ALPHA") {
		t.Fatal("composite should keep the backdrop content visible")
	}
	// … and the overlay box is spliced over it.
	if !strings.Contains(plain, "Delete instance?") {
		t.Fatal("composite should show the overlay box")
	}

	// The backdrop is dimmed: the TextFaint SGR must appear in the output.
	probe := lipgloss.NewStyle().Foreground(Active.TextFaint).Render("Z")
	faintSGR := probe[:strings.Index(probe, "Z")]
	if faintSGR == "" || !strings.Contains(out, faintSGR) {
		t.Fatal("backdrop should be recolored to the faint tone")
	}

	// The result stays within the frame.
	for _, line := range strings.Split(out, "\n") {
		if lipgloss.Width(line) > w {
			t.Fatalf("composite line exceeds width: %d > %d", lipgloss.Width(line), w)
		}
	}
	if got := len(strings.Split(out, "\n")); got != h {
		t.Fatalf("composite height = %d, want %d", got, h)
	}

	// Wide runes (CJK/emoji) in the backdrop straddling the box edge must not
	// desync the cell math — every composited row stays exactly `width` cells.
	wideBg := strings.Repeat("生存世界 サバイバル 🎮 world\n", h)
	wideOut := Composite(wideBg, overlay, w, h)
	for i, line := range strings.Split(wideOut, "\n") {
		if got := lipgloss.Width(line); got != w {
			t.Fatalf("wide-rune composite row %d width = %d, want exactly %d", i, got, w)
		}
	}
}

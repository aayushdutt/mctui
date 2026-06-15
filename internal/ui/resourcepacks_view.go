package ui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// View implements tea.Model. Full-screen view: renders WITHOUT outer padding by
// itself — it adds a Padding(1,2) block at the end exactly like mods_view.go (the
// root app then wraps the whole thing with AppShellStyle).
func (m *ResourcePacksModel) View() string {
	// Unsupported instance version: no published catalog. Centered notice.
	if !m.catalogSupported {
		return m.unsupportedView()
	}

	if m.rpDialog == rpDialogConfirmClear {
		return ConfirmDialog{
			Title:    "Clear your pack?",
			Message:  fmt.Sprintf("Remove all %d tweak%s from this instance's pack?", m.sel.Count(), plural(m.sel.Count())),
			Warning:  "This empties the cart (does not delete an already-applied pack).",
			Confirm:  "Clear",
			Cancel:   "Keep",
			Kind:     ConfirmDanger,
			FocusYes: false,
		}.Render(m.width, m.height)
	}

	header := m.headerBlock()
	status := m.statusLine()

	var layout string
	if m.compactLayout {
		layout = lipgloss.JoinVertical(lipgloss.Left,
			header,
			status,
			m.categoriesPane(),
			"",
			m.packsPane(),
			"",
			m.cartPane(),
		)
	} else {
		row := lipgloss.JoinHorizontal(lipgloss.Top,
			m.categoriesPane(),
			strings.Repeat(" ", 2),
			m.packsPane(),
			strings.Repeat(" ", 2),
			m.cartPane(),
		)
		layout = lipgloss.JoinVertical(lipgloss.Left, header, status, "", row)
	}

	helpItems := m.footerHelpItems()
	contentInnerW := max(24, m.width-8)
	help := lipgloss.JoinVertical(lipgloss.Left,
		Rule(min(contentInnerW, 64)),
		lipgloss.NewStyle().MarginTop(1).Render(rpRenderHelp(helpItems, m.width-6)))

	return lipgloss.Place(m.width, m.height, lipgloss.Left, lipgloss.Top,
		lipgloss.NewStyle().Padding(1, 2).Render(lipgloss.JoinVertical(lipgloss.Left, layout, "", help)))
}

func (m *ResourcePacksModel) unsupportedView() string {
	brand := lipgloss.NewStyle().Bold(true).Foreground(Active.Primary).Render("Resource Packs")
	divider := lipgloss.NewStyle().Foreground(Active.BorderSubtle).
		Render(strings.Repeat("─", min(46, max(24, m.width-8))))
	msg := "Vanilla Tweaks has no catalog for Minecraft " + m.inst.Version + "."
	body := lipgloss.JoinVertical(
		lipgloss.Center,
		brand,
		lipgloss.NewStyle().Foreground(Active.TextSubtle).Render("Vanilla Tweaks"),
		"",
		divider,
		"",
		lipgloss.NewStyle().Foreground(Active.Text).Render(msg),
		lipgloss.NewStyle().Foreground(Active.TextMuted).Render("Try an instance on a supported version (e.g. 1.21, 1.20)."),
		"",
		KeyHints(40, KeyHint{"esc", "back"}),
	)
	return lipgloss.Place(m.width, m.height, lipgloss.Center, lipgloss.Center, body)
}

// footerHelpItems returns the key hints shown at the bottom of the screen.
func (m *ResourcePacksModel) footerHelpItems() []KeyHint {
	return rpHelpItemsPick(m.height, m.width)
}

// headerBlock renders the screen header (title + which catalog version is used,
// cart size, and applied/dirty state).
func (m *ResourcePacksModel) headerBlock() string {
	contentInnerW := max(24, m.width-8)

	// Cart summary + state, set flush-right against the title row when there's room.
	n := m.sel.Count()
	summary := lipgloss.NewStyle().Foreground(Active.SuccessAccent).
		Render(fmt.Sprintf("%d tweak%s in your pack", n, plural(n)))
	state := m.dirtyStateLabel()
	rightLine := summary
	if state != "" {
		rightLine += lipgloss.NewStyle().Foreground(Active.BorderSubtle).Render("  ·  ") + state
	}

	top := StatusBar(
		lipgloss.NewStyle().Bold(true).Foreground(Active.Primary).Render("Resource Packs"),
		rightLine,
		contentInnerW,
	)
	subtitle := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(m.instanceContextLine())

	return lipgloss.JoinVertical(lipgloss.Left, top, subtitle, Rule(contentInnerW))
}

// dirtyStateLabel describes whether the cart has unapplied changes.
func (m *ResourcePacksModel) dirtyStateLabel() string {
	if m.sel.Count() == 0 {
		return lipgloss.NewStyle().Foreground(Active.TextMuted).Render("empty")
	}
	if m.sel.Dirty() {
		return lipgloss.NewStyle().Foreground(Active.Warning).Render("● unbuilt changes")
	}
	return lipgloss.NewStyle().Foreground(Active.Success).Render("✓ applied")
}

// statusLine renders the single transient status/notice row above the panes.
func (m *ResourcePacksModel) statusLine() string {
	switch {
	case m.loading:
		return lipgloss.JoinHorizontal(lipgloss.Left,
			m.spinner.View(), " ",
			lipgloss.NewStyle().Foreground(Active.TextSubtle).Render("Loading Vanilla Tweaks catalog…"))
	case m.applying:
		return lipgloss.JoinHorizontal(lipgloss.Left,
			m.spinner.View(), " ",
			lipgloss.NewStyle().Foreground(Active.Secondary).Render("Building your pack…"))
	case m.loadErr != "":
		return lipgloss.NewStyle().Foreground(Active.Error).Render("Catalog error: " + m.loadErr)
	case m.applyErr != "":
		return lipgloss.NewStyle().Foreground(Active.Error).Render(m.applyErr)
	case m.applyOK != "":
		return lipgloss.NewStyle().Foreground(Active.SuccessAccent).Render(GlyphDone + " " + m.applyOK)
	case m.statusMsg != "":
		return lipgloss.NewStyle().Foreground(Active.Warning).Render(m.statusMsg)
	default:
		// No transient state: show the highlighted pack's full(er) description here.
		// This wide row (≈full width vs the narrow pack row) replaces the dropped
		// details modal — more text, always visible, no keypress.
		return m.highlightedDetailLine()
	}
}

// highlightedPack returns the pack highlighted in the cart pane (when focused)
// or otherwise in the packs pane.
func (m *ResourcePacksModel) highlightedPack() (RPPackRef, bool) {
	if m.rpFocus == rpPanelCart {
		if it, ok := m.cart.SelectedItem().(rpCartItem); ok {
			return it.pack, true
		}
	}
	if it, ok := m.packs.SelectedItem().(rpPackItem); ok {
		return it.pack, true
	}
	return RPPackRef{}, false
}

// highlightedDetailLine renders the highlighted pack's name + full description
// (and a video affordance) into the wide status row, truncated to one line.
func (m *ResourcePacksModel) highlightedDetailLine() string {
	ref, ok := m.highlightedPack()
	if !ok {
		return ""
	}
	p := ref.Pack
	seg := lipgloss.NewStyle().Foreground(Active.TextSubtle).Render(displayOf(p))
	if p.Experiment {
		seg += lipgloss.NewStyle().Foreground(Active.WarningSoft).Render("  experimental")
	}
	if desc := stripHTML(p.Description); desc != "" {
		seg += lipgloss.NewStyle().Foreground(Active.TextMuted).Render("  —  " + desc)
	}
	if p.Video != "" {
		seg += lipgloss.NewStyle().Foreground(Active.TextFaint).Render("   ·  [w] video")
	}
	return ansi.Truncate(seg, max(20, m.width-8), "…")
}

// categoriesPane renders the left category list pane.
func (m *ResourcePacksModel) categoriesPane() string {
	title, accent := "Categories", Active.Border
	if m.rpFocus == rpPanelCategories {
		title, accent = GlyphPointer+" Categories", Active.Primary
	}
	w := m.categoriesListW
	if w < 4 {
		w = 16
	}
	return Panel(title, m.categories.View(), w+4, accent)
}

// packsPane renders the middle pack list pane (checkbox rows). The title shows
// the breadcrumb to the selected category, e.g. "Aesthetic › More Zombies".
func (m *ResourcePacksModel) packsPane() string {
	catName := "Packs"
	cat, ok := m.selectedCategory()
	if ok {
		catName = cat.crumb
	}
	if m.searchMode {
		catName = "Search all packs"
	}
	title, accent := catName, Active.BorderFaint
	if m.rpFocus == rpPanelPacks {
		title, accent = GlyphPointer+" "+catName, Active.Success
	}
	w := m.packsListW
	if w < 4 {
		w = 24
	}

	var inner string
	switch {
	case ok && len(cat.category.Packs) == 0 && cat.isContainer:
		// A pure container has no packs of its own — guide the user to drill in
		// rather than showing an empty list (fixes the GUI-style dead end).
		inner = m.containerHint(cat, w)
	default:
		inner = m.packs.View()
		// Surface a category-level warning if present. Truncate to a single line so
		// it occupies exactly one row (the rpCategoryWarningReserve the layout
		// budgets); a wrapping warning would push the pane past its height and clip
		// the footer.
		if warn := m.selectedCategoryWarning(); warn != "" {
			text := ansi.Truncate(GlyphWarn+" "+warn, max(1, w), "…")
			line := lipgloss.NewStyle().Foreground(Active.Warning).Render(text)
			inner = lipgloss.JoinVertical(lipgloss.Left, line, inner)
		}
	}
	return Panel(title, inner, w+4, accent)
}

// containerHint renders the pack-pane body for a pure container (only
// subcategories, no own packs), listing the subcategories and how to open them.
func (m *ResourcePacksModel) containerHint(cat rpCategoryItem, w int) string {
	names := make([]string, 0, len(cat.category.Categories))
	for _, sub := range cat.category.Categories {
		names = append(names, sub.Category)
	}
	n := len(names)
	lead := fmt.Sprintf("Press → to open %d set%s", n, plural(n))
	if cat.expanded {
		lead = "Pick a subcategory below ↓"
	}
	listLine := ansi.Truncate(strings.Join(names, " · "), max(1, w), "…")
	return lipgloss.JoinVertical(lipgloss.Left,
		lipgloss.NewStyle().Foreground(Active.TextMuted).Render(lead),
		"",
		lipgloss.NewStyle().Foreground(Active.TextFaint).Render(listLine),
	)
}

// cartPane renders the right cart pane with a Build & Apply call to action.
func (m *ResourcePacksModel) cartPane() string {
	n := m.sel.Count()
	title := fmt.Sprintf("Your pack (%d)", n)
	accent := Active.Border
	if m.rpFocus == rpPanelCart {
		title, accent = GlyphPointer+" "+title, Active.Warning
	}
	w := m.cartListW
	if w < 4 {
		w = 18
	}

	var inner string
	if n == 0 {
		inner = lipgloss.JoinVertical(lipgloss.Left,
			lipgloss.NewStyle().Foreground(Active.TextMuted).Render("No tweaks yet."),
			lipgloss.NewStyle().Foreground(Active.TextFaint).Render("Pick packs in the middle pane."),
		)
	} else {
		cta := lipgloss.NewStyle().Foreground(Active.SuccessAccent).Render("[b] Build & Apply")
		inner = lipgloss.JoinVertical(lipgloss.Left, m.cart.View(), "", cta)
	}
	return Panel(title, inner, w+4, accent)
}

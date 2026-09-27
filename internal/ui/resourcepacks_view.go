package ui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// View implements tea.Model. Full-screen view: renders WITHOUT any outer padding
// of its own — the root app applies the single AppShellStyle inset once
// (app.View), and SetSize already receives the pre-padded contentSize.
func (m *ResourcePacksModel) View() string {
	base := m.baseView()
	switch {
	case m.help:
		return Overlay(base, HelpCard("Resource packs", m.helpGroups(), m.width, m.height), m.width, m.height)
	case m.palette.Active:
		return Overlay(base, m.palette.View(m.width, m.height), m.width, m.height)
	case m.rpDialog == rpDialogConfirmClear:
		return Overlay(base, ConfirmDialog{
			Title:    "Clear your pack?",
			Message:  fmt.Sprintf("Remove all %d tweak%s from this instance's pack?", m.sel.Count(), plural(m.sel.Count())),
			Warning:  "This empties the cart (does not delete an already-applied pack).",
			Confirm:  "Clear",
			Cancel:   "Keep",
			Kind:     ConfirmDanger,
			FocusYes: m.rpDialogFocusYes,
		}.View(m.width), m.width, m.height)
	}
	return base
}

// baseView renders the screen itself (overlays composite over it in View).
func (m *ResourcePacksModel) baseView() string {

	// Unsupported instance version: no published catalog. Centered notice.
	if !m.catalogSupported {
		return m.unsupportedView()
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
	// A blank line where the footer rule used to be keeps the height budget
	// (footerHelpHeight) while matching every other screen's footer.
	help := lipgloss.JoinVertical(lipgloss.Left, "", rpRenderHelp(helpItems, rpHelpBodyMaxWidth(m.width)))

	// Bound to the content width so a stray sub-cell from a bubble list can't push
	// a line past the frame (mirrors home's final MaxWidth guard).
	return lipgloss.NewStyle().MaxWidth(m.width).Render(
		lipgloss.Place(m.width, m.height, lipgloss.Left, lipgloss.Top,
			lipgloss.JoinVertical(lipgloss.Left, layout, "", help)))
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

// footerHelpItems returns the key hints for the current state and pane.
func (m *ResourcePacksModel) footerHelpItems() []KeyHint {
	switch {
	case m.applying:
		return rpFooterItems(rpFooterApplying)
	case m.loadErr != "":
		return rpFooterItems(rpFooterLoadErr)
	case m.rpFocus == rpPanelPacks && m.packs.FilterState() == list.Filtering:
		return rpFooterItems(rpFooterFilter)
	case m.searchMode && m.rpFocus == rpPanelPacks:
		return rpFooterItems(rpFooterSearch)
	case m.rpFocus == rpPanelCategories:
		return rpFooterItems(rpFooterCategories)
	case m.rpFocus == rpPanelCart:
		return rpFooterItems(rpFooterCart)
	default:
		return rpFooterItems(rpFooterPacks)
	}
}

// headerBlock renders the screen header — the shared AppHeader with the cart
// size and applied/dirty state flush right (plus which catalog version is used
// when it differs from the instance's) — and its bottom margin.
func (m *ResourcePacksModel) headerBlock() string {
	n := m.sel.Count()
	dim := lipgloss.NewStyle().Foreground(Active.TextDim)
	right := dim.Render(fmt.Sprintf("%d tweak%s", n, plural(n)))
	if state := m.dirtyStateLabel(); state != "" {
		right += dim.Render("  ·  ") + state
	}
	if m.catalogSupported && m.catalogVersion != "" && m.inst != nil && m.catalogVersion != m.inst.Version {
		right = dim.Render("catalog "+m.catalogVersion+"  ·  ") + right
	}
	name := ""
	if m.inst != nil {
		name = m.inst.Name
	}
	return AppHeader(m.width, right, name, "Resource packs") + "\n"
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
	w := m.categoriesListW
	if w < 4 {
		w = 16
	}
	return Zone("Categories", m.categories.View(), w, m.rpFocus == rpPanelCategories)
}

// rpPaneText renders a line of pane body text at the list rows' 2-cell gutter,
// truncated to the pane.
func rpPaneText(s string, w int) string {
	return "  " + ansi.Truncate(s, max(1, w-2), "…")
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
			line := rpPaneText(lipgloss.NewStyle().Foreground(Active.Warning).Render(GlyphWarn+" "+warn), w)
			inner = lipgloss.JoinVertical(lipgloss.Left, line, inner)
		}
	}
	return Zone(catName, inner, w, m.rpFocus == rpPanelPacks)
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
	return lipgloss.JoinVertical(lipgloss.Left,
		rpPaneText(lipgloss.NewStyle().Foreground(Active.TextMuted).Render(lead), w),
		"",
		rpPaneText(lipgloss.NewStyle().Foreground(Active.TextFaint).Render(strings.Join(names, " · ")), w),
	)
}

// cartPane renders the right cart pane: the tweaks in your pack, the count on
// the title (b build & apply is in the footer).
func (m *ResourcePacksModel) cartPane() string {
	n := m.sel.Count()
	w := m.cartListW
	if w < 4 {
		w = 18
	}

	inner := m.cart.View()
	if n == 0 {
		hint := lipgloss.NewStyle().Width(max(1, w-2))
		inner = lipgloss.NewStyle().PaddingLeft(2).Render(lipgloss.JoinVertical(lipgloss.Left,
			hint.Foreground(Active.TextMuted).Render("No tweaks yet."),
			hint.Foreground(Active.TextFaint).Render("Pick packs in the middle pane."),
		))
	}
	count := lipgloss.NewStyle().Foreground(Active.TextMuted).Render(fmt.Sprint(n))
	return ZoneR("Your pack", count, inner, w, m.rpFocus == rpPanelCart)
}

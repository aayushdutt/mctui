package ui

import (
	"strconv"
	"strings"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/resourcepacks"
	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
)

// Category disclosure triangles and the incompatibility-swap affordance glyph.
const (
	rpGlyphCollapsed = "▸"
	rpGlyphExpanded  = "▾"
	rpGlyphSwap      = "⇄"
)

// lipglossSpinnerStyle is the themed style for the resource-packs spinner.
func lipglossSpinnerStyle() lipgloss.Style {
	return lipgloss.NewStyle().Foreground(Active.Secondary)
}

// rpSplitMinWidth is the width below which the resource-packs screen stacks its
// panes instead of showing them side by side. Mirrors mods' splitMinWidth.
const rpSplitMinWidth = 78

// rpPanel identifies the focused pane on the resource-packs screen.
type rpPanel int

const (
	// rpPanelCategories is the left category/navigation list.
	rpPanelCategories rpPanel = iota
	// rpPanelPacks is the pack list for the selected category.
	rpPanelPacks
	// rpPanelCart is the cart (current selection) pane.
	rpPanelCart
)

// rpDialogKind identifies a modal on the resource-packs screen.
type rpDialogKind int

const (
	rpDialogNone rpDialogKind = iota
	// rpDialogConfirmClear confirms emptying the cart.
	rpDialogConfirmClear
)

// --- async message types ---

// rpCatalogLoadedMsg carries the result of fetching the Vanilla Tweaks catalog.
type rpCatalogLoadedMsg struct {
	catalog *api.ResourcePackCatalog
	version string // catalog version actually used
	err     error
	// seq guards against a stale fetch landing after a newer one (race-clean).
	seq int
}

// rpClearNoticeMsg clears the transient swap/status notice after a short delay.
type rpClearNoticeMsg struct{ token int }

// rpApplyDoneMsg is sent when BuildAndApply finishes or fails.
type rpApplyDoneMsg struct {
	result *resourcepacks.ApplyResult
	err    error
	// seq guards against a stale apply result landing after a cancel/restart,
	// mirroring rpCatalogLoadedMsg.seq.
	seq int
}

// --- list items ---

// rpCategoryItem is a row in the collapsible category tree. A node may be a
// nested subcategory (depth>0) and may be a container (has child categories).
// Rows render single-line (the category list hides descriptions): a disclosure
// triangle for containers, the category name, a right-aligned aggregate pack
// count, and a cart badge when the subtree holds selected packs.
type rpCategoryItem struct {
	category api.RPCategory
	// path is the stable identity key (display names joined, NUL-separated) used
	// to preserve the highlighted row across expand/collapse rebuilds.
	path string
	// crumb is the human breadcrumb to this node, e.g. "Aesthetic › More Zombies".
	crumb string
	depth int
	// isContainer is true when the node has child subcategories.
	isContainer bool
	// expanded reflects whether this container's children are currently shown.
	expanded bool
	// totalPacks is the aggregate pack count in this subtree (self + descendants).
	totalPacks int
	// selectedCount is the aggregate number of subtree packs currently in the cart.
	selectedCount int
	// rowWidth is the category pane content width, for right-aligning the counts.
	rowWidth int
}

func (i rpCategoryItem) Title() string {
	indent := strings.Repeat("  ", i.depth)

	glyph := "  "
	if i.isContainer {
		if i.expanded {
			glyph = lipgloss.NewStyle().Foreground(Active.Primary).Render(rpGlyphExpanded) + " "
		} else {
			glyph = lipgloss.NewStyle().Foreground(Active.TextMuted).Render(rpGlyphCollapsed) + " "
		}
	}
	left := indent + glyph

	// Right cluster: dim aggregate total + an accent cart badge when non-empty.
	right := lipgloss.NewStyle().Foreground(Active.TextFaint).Render(strconv.Itoa(i.totalPacks))
	if i.selectedCount > 0 {
		right += " " + lipgloss.NewStyle().Foreground(Active.SuccessAccent).
			Render(GlyphDot+strconv.Itoa(i.selectedCount))
	}

	// Budget the row against the pane width, leaving room for the delegate gutter.
	inner := i.rowWidth - 3
	if inner < 10 {
		inner = 16
	}
	rightW := lipgloss.Width(right)
	avail := inner - lipgloss.Width(left) - rightW - 1
	if avail < 1 {
		avail = 1
	}
	name := lipgloss.NewStyle().Foreground(Active.Text).Render(ansi.Truncate(i.category.Category, avail, "…"))

	pad := inner - lipgloss.Width(left) - lipgloss.Width(name) - rightW
	if pad < 1 {
		pad = 1
	}
	return left + name + strings.Repeat(" ", pad) + right
}
func (i rpCategoryItem) Description() string { return "" }
func (i rpCategoryItem) FilterValue() string { return i.category.Category }

// rpPackItem is a row in the pack list, scoped to one category (or, in global
// search, drawn from any category — carrying its own RPPackRef).
type rpPackItem struct {
	pack RPPackRef
	// inCart is true when the pack is currently in the cart.
	inCart bool
	// applied is true when the pack is in the last-built (applied) snapshot. The
	// inCart/applied pair distinguishes built vs pending-add vs pending-remove.
	applied bool
	// blocker is the display label of a cart pack this one conflicts with, or ""
	// when the pack can be freely toggled.
	blocker string
	// showCategory prefixes the description with the owning category (global
	// search, where rows span categories).
	showCategory bool
}

func (i rpPackItem) Title() string {
	name := displayOf(i.pack.Pack)
	switch {
	case i.inCart && i.applied:
		// In the built pack and still selected — unchanged.
		box := lipgloss.NewStyle().Foreground(Active.SuccessAccent).Render("[x]")
		return box + " " + lipgloss.NewStyle().Foreground(Active.TextStrong).Render(name)
	case i.inCart:
		// Newly added, not yet built — pending addition.
		box := lipgloss.NewStyle().Foreground(Active.Warning).Render("[+]")
		return box + " " + lipgloss.NewStyle().Foreground(Active.TextStrong).Render(name)
	case i.applied:
		// Was built but removed from the cart — pending removal on next build.
		box := lipgloss.NewStyle().Foreground(Active.TextMuted).Render("[-]")
		return box + " " + lipgloss.NewStyle().Foreground(Active.TextMuted).Strikethrough(true).Render(name)
	case i.blocker != "":
		// Selectable but conflicting — toggling swaps out the conflict. Muted (not
		// faint/disabled) so it doesn't read as inert.
		box := lipgloss.NewStyle().Foreground(Active.WarningSoft).Render("[ ]")
		return box + " " + lipgloss.NewStyle().Foreground(Active.TextMuted).Render(name)
	default:
		box := lipgloss.NewStyle().Foreground(Active.Border).Render("[ ]")
		return box + " " + lipgloss.NewStyle().Foreground(Active.Text).Render(name)
	}
}

func (i rpPackItem) Description() string {
	// A conflict reads as a swap, not a wall: "replaces X" tells the user what
	// selecting this pack will evict from the cart.
	if i.blocker != "" && !i.inCart {
		return lipgloss.NewStyle().Foreground(Active.Warning).
			Render(rpGlyphSwap + " replaces " + i.blocker)
	}
	if i.applied && !i.inCart {
		return lipgloss.NewStyle().Foreground(Active.TextMuted).Render("removed — rebuild to apply")
	}

	var lead string
	if i.showCategory {
		lead = lipgloss.NewStyle().Foreground(Active.Secondary).Render(i.pack.Category)
	}
	desc := stripHTML(i.pack.Pack.Description)
	if i.pack.Pack.Experiment {
		tag := lipgloss.NewStyle().Foreground(Active.WarningSoft).Render("experimental")
		desc = strings.TrimSpace(tag + " " + lipgloss.NewStyle().Foreground(Active.TextFaint).Render(desc))
	} else if desc != "" {
		desc = lipgloss.NewStyle().Foreground(Active.TextMuted).Render(desc)
	}
	switch {
	case lead != "" && desc != "":
		return lead + lipgloss.NewStyle().Foreground(Active.TextFaint).Render(" · ") + desc
	case lead != "":
		return lead
	default:
		return desc
	}
}
func (i rpPackItem) FilterValue() string { return i.pack.Pack.Display + " " + i.pack.Pack.Name }

// rpCartItem is a row in the cart pane: one selected pack, with its owning
// category so it can be removed/toggled.
type rpCartItem struct {
	pack RPPackRef
	// applied marks packs already in the built pack (✓) vs pending additions (+).
	applied bool
}

func (i rpCartItem) Title() string {
	mark := lipgloss.NewStyle().Foreground(Active.Warning).Render("+")
	if i.applied {
		mark = lipgloss.NewStyle().Foreground(Active.Success).Render(GlyphDone)
	}
	return mark + " " + lipgloss.NewStyle().Foreground(Active.Text).Render(displayOf(i.pack.Pack))
}
func (i rpCartItem) Description() string {
	return lipgloss.NewStyle().Foreground(Active.TextMuted).Render(i.pack.Category)
}
func (i rpCartItem) FilterValue() string { return i.pack.Pack.Display }

// RPPackRef pairs a pack with its owning category display name. Carrying the
// category alongside the pack avoids re-deriving it when toggling cart membership.
type RPPackRef struct {
	Category string
	Pack     api.RPPack
}

// ensure list.Item conformance at compile time.
var (
	_ list.Item = rpCategoryItem{}
	_ list.Item = rpPackItem{}
	_ list.Item = rpCartItem{}
)

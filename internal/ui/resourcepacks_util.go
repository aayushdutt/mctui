package ui

import (
	"fmt"

	"github.com/aayushdutt/mctui/internal/api"
)

// Generic, stateless helpers shared across the resource-packs screen files.

// allCategories flattens the (possibly nested) catalog tree into a flat slice.
func allCategories(cat *api.ResourcePackCatalog) []api.RPCategory {
	if cat == nil {
		return nil
	}
	var out []api.RPCategory
	var walk func(c api.RPCategory)
	walk = func(c api.RPCategory) {
		out = append(out, c)
		for _, sub := range c.Categories {
			walk(sub)
		}
	}
	for _, c := range cat.Categories {
		walk(c)
	}
	return out
}

// displayOf returns a pack's display label, falling back to its id.
func displayOf(p api.RPPack) string {
	if p.Display != "" {
		return p.Display
	}
	return p.Name
}

// plural returns "" for 1 and "s" otherwise, for simple count suffixes.
func plural(n int) string {
	if n == 1 {
		return ""
	}
	return "s"
}

// joinAnd renders a human list ("A", "A and B", "A and 2 more").
func joinAnd(items []string) string {
	switch len(items) {
	case 0:
		return ""
	case 1:
		return items[0]
	case 2:
		return items[0] + " and " + items[1]
	default:
		return fmt.Sprintf("%s and %d more", items[0], len(items)-1)
	}
}

package ui

import (
	"html"
	"regexp"
	"strings"
)

// rpTagRe matches HTML tags so pack descriptions (which ship as HTML markup in
// the Vanilla Tweaks catalog) can be rendered as plain, single-line text.
var rpTagRe = regexp.MustCompile(`<[^>]*>`)

// stripHTML turns a Vanilla Tweaks HTML description into a single line of plain
// text: tags removed, entities decoded, and whitespace collapsed. The list
// delegate truncates to the row width, so this only needs to flatten the markup.
func stripHTML(s string) string {
	if s == "" {
		return ""
	}
	s = rpTagRe.ReplaceAllString(s, " ")
	s = html.UnescapeString(s)
	return strings.Join(strings.Fields(s), " ")
}

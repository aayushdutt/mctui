package mods

import "strings"

// SearchIndex picks Modrinth's sort index: downloads for browse (empty query), relevance when searching.
func SearchIndex(query string) string {
	if strings.TrimSpace(query) == "" {
		return "downloads"
	}
	return "relevance"
}

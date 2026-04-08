package mods

import "testing"

func TestSearchIndex(t *testing.T) {
	if g, w := SearchIndex(""), "downloads"; g != w {
		t.Fatalf("empty: got %q want %q", g, w)
	}
	if g, w := SearchIndex("   "), "downloads"; g != w {
		t.Fatalf("whitespace: got %q want %q", g, w)
	}
	if g, w := SearchIndex("sodium"), "relevance"; g != w {
		t.Fatalf("query: got %q want %q", g, w)
	}
}

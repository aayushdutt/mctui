package ui

import (
	"strings"
	"testing"
)

func TestBuildHelpText_SingleLine(t *testing.T) {
	items := []string{"[a] one", "[b] two", "[c] three"}
	result := buildHelpText(items, 100)

	// Should fit on one line
	if strings.Contains(result, "\n") {
		t.Errorf("Expected single line, got: %q", result)
	}

	// Should contain all items with separators
	if !strings.Contains(result, "[a] one") {
		t.Error("Missing first item")
	}
	if !strings.Contains(result, " • ") {
		t.Error("Missing separator")
	}
}

func TestBuildHelpText_MultiLine(t *testing.T) {
	items := []string{"[enter] launch", "[n] new", "[f] folder", "[d] delete", "[q] quit"}
	result := buildHelpText(items, 40) // Force wrapping

	lines := strings.Split(result, "\n")
	if len(lines) < 2 {
		t.Errorf("Expected multiple lines for narrow width, got: %q", result)
	}

	// Each line should not exceed max width
	for _, line := range lines {
		if len(line) > 40 {
			t.Errorf("Line exceeds max width: %q (len=%d)", line, len(line))
		}
	}
}

func TestBuildHelpText_ItemsStayTogether(t *testing.T) {
	items := []string{"[enter] launch game", "[n] new"}
	result := buildHelpText(items, 25)

	// Items should not be split mid-word
	lines := strings.Split(result, "\n")
	for _, line := range lines {
		// Each line should contain complete items (start with [ or be a continuation)
		trimmed := strings.TrimSpace(line)
		if trimmed != "" && !strings.HasPrefix(trimmed, "[") && !strings.Contains(line, " • ") {
			// This is okay if it's a continuation line starting with [
		}
	}

	// All items should appear in output
	if !strings.Contains(result, "[enter] launch game") {
		t.Error("First item was split")
	}
	if !strings.Contains(result, "[n] new") {
		t.Error("Second item was split")
	}
}

func TestBuildHelpText_EmptyItems(t *testing.T) {
	result := buildHelpText([]string{}, 80)
	if result != "" {
		t.Errorf("Expected empty result for empty items, got: %q", result)
	}
}

func TestBuildHelpText_VerySmallWidth(t *testing.T) {
	items := []string{"[a] test", "[b] item"}
	result := buildHelpText(items, 5) // Very small width

	// Should still produce output (each item on own line when width is tiny)
	if result == "" {
		t.Error("Expected non-empty result even with tiny width")
	}
	if !strings.Contains(result, "[a] test") || !strings.Contains(result, "[b] item") {
		t.Error("Items should still appear in output")
	}
}

func TestBuildHelpText_DefaultWidth(t *testing.T) {
	items := []string{"[a] test"}
	result := buildHelpText(items, 0) // Zero width should use default

	if result != "[a] test" {
		t.Errorf("Expected item to appear unchanged, got: %q", result)
	}
}

package ui

import (
	"math/rand"

	"github.com/charmbracelet/lipgloss"
)

// splash.go is Phase 3's one deliberate spend of the personality budget on the
// home screen: a rotating Minecraft-title-screen-style one-liner. Kept short
// (≤40 cells), sentence case, no emoji — rendered faint-yellow italic so it
// reads as a wink, never as chrome.

// splashLines is the curated pool. mctui-flavored, terminal-native, and
// affectionate toward the original Minecraft splashes without copying them.
var splashLines = []string{
	"Now with 100% more terminal!",
	"Also try NetHack!",
	"Ctrl+C safe!",
	"No mouse required!",
	"Runs on your fridge, probably!",
	"So indie it hurts!",
	"Blazingly monospaced!",
	"As seen in a terminal!",
	"Pixels are optional!",
	"It's a-me, a launcher!",
	"Powered by good intentions!",
	"Fabric-tested, vanilla-approved!",
	"Zero telemetry, all vibes!",
	"Press ? for secrets!",
	"Punch some trees!",
	"Diamonds are down there!",
	"Creeper? Aww man.",
	"Respawn anchor sold separately!",
	"Colored by your terminal!",
	"Now Turing complete!",
	"Made with 64-bit love!",
	"Emeralds not included!",
	"Feed the endermen!",
	"May contain traces of Java!",
	"Written between mob spawns!",
	"Loads faster than a nether portal!",
	"Certified cave-safe!",
	"Try the light theme!",
	"gg ez launch!",
	"Keyboard-first, always!",
	"Mine deeper. Type faster.",
}

// appSplash is picked once per process start — a stable choice for the lifetime
// of the launcher (like Minecraft's per-launch splash). math/rand is auto-seeded
// in modern Go, so consecutive launches vary. Empty pool is impossible (literal
// above), but guard anyway.
var appSplash = pickSplash()

func pickSplash() string {
	if len(splashLines) == 0 {
		return ""
	}
	return splashLines[rand.Intn(len(splashLines))]
}

// SplashLine returns the process's chosen splash styled as the Minecraft title
// wink: faint yellow (the Warning family at low emphasis), italic. Returns "" if
// the pool is somehow empty so callers can omit the row entirely.
func SplashLine() string {
	if appSplash == "" {
		return ""
	}
	return lipgloss.NewStyle().
		Foreground(Active.WarningSoft).
		Italic(true).
		Render(appSplash)
}

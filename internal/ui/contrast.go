package ui

import (
	"math"
	"strconv"

	"github.com/charmbracelet/lipgloss"
)

// Foreground colors used on painted backgrounds. Pure white / black so OnColor
// can guarantee WCAG AA (≥4.5:1) against any accent — the worst case, at the
// crossover luminance, is ≈4.58:1. Softer near-values would dip below 4.5 on
// mid-tone accents (e.g. Nord's red).
var (
	onLightText = lipgloss.Color("#FFFFFF") // for dark backgrounds
	onDarkText  = lipgloss.Color("#000000") // for light backgrounds
)

// OnColor returns a foreground (near-white or near-black) that stays readable on
// bg, chosen by WCAG luminance. Use it for text painted on a saturated accent
// background (title bars, status pills) so contrast holds across every theme
// without hand-picking an "on-accent" color per theme. Unknown/ANSI colors fall
// back to light text (assumes a dark background).
func OnColor(bg lipgloss.Color) lipgloss.Color {
	l, ok := luminance(bg)
	if !ok {
		return onLightText
	}
	if contrastBetween(l, 0) >= contrastBetween(l, 1) {
		return onDarkText
	}
	return onLightText
}

// ContrastRatio is the WCAG contrast ratio between two colors (1.0–21.0). The
// second return is false if either color is not a parseable hex.
func ContrastRatio(a, b lipgloss.Color) (float64, bool) {
	la, oka := luminance(a)
	lb, okb := luminance(b)
	if !oka || !okb {
		return 0, false
	}
	return contrastBetween(la, lb), true
}

func contrastBetween(l1, l2 float64) float64 {
	hi, lo := l1, l2
	if lo > hi {
		hi, lo = lo, hi
	}
	return (hi + 0.05) / (lo + 0.05)
}

// luminance returns the WCAG relative luminance of a "#rrggbb" color.
func luminance(c lipgloss.Color) (float64, bool) {
	r, g, b, ok := hexRGB(string(c))
	if !ok {
		return 0, false
	}
	return 0.2126*linearize(r) + 0.7152*linearize(g) + 0.0722*linearize(b), true
}

func linearize(c float64) float64 {
	if c <= 0.03928 {
		return c / 12.92
	}
	return math.Pow((c+0.055)/1.055, 2.4)
}

func hexRGB(s string) (r, g, b float64, ok bool) {
	if len(s) != 7 || s[0] != '#' {
		return 0, 0, 0, false
	}
	v, err := strconv.ParseUint(s[1:], 16, 32)
	if err != nil {
		return 0, 0, 0, false
	}
	return float64((v>>16)&0xff) / 255, float64((v>>8)&0xff) / 255, float64(v&0xff) / 255, true
}

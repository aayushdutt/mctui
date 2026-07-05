package ui

import (
	"errors"
	"testing"
)

// A failed instance load is surfaced as a banner instead of being swallowed
// (spec M1.5).
func TestHome_InstancesLoadedErrorShowsBanner(t *testing.T) {
	restoreDark(t)
	m := NewHomeModel()
	m.SetSize(120, 40)
	_, _ = m.Update(InstancesLoaded{Error: errors.New("disk on fire")})
	if m.transientBanner == "" {
		t.Fatal("a failed InstancesLoaded should surface a banner")
	}
	if m.loading {
		t.Fatal("loading should be cleared even on error so the screen isn't stuck")
	}
}

// The transient banner survives unrelated keypresses (it used to clear on the
// next key); only its tea.Tick expiry — keyed on generation — clears it.
func TestHome_BannerSurvivesKeypressAndExpiresByGen(t *testing.T) {
	restoreDark(t)
	m := newHomeWith(t, fabricInstance(t))
	m.SetTransientBanner("hello")
	gen := m.bannerGen
	if m.transientBanner == "" {
		t.Fatal("banner should be set")
	}

	// An unrelated key must not clear it.
	sendKey(t, m, "j")
	if m.transientBanner == "" {
		t.Fatal("banner should survive an unrelated keypress")
	}

	// A stale expiry (older generation) must not clear a current banner.
	_, _ = m.Update(homeBannerExpireMsg{gen: gen - 1})
	if m.transientBanner == "" {
		t.Fatal("a stale-generation expiry must not clear the current banner")
	}

	// The matching-generation expiry clears it.
	_, _ = m.Update(homeBannerExpireMsg{gen: m.bannerGen})
	if m.transientBanner != "" {
		t.Fatal("the matching-generation expiry should clear the banner")
	}
}

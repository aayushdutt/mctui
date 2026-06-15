package api

import "testing"

// Vanilla Tweaks only accepts a major.minor catalog version (e.g. "1.21") and
// rejects full patch versions ("1.21.4" returns {"status":"error"}). Instances
// carry a full version, so NormalizeVTVersion MUST collapse it to the nearest
// published major.minor before any catalog fetch or build POST. This guards that
// contract.
func TestNormalizeVTVersion(t *testing.T) {
	cases := []struct {
		in       string
		wantVer  string
		wantOK   bool
		describe string
	}{
		{"1.21.4", "1.21", true, "patch version collapses to published major.minor"},
		{"1.21", "1.21", true, "exact published version passes through"},
		{"1.20.1", "1.20", true, "patch on an older published line"},
		{"1.22.0", "1.21", true, "newer-than-published pins to latest available"},
		{"1.8", "1.8", true, "oldest published line"},
		{"1.7", "1.7", false, "older than every published catalog -> unsupported"},
		{"24w14a", "", false, "snapshot is not a numeric version -> unsupported"},
		{"", "", false, "empty -> unsupported"},
	}
	for _, c := range cases {
		gotVer, gotOK := NormalizeVTVersion(c.in)
		if gotVer != c.wantVer || gotOK != c.wantOK {
			t.Errorf("NormalizeVTVersion(%q) = (%q, %v), want (%q, %v) — %s",
				c.in, gotVer, gotOK, c.wantVer, c.wantOK, c.describe)
		}
	}
}

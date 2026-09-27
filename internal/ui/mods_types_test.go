package ui

import (
	"strings"
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/charmbracelet/x/ansi"
)

func TestModDisplayName(t *testing.T) {
	cases := []struct{ file, name, version string }{
		{"sodium-fabric-0.9.1+mc26.2.jar", "Sodium", "0.9.1"},
		{"fabric-api-0.154.2+26.2.jar", "Fabric API", "0.154.2"},
		{"ImmediatelyFast-Fabric-1.17.1+26.3.jar", "ImmediatelyFast", "1.17.1"},
		{"lithium-fabric-mc1.21.4-0.14.3.jar", "Lithium", "0.14.3"},
		{"modmenu-17.0.0.jar", "Modmenu", "17.0.0"},
		{"cloth-config-v15.0.140.jar", "Cloth Config", "15.0.140"},
		{"sodium.jar", "Sodium", ""},
		{"fabric.jar", "Fabric", ""}, // a lone loader word is still the name
		{"Xaeros_Minimap_24.1.0_Fabric_1.21.jar", "Xaeros Minimap", "24.1.0"},
		{"1.0.jar", "1.0", ""}, // nothing name-like: the file name as is
	}
	for _, c := range cases {
		name, version := modDisplayName(c.file)
		if name != c.name || version != c.version {
			t.Errorf("modDisplayName(%q) = %q, %q; want %q, %q", c.file, name, version, c.name, c.version)
		}
	}
}

func TestModResultDescription(t *testing.T) {
	restoreDark(t)
	hit := api.SearchHit{
		Title: "Sodium", Downloads: 146_600_000,
		DisplayCats: []string{"fabric", "quilt", "optimization", "utility", "decoration"},
		ClientSide:  "required", ServerSide: "unsupported",
	}
	got := ansi.Strip(modListItem{hit: hit, rowNote: modRowInstalled}.Description())
	want := "✓ Installed · 146.6M downloads · client-side · optimization · utility"
	if got != want {
		t.Errorf("description = %q, want %q", got, want)
	}

	hit.ServerSide = "optional"
	if got := ansi.Strip(modListItem{hit: hit}.Description()); strings.Contains(got, "side") || strings.Contains(got, "Installed") {
		t.Errorf("a two-sided, not-installed mod should say neither: %q", got)
	}
}

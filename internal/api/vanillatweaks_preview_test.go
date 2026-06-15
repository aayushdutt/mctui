package api

import "testing"

func TestResourcePackPreviewURL(t *testing.T) {
	const base = "https://vanillatweaks.net"
	// Animated preview when previewExtension is set; version normalized to 1.21.
	gif := ResourcePackPreviewURL("1.21.4", RPPack{Name: "AltBlockDestruction", PreviewExtension: "gif"})
	if want := base + "/assets/resources/previews/resourcepacks/1.21/AltBlockDestruction.gif"; gif != want {
		t.Errorf("preview URL = %q, want %q", gif, want)
	}
	// Static icon (.png) when no previewExtension.
	png := ResourcePackPreviewURL("1.21", RPPack{Name: "BorderlessGlass"})
	if want := base + "/assets/resources/icons/resourcepacks/1.21/BorderlessGlass.png"; png != want {
		t.Errorf("icon URL = %q, want %q", png, want)
	}
}

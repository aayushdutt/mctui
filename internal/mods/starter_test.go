package mods

import (
	"testing"

	"github.com/quasar/mctui/internal/core"
)

func TestStarterFabricModsComplete_nil(t *testing.T) {
	if StarterFabricModsComplete(nil) {
		t.Fatal("nil instance should not be complete")
	}
	if StarterFabricModsComplete(&core.Instance{Path: "/tmp/x"}) {
		// No catalog / jars — not complete
		t.Fatal("instance without mods should not be complete")
	}
}

func TestStarterFabricModsDefault_order(t *testing.T) {
	if len(starterFabricModsDefault) < 2 {
		t.Fatal("expected at least 2 starter mods")
	}
	if starterFabricModsDefault[0].slug != "fabric-api" {
		t.Fatalf("Fabric API must be first, got %q", starterFabricModsDefault[0].slug)
	}
}

package mods

import (
	"testing"

	"github.com/mctui/mctui/internal/api"
)

func TestPickBestFabricVersion(t *testing.T) {
	if PickBestFabricVersion(nil) != nil {
		t.Fatal("expected nil")
	}
	vers := []api.ProjectVersion{
		{VersionNumber: "a", VersionType: "beta"},
		{VersionNumber: "b", VersionType: "release"},
		{VersionNumber: "c", VersionType: "release"},
	}
	got := PickBestFabricVersion(vers)
	if got == nil || got.VersionNumber != "b" {
		t.Fatalf("got %+v", got)
	}
}

func TestPrimaryJar(t *testing.T) {
	if PrimaryJar(nil) != nil {
		t.Fatal("expected nil")
	}
	v := &api.ProjectVersion{
		Files: []api.VersionFile{
			{Filename: "readme.txt", Primary: true},
			{Filename: "mod-1.0.jar", Primary: false},
		},
	}
	if f := PrimaryJar(v); f == nil || f.Filename != "mod-1.0.jar" {
		t.Fatalf("got %+v", f)
	}
	v2 := &api.ProjectVersion{
		Files: []api.VersionFile{
			{Filename: "x.jar", Primary: true},
		},
	}
	if f := PrimaryJar(v2); f == nil || f.Filename != "x.jar" {
		t.Fatalf("got %+v", f)
	}
}

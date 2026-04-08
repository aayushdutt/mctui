package mods

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/mctui/mctui/internal/core"
)

func TestSlugHeuristicMatch(t *testing.T) {
	jars := []string{"sodium-fabric-mc1.21.4-0.6.0.jar"}
	if !SlugHeuristicMatch(jars, "sodium") {
		t.Fatal("expected match")
	}
	if SlugHeuristicMatch(jars, "notthere") {
		t.Fatal("no match")
	}
}

func TestRecordAndProjectRecorded(t *testing.T) {
	tmp := t.TempDir()
	inst := &core.Instance{Path: tmp}
	if err := EnsureModsDir(inst); err != nil {
		t.Fatal(err)
	}
	if err := RecordModrinthInstall(inst, "pid1", "foo", "foo-1.jar"); err != nil {
		t.Fatal(err)
	}
	c, err := LoadModrinthCatalog(inst)
	if err != nil {
		t.Fatal(err)
	}
	jars := []InstalledJar{{Name: "foo-1.jar"}}
	if !ProjectRecorded(c, jars, "pid1") {
		t.Fatal("expected recorded")
	}
	if !IsModrinthProjectInstalled(c, jars, "pid1", "foo") {
		t.Fatal("installed")
	}
}

func TestRecordModrinthInstallRejectsCorruptCatalog(t *testing.T) {
	tmp := t.TempDir()
	inst := &core.Instance{Path: tmp}
	modsDir := filepath.Join(tmp, ".minecraft", "mods")
	if err := os.MkdirAll(modsDir, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(modsDir, catalogFileName), []byte("{"), 0644); err != nil {
		t.Fatal(err)
	}
	if err := RecordModrinthInstall(inst, "pid", "slug", "a.jar"); err == nil {
		t.Fatal("expected error when catalog is unreadable")
	}
}

func TestDropCatalogEntriesForJar(t *testing.T) {
	tmp := t.TempDir()
	inst := &core.Instance{Path: tmp}
	if err := EnsureModsDir(inst); err != nil {
		t.Fatal(err)
	}
	if err := RecordModrinthInstall(inst, "pidA", "alpha", "alpha.jar"); err != nil {
		t.Fatal(err)
	}
	if err := RecordModrinthInstall(inst, "pidB", "beta", "beta.jar"); err != nil {
		t.Fatal(err)
	}
	if err := DropCatalogEntriesForJar(inst, "alpha.jar"); err != nil {
		t.Fatal(err)
	}
	c, err := LoadModrinthCatalog(inst)
	if err != nil {
		t.Fatal(err)
	}
	if len(c.Projects) != 1 || c.Projects[0].ProjectID != "pidB" {
		t.Fatalf("catalog after drop: %#v", c.Projects)
	}
}

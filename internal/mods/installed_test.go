package mods

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/mctui/mctui/internal/core"
)

func TestListInstalledJars(t *testing.T) {
	tmp := t.TempDir()
	mc := filepath.Join(tmp, ".minecraft", "mods")
	if err := os.MkdirAll(mc, 0755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(mc, "b-mod.jar"), []byte("x"), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(mc, "a-mod.jar"), []byte("yy"), 0644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(mc, "readme.txt"), []byte("z"), 0644); err != nil {
		t.Fatal(err)
	}
	inst := &core.Instance{Path: tmp}
	got, err := ListInstalledJars(inst)
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != 2 {
		t.Fatalf("len %d", len(got))
	}
	if got[0].Name != "a-mod.jar" || got[1].Name != "b-mod.jar" {
		t.Fatalf("%v", got)
	}
}

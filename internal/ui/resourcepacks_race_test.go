package ui

import (
	"context"
	"os"
	"sync"
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/resourcepacks"
)

// rpSlowVT is a VanillaTweaksAPI whose build step blocks until released, so the
// apply goroutine is guaranteed to overlap with concurrent View() renders.
type rpSlowVT struct {
	release chan struct{}
}

func (v *rpSlowVT) FetchResourcePackCatalog(ctx context.Context, version string) (*api.ResourcePackCatalog, error) {
	return &api.ResourcePackCatalog{Version: version}, nil
}
func (v *rpSlowVT) BuildResourcePackZip(ctx context.Context, sel map[string][]string, version string) (string, error) {
	<-v.release // hold the goroutine inside BuildAndApply while the UI renders
	return "https://vanillatweaks.net/download/x.zip", nil
}
func (v *rpSlowVT) DownloadResourcePackZip(ctx context.Context, url, dest string) error {
	return os.WriteFile(dest, []byte("PK\x03\x04 fake"), 0644)
}

// TestResourcePacksApplyDoesNotRaceView reproduces the data race the adversarial
// review flagged: BuildAndApply mutates the selection in a goroutine while View
// reads it on the render loop. With the fix (a deep clone handed to the
// goroutine), this must be race-clean under `go test -race`.
func TestResourcePacksApplyDoesNotRaceView(t *testing.T) {
	dir := t.TempDir()
	inst := &core.Instance{Path: dir, Name: "Race", Version: "1.21.4"}
	m := NewResourcePacksModel(inst, nil)
	m.svc = resourcepacks.NewService(&rpSlowVT{release: make(chan struct{})})
	m.catalog = rpTestCatalog()
	m.catalogSupported = true
	m.sel = resourcepacks.NewSelection("1.21")
	m.sel.Toggle(m.catalog, "Aesthetic", "ClearGlass")
	m.sel.Toggle(m.catalog, "Aesthetic", "BorderlessGlass")
	m.rebuildCategoryItems()
	m.rebuildPackItems()
	m.rebuildCartItems()
	m.SetSize(100, 40)

	vt := m.svc.VT.(*rpSlowVT)

	// Kick off the apply: applyCmd starts the goroutine immediately and returns a
	// cmd that blocks on the result channel.
	cmd := m.applyCmd()

	// Hammer View() continuously on several goroutines so a read is almost always
	// in flight. View reads m.sel via Count()/Dirty(). The apply goroutine, once
	// released, runs through the cart mutation (MarkApplied -> cloneSelection writes
	// Applied) and SaveSelection. Pre-fix, those writes overlapped these reads on
	// the SAME *Selection -> race (reproduced under -race). With the fix the
	// goroutine owns a clone, so this is race-clean.
	stop := make(chan struct{})
	var wg sync.WaitGroup
	for g := 0; g < 8; g++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for {
				select {
				case <-stop:
					return
				default:
					_ = m.View()
				}
			}
		}()
	}

	// Let the render loop spin up, then release the build so its post-build mutation
	// window overlaps the in-flight View() reads.
	for i := 0; i < 50; i++ {
		_ = m.View()
	}
	close(vt.release)
	if cmd != nil {
		_ = cmd() // drain the result (blocks until the goroutine mutates + returns)
	}
	// Keep rendering briefly after the mutation completes, then stop.
	for i := 0; i < 50; i++ {
		_ = m.View()
	}
	close(stop)
	wg.Wait()
}

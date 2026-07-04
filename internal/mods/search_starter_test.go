package mods

import (
	"context"
	"net/http"
	"net/http/httptest"
	"reflect"
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
)

func TestSearchFabricMods_optionsMapping(t *testing.T) {
	t.Run("empty query maps to downloads index with fabric facets", func(t *testing.T) {
		f := &fakeModrinth{}
		inst := testInstance(t)
		if _, err := NewService(f).SearchFabricMods(context.Background(), inst, "", 0); err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		if f.lastSearch == nil {
			t.Fatal("Search was not called")
		}
		opts := *f.lastSearch
		if !reflect.DeepEqual(opts.Loaders, []string{"fabric"}) {
			t.Fatalf("Loaders = %v, want [fabric]", opts.Loaders)
		}
		if opts.GameVersion != testMC {
			t.Fatalf("GameVersion = %q, want %q", opts.GameVersion, testMC)
		}
		if opts.ProjectType != "mod" {
			t.Fatalf("ProjectType = %q, want mod", opts.ProjectType)
		}
		if opts.Index != "downloads" {
			t.Fatalf("Index = %q, want downloads (empty query)", opts.Index)
		}
		if opts.Limit != 20 {
			t.Fatalf("Limit = %d, want 20", opts.Limit)
		}
		if opts.Offset != 0 {
			t.Fatalf("Offset = %d, want 0", opts.Offset)
		}
		if opts.Query != "" {
			t.Fatalf("Query = %q, want empty", opts.Query)
		}
	})

	t.Run("non-empty query maps to relevance index, trims, passes offset", func(t *testing.T) {
		f := &fakeModrinth{}
		inst := testInstance(t)
		if _, err := NewService(f).SearchFabricMods(context.Background(), inst, "  sodium  ", 40); err != nil {
			t.Fatalf("unexpected error: %v", err)
		}
		opts := *f.lastSearch
		if opts.Index != "relevance" {
			t.Fatalf("Index = %q, want relevance", opts.Index)
		}
		if opts.Query != "sodium" {
			t.Fatalf("Query = %q, want trimmed 'sodium'", opts.Query)
		}
		if opts.Offset != 40 {
			t.Fatalf("Offset = %d, want 40", opts.Offset)
		}
	})

	t.Run("nil instance errors", func(t *testing.T) {
		_, err := NewService(&fakeModrinth{}).SearchFabricMods(context.Background(), nil, "", 0)
		if err == nil {
			t.Fatal("expected error for nil instance")
		}
	})

	t.Run("empty version errors", func(t *testing.T) {
		inst := testInstance(t)
		inst.Version = ""
		_, err := NewService(&fakeModrinth{}).SearchFabricMods(context.Background(), inst, "", 0)
		if err == nil {
			t.Fatal("expected error for empty version")
		}
	})
}

func TestInstallStarterFabricMods(t *testing.T) {
	mux := http.NewServeMux()
	srv := httptest.NewServer(mux)
	defer srv.Close()

	// starterFabricModsDefault slugs, in order.
	slugs := []string{"fabric-api", "modmenu", "sodium", "lithium"}
	versions := map[string][]api.ProjectVersion{}
	for _, slug := range slugs {
		body := []byte("jar for " + slug)
		file := slug + ".jar"
		url, sha := serveJar(t, mux, srv.URL, file, body)
		versions[slug] = []api.ProjectVersion{
			servedVersion(slug, slug+"-v1", file, url, sha, int64(len(body))),
		}
	}

	f := &fakeModrinth{versionsByProject: versions}
	inst := testInstance(t)

	var calls []string
	progress := func(index, total int, label string) {
		if total != len(slugs) {
			t.Errorf("progress total = %d, want %d", total, len(slugs))
		}
		calls = append(calls, label)
	}

	if err := NewService(f).InstallStarterFabricMods(context.Background(), inst, progress); err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(calls) != len(slugs) {
		t.Fatalf("progress fired %d times, want %d (%v)", len(calls), len(slugs), calls)
	}

	for _, slug := range slugs {
		if !catalogHasProject(t, inst, slug) {
			t.Fatalf("%s should be recorded in catalog", slug)
		}
	}

	if !StarterFabricModsComplete(inst) {
		t.Fatal("StarterFabricModsComplete should be true after install")
	}
}

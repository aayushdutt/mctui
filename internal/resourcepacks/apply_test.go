package resourcepacks

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"testing"

	"github.com/aayushdutt/mctui/internal/api"
	"github.com/aayushdutt/mctui/internal/core"
)

// readResourcePacks loads options.txt and returns the parsed resourcePacks array.
func readResourcePacks(t *testing.T, inst *core.Instance) ([]string, bool) {
	t.Helper()
	data, err := os.ReadFile(OptionsTxtPath(inst))
	if err != nil {
		t.Fatalf("read options.txt: %v", err)
	}
	return parseResourcePacksLine(string(data))
}

func TestEnsureEntryInOptions(t *testing.T) {
	tests := []struct {
		name        string
		in          string
		wantEntries []string // expected resourcePacks array after; nil = line absent
		wantChanged bool
	}{
		{
			name:        "no resourcePacks line appended",
			in:          "version:3700\nfancyGraphics:true\n",
			wantEntries: []string{"vanilla", optionsEntry},
			wantChanged: true,
		},
		{
			name:        "existing packs preserved, entry appended",
			in:          `resourcePacks:["vanilla","file/Foo.zip"]` + "\n",
			wantEntries: []string{"vanilla", "file/Foo.zip", optionsEntry},
			wantChanged: true,
		},
		{
			name:        "already present is idempotent (no change)",
			in:          `resourcePacks:["vanilla","file/VanillaTweaks.zip"]` + "\n",
			wantEntries: []string{"vanilla", optionsEntry},
			wantChanged: false,
		},
		{
			name:        "empty array gets entry added",
			in:          "resourcePacks:[]\n",
			wantEntries: []string{optionsEntry},
			wantChanged: true,
		},
		{
			name:        "malformed value rebuilt to sane default",
			in:          "resourcePacks:[\"vanilla\",\n",
			wantEntries: []string{"vanilla", optionsEntry},
			wantChanged: true,
		},
		{
			name:        "line not newline-terminated still appends",
			in:          "version:3700",
			wantEntries: []string{"vanilla", optionsEntry},
			wantChanged: true,
		},
		{
			name:        "preserves surrounding lines",
			in:          "fancyGraphics:true\nresourcePacks:[\"file/A.zip\"]\nlang:en_us\n",
			wantEntries: []string{"file/A.zip", optionsEntry},
			wantChanged: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			out, changed := ensureEntryInOptions(tt.in)
			if changed != tt.wantChanged {
				t.Errorf("changed = %v, want %v", changed, tt.wantChanged)
			}
			entries, ok := parseResourcePacksLine(out)
			if !ok {
				t.Fatalf("output has no parseable resourcePacks line:\n%s", out)
			}
			if !reflect.DeepEqual(entries, tt.wantEntries) {
				t.Errorf("entries = %v, want %v\noutput:\n%s", entries, tt.wantEntries, out)
			}
			// Idempotency: re-running must not change anything.
			out2, changed2 := ensureEntryInOptions(out)
			if changed2 {
				t.Errorf("second pass changed output (not idempotent):\n%s", out2)
			}
		})
	}
}

func TestEnsureEntryPreservesOtherLines(t *testing.T) {
	in := "fancyGraphics:true\nresourcePacks:[\"file/A.zip\"]\nlang:en_us\n"
	out, _ := ensureEntryInOptions(in)
	for _, want := range []string{"fancyGraphics:true", "lang:en_us"} {
		if !contains(out, want) {
			t.Errorf("output dropped line %q:\n%s", want, out)
		}
	}
}

func contains(haystack, needle string) bool {
	return len(haystack) >= len(needle) && (func() bool {
		for i := 0; i+len(needle) <= len(haystack); i++ {
			if haystack[i:i+len(needle)] == needle {
				return true
			}
		}
		return false
	})()
}

func TestEnableInOptionsTxtAbsentFile(t *testing.T) {
	dir := t.TempDir()
	inst := &core.Instance{Path: dir}

	if IsEnabledInOptionsTxt(inst) {
		t.Fatalf("should not be enabled with no options.txt")
	}

	present, err := EnableInOptionsTxt(inst)
	if err != nil {
		t.Fatalf("EnableInOptionsTxt: %v", err)
	}
	if !present {
		t.Fatalf("expected present=true after enabling")
	}
	if _, err := os.Stat(OptionsTxtPath(inst)); err != nil {
		t.Fatalf("options.txt not created: %v", err)
	}
	if !IsEnabledInOptionsTxt(inst) {
		t.Fatalf("IsEnabledInOptionsTxt should be true after enable")
	}
	entries, ok := readResourcePacks(t, inst)
	if !ok {
		t.Fatalf("resourcePacks line missing")
	}
	if !containsEntry(entries, optionsEntry) {
		t.Fatalf("entry missing: %v", entries)
	}
}

func TestEnableInOptionsTxtExistingFileIdempotent(t *testing.T) {
	dir := t.TempDir()
	inst := &core.Instance{Path: dir}
	mcDir := filepath.Join(dir, ".minecraft")
	if err := os.MkdirAll(mcDir, 0755); err != nil {
		t.Fatal(err)
	}
	original := "version:3700\nresourcePacks:[\"vanilla\",\"file/Existing.zip\"]\nlang:en_us\n"
	if err := os.WriteFile(filepath.Join(mcDir, "options.txt"), []byte(original), 0644); err != nil {
		t.Fatal(err)
	}

	if _, err := EnableInOptionsTxt(inst); err != nil {
		t.Fatalf("EnableInOptionsTxt: %v", err)
	}
	entries, _ := readResourcePacks(t, inst)
	want := []string{"vanilla", "file/Existing.zip", optionsEntry}
	if !reflect.DeepEqual(entries, want) {
		t.Fatalf("entries = %v, want %v", entries, want)
	}

	// Second call: idempotent, file content unchanged.
	before, _ := os.ReadFile(OptionsTxtPath(inst))
	if _, err := EnableInOptionsTxt(inst); err != nil {
		t.Fatalf("EnableInOptionsTxt (2): %v", err)
	}
	after, _ := os.ReadFile(OptionsTxtPath(inst))
	if string(before) != string(after) {
		t.Fatalf("second enable mutated file:\nbefore:%q\nafter:%q", before, after)
	}
}

func containsEntry(entries []string, want string) bool {
	for _, e := range entries {
		if e == want {
			return true
		}
	}
	return false
}

// fakeVT is a stub VanillaTweaksAPI for exercising BuildAndApply end-to-end
// against the real filesystem.
type fakeVT struct {
	buildURL    string
	buildErr    error
	gotSelMap   map[string][]string
	gotVersion  string
	downloadErr error
	zipContent  []byte
}

func (f *fakeVT) FetchResourcePackCatalog(ctx context.Context, version string) (*api.ResourcePackCatalog, error) {
	return &api.ResourcePackCatalog{Version: version}, nil
}

func (f *fakeVT) BuildResourcePackZip(ctx context.Context, selection map[string][]string, version string) (string, error) {
	f.gotSelMap = selection
	f.gotVersion = version
	return f.buildURL, f.buildErr
}

func (f *fakeVT) DownloadResourcePackZip(ctx context.Context, downloadURL, dest string) error {
	if f.downloadErr != nil {
		return f.downloadErr
	}
	content := f.zipContent
	if content == nil {
		content = []byte("PK\x03\x04 fake zip")
	}
	return os.WriteFile(dest, content, 0644)
}

func TestBuildAndApply(t *testing.T) {
	dir := t.TempDir()
	inst := &core.Instance{Path: dir, Version: "1.21.4"}

	vt := &fakeVT{buildURL: "https://vanillatweaks.net/download/abc/x.zip"}
	svc := NewService(vt)

	sel := NewSelection("1.21")
	sel.add("Aesthetic", "ClearGlass")
	sel.add("Aesthetic", "BorderlessGlass")

	res, err := svc.BuildAndApply(context.Background(), inst, sel)
	if err != nil {
		t.Fatalf("BuildAndApply: %v", err)
	}
	if res.PackCount != 2 {
		t.Errorf("PackCount = %d, want 2", res.PackCount)
	}
	if res.Version != "1.21" {
		t.Errorf("Version = %q, want 1.21", res.Version)
	}
	if !res.EnabledInOptions {
		t.Errorf("EnabledInOptions = false, want true")
	}
	if res.DestPath != MergedPackPath(inst) {
		t.Errorf("DestPath = %q, want %q", res.DestPath, MergedPackPath(inst))
	}

	// Zip written.
	if _, err := os.Stat(MergedPackPath(inst)); err != nil {
		t.Fatalf("merged zip not written: %v", err)
	}
	// Build received the projected, sorted selection.
	want := map[string][]string{"Aesthetic": {"BorderlessGlass", "ClearGlass"}}
	if !reflect.DeepEqual(vt.gotSelMap, want) {
		t.Errorf("build selection = %v, want %v", vt.gotSelMap, want)
	}
	// options.txt enabled.
	if !IsEnabledInOptionsTxt(inst) {
		t.Errorf("merged pack not enabled in options.txt")
	}
	// Cart marked applied + persisted (not dirty after reload).
	if sel.Dirty() {
		t.Errorf("selection should not be dirty after apply")
	}
	reloaded, err := LoadSelection(inst)
	if err != nil {
		t.Fatalf("LoadSelection: %v", err)
	}
	if reloaded.Dirty() {
		t.Errorf("reloaded cart should not be dirty (Applied persisted)")
	}
}

func TestBuildAndApplyEmptyCart(t *testing.T) {
	dir := t.TempDir()
	inst := &core.Instance{Path: dir, Version: "1.21.4"}
	svc := NewService(&fakeVT{})
	_, err := svc.BuildAndApply(context.Background(), inst, NewSelection("1.21"))
	if err == nil {
		t.Fatalf("expected error for empty cart")
	}
}

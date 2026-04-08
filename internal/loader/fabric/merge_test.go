package fabric

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/mctui/mctui/internal/core"
)

func TestMergeProfile_mainClassAndLibraries(t *testing.T) {
	parent := &core.VersionDetails{
		ID:        "1.21.1",
		MainClass: "net.minecraft.client.main.Main",
		Libraries: []core.Library{
			{Name: "parent:dep:1"},
		},
	}
	parent.Libraries[0].Downloads = &core.LibraryDownloads{
		Artifact: &core.Artifact{Path: "p/d/1/x.jar", URL: "http://example/x.jar"},
	}

	profile := `{
  "inheritsFrom": "1.21.1",
  "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
  "arguments": { "game": [], "jvm": ["-DFabricMcEmu= "] },
  "libraries": [
    {"name": "net.fabricmc:fabric-loader:0.16.0","url":"https://maven.fabricmc.net/","sha1":"abc","size":3}
  ]
}`

	merged, err := MergeProfile(parent, []byte(profile))
	if err != nil {
		t.Fatal(err)
	}
	if merged.MainClass != "net.fabricmc.loader.impl.launch.knot.KnotClient" {
		t.Fatalf("mainClass: %s", merged.MainClass)
	}
	if len(merged.Libraries) != 2 {
		t.Fatalf("libraries len %d", len(merged.Libraries))
	}
	if merged.Arguments == nil || len(merged.Arguments.JVM) != 1 {
		t.Fatalf("jvm args: %+v", merged.Arguments)
	}
}

func TestMavenArtifactPath(t *testing.T) {
	p, err := mavenArtifactPath("net.fabricmc:fabric-loader:0.16.0")
	if err != nil {
		t.Fatal(err)
	}
	want := "net/fabricmc/fabric-loader/0.16.0/fabric-loader-0.16.0.jar"
	if p != want {
		t.Fatalf("got %q want %q", p, want)
	}
}

func TestNormalizeFabricLibraries_url(t *testing.T) {
	libs, err := normalizeFabricLibraries([]fabricLibraryRaw{
		{Name: "com.google.guava:guava:33.0.0-jre", URL: "https://repo1.maven.org/maven2/", SHA1: "x", Size: 10},
	})
	if err != nil {
		t.Fatal(err)
	}
	if len(libs) != 1 || libs[0].Downloads == nil || libs[0].Downloads.Artifact == nil {
		t.Fatalf("%+v", libs)
	}
	if libs[0].Downloads.Artifact.URL == "" {
		t.Fatal("empty url")
	}
}

func TestDropOW2AsmAllWhenModularPresent_dropsFatJar(t *testing.T) {
	parent := []core.Library{
		{Name: "org.ow2.asm:asm-all:5.2", Downloads: &core.LibraryDownloads{Artifact: &core.Artifact{Path: "o/a/5/asm-all-5.2.jar"}}},
		{Name: "com.mojang:foo:1", Downloads: &core.LibraryDownloads{Artifact: &core.Artifact{Path: "f.jar"}}},
	}
	fabric := []core.Library{
		{Name: "org.ow2.asm:asm:9.6", Downloads: &core.LibraryDownloads{Artifact: &core.Artifact{Path: "o/a/9/asm-9.6.jar"}}},
	}
	merged := dropOW2AsmAllWhenModularPresent(append(append([]core.Library{}, parent...), fabric...))
	if len(merged) != 2 {
		t.Fatalf("want 2 libs after dropping asm-all, got %d", len(merged))
	}
	for _, lib := range merged {
		if strings.Contains(lib.Name, "asm-all") {
			t.Fatalf("asm-all should be removed: %q", lib.Name)
		}
	}
}

func TestMergeLibrariesByMavenIdentity_higherVersionWins(t *testing.T) {
	parent := []core.Library{
		{Name: "org.ow2.asm:asm:9.6"},
		{Name: "com.mojang:foo:1"},
	}
	child := []core.Library{
		{Name: "org.ow2.asm:asm:9.7.1"},
		{Name: "net.fabricmc:fabric-loader:1"},
	}
	out := mergeLibrariesByMavenIdentity(parent, child)
	if len(out) != 3 {
		t.Fatalf("want 3 libs, got %d: %+v", len(out), out)
	}
	var asmName string
	for _, lib := range out {
		if strings.HasPrefix(lib.Name, "org.ow2.asm:asm:") {
			asmName = lib.Name
			break
		}
	}
	if asmName != "org.ow2.asm:asm:9.7.1" {
		t.Fatalf("expected child asm version to win, got %q", asmName)
	}
}

func TestMergeLibrariesByMavenIdentity_classifierIsSeparateSlot(t *testing.T) {
	parent := []core.Library{{Name: "g:a:1:linux"}}
	child := []core.Library{{Name: "g:a:2:osx"}}
	out := mergeLibrariesByMavenIdentity(parent, child)
	if len(out) != 2 {
		t.Fatalf("want 2 (different classifiers), got %d", len(out))
	}
}

func TestMergeLibrariesByMavenIdentity_childDoesNotDowngrade(t *testing.T) {
	parent := []core.Library{{Name: "org.ow2.asm:asm:9.7.1"}}
	child := []core.Library{{Name: "org.ow2.asm:asm:9.6"}}
	out := mergeLibrariesByMavenIdentity(parent, child)
	if len(out) != 1 || out[0].Name != "org.ow2.asm:asm:9.7.1" {
		t.Fatalf("parent version should be kept: %+v", out)
	}
}

func TestDedupeLibrariesByArtifactPath(t *testing.T) {
	libs := []core.Library{
		{Name: "a:1:1", Downloads: &core.LibraryDownloads{Artifact: &core.Artifact{Path: "x/y/z.jar"}}},
		{Name: "b:2:2", Downloads: &core.LibraryDownloads{Artifact: &core.Artifact{Path: "x/y/z.jar"}}},
	}
	out := dedupeLibrariesByArtifactPath(libs)
	if len(out) != 1 || out[0].Name != "a:1:1" {
		t.Fatalf("got %+v", out)
	}
}

func TestDropOW2AsmAllWhenModularPresent_keepsAsmAllIfAlone(t *testing.T) {
	libs := []core.Library{
		{Name: "org.ow2.asm:asm-all:5.2"},
	}
	out := dropOW2AsmAllWhenModularPresent(libs)
	if len(out) != 1 {
		t.Fatalf("got %d", len(out))
	}
}

func TestMergeProfileJSON_roundtrip(t *testing.T) {
	parent := &core.VersionDetails{
		ID:          "1.21.1",
		MainClass:   "net.minecraft.client.main.Main",
		AssetIndex:  core.AssetIndexRef{ID: "21"},
		Libraries:   []core.Library{},
		JavaVersion: core.JavaVersionReq{MajorVersion: 21},
	}
	profile := `{"inheritsFrom":"1.21.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","arguments":{"jvm":[]},"libraries":[]}`
	merged, err := MergeProfile(parent, []byte(profile))
	if err != nil {
		t.Fatal(err)
	}
	data, err := json.Marshal(merged)
	if err != nil {
		t.Fatal(err)
	}
	var back core.VersionDetails
	if err := json.Unmarshal(data, &back); err != nil {
		t.Fatal(err)
	}
	if back.ID != parent.ID {
		t.Fatalf("ID changed to %s (caller should set after merge if needed)", back.ID)
	}
}

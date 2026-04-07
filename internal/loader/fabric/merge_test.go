package fabric

import (
	"encoding/json"
	"testing"

	"github.com/quasar/mctui/internal/core"
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

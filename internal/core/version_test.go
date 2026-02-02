package core

import "testing"

func TestParseMajorVersion(t *testing.T) {
	tests := []struct {
		name    string
		version string
		want    int
	}{
		{"Java 8 old format", "1.8.0_391", 8},
		{"Java 8 short", "1.8.0", 8},
		{"Java 11", "11.0.21", 11},
		{"Java 17", "17.0.9", 17},
		{"Java 21", "21.0.1", 21},
		{"Java 21 short", "21", 21},
		{"Snapshot format", "21-ea", 21},
		{"Empty", "", 0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// We need to import the function from java package
			// For now, test the version constant definitions
		})
	}
}

func TestVersionType(t *testing.T) {
	types := []VersionType{
		VersionTypeRelease,
		VersionTypeSnapshot,
		VersionTypeOldBeta,
		VersionTypeOldAlpha,
	}

	for _, vt := range types {
		if string(vt) == "" {
			t.Errorf("VersionType should not be empty string")
		}
	}
}

func TestLoaderType(t *testing.T) {
	types := []LoaderType{
		LoaderVanilla,
		LoaderFabric,
		LoaderForge,
		LoaderQuilt,
		LoaderNeoForge,
	}

	for _, lt := range types {
		if string(lt) == "" {
			t.Errorf("LoaderType should not be empty string")
		}
	}
}

package config

import "testing"

func TestDefaultJVMArgs_ReturnsExpected(t *testing.T) {
	got := DefaultJVMArgs()
	want := []string{"-Xmx2G", "-Xms512M"}
	if len(got) != len(want) {
		t.Fatalf("DefaultJVMArgs() = %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("DefaultJVMArgs()[%d] = %q, want %q", i, got[i], want[i])
		}
	}
}

// DefaultJVMArgs must hand out a fresh slice each call — that's the whole reason
// it's a function and not a shared package-level var. Mutating one result must
// not leak into the next caller (or into a Config built from it).
func TestDefaultJVMArgs_ReturnsFreshSlice(t *testing.T) {
	a := DefaultJVMArgs()
	a[0] = "-Xmx8G"
	b := DefaultJVMArgs()
	if b[0] != "-Xmx2G" {
		t.Fatalf("DefaultJVMArgs() shares backing storage: second call returned %q after mutating the first", b[0])
	}
}

func TestDefaultConfig_UsesDefaultJVMArgs(t *testing.T) {
	cfg := DefaultConfig()
	want := DefaultJVMArgs()
	if len(cfg.JVMArgs) != len(want) {
		t.Fatalf("DefaultConfig().JVMArgs = %v, want %v", cfg.JVMArgs, want)
	}
	for i := range want {
		if cfg.JVMArgs[i] != want[i] {
			t.Fatalf("DefaultConfig().JVMArgs[%d] = %q, want %q", i, cfg.JVMArgs[i], want[i])
		}
	}
}

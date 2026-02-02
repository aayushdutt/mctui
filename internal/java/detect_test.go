package java

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
		{"Empty string", "", 0},
		{"Invalid", "abc", 0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := parseMajorVersion(tt.version)
			if got != tt.want {
				t.Errorf("parseMajorVersion(%q) = %d, want %d", tt.version, got, tt.want)
			}
		})
	}
}

func TestParseVersionOutput_OpenJDK21(t *testing.T) {
	d := NewDetector()
	output := `openjdk version "21.0.1" 2023-10-17
OpenJDK Runtime Environment (build 21.0.1+12-29)
OpenJDK 64-Bit Server VM (build 21.0.1+12-29, mixed mode, sharing)`

	inst := d.parseVersionOutput("/usr/bin/java", output)

	if inst == nil {
		t.Fatal("Expected non-nil installation")
	}
	if inst.MajorVersion != 21 {
		t.Errorf("MajorVersion = %d, want 21", inst.MajorVersion)
	}
	if !inst.Is64Bit {
		t.Error("Expected 64-bit")
	}
	if inst.Vendor != "OpenJDK" {
		t.Errorf("Vendor = %q, want OpenJDK", inst.Vendor)
	}
}

func TestParseVersionOutput_Java8(t *testing.T) {
	d := NewDetector()
	output := `java version "1.8.0_391"
Java(TM) SE Runtime Environment (build 1.8.0_391-b13)
Java HotSpot(TM) 64-Bit Server VM (build 25.391-b13, mixed mode)`

	inst := d.parseVersionOutput("/usr/bin/java", output)

	if inst == nil {
		t.Fatal("Expected non-nil installation")
	}
	if inst.MajorVersion != 8 {
		t.Errorf("MajorVersion = %d, want 8", inst.MajorVersion)
	}
	if !inst.Is64Bit {
		t.Error("Expected 64-bit")
	}
}

func TestParseVersionOutput_Temurin(t *testing.T) {
	d := NewDetector()
	output := `openjdk version "17.0.9" 2023-10-17
OpenJDK Runtime Environment Temurin-17.0.9+9 (build 17.0.9+9)
OpenJDK 64-Bit Server VM Temurin-17.0.9+9 (build 17.0.9+9, mixed mode)`

	inst := d.parseVersionOutput("/usr/bin/java", output)

	if inst == nil {
		t.Fatal("Expected non-nil installation")
	}
	if inst.Vendor != "Eclipse Adoptium" {
		t.Errorf("Vendor = %q, want Eclipse Adoptium", inst.Vendor)
	}
}

func TestFormatInstallation(t *testing.T) {
	inst := &Installation{
		Path:         "/usr/bin/java",
		Version:      "21.0.1",
		MajorVersion: 21,
		Is64Bit:      true,
		Vendor:       "OpenJDK",
	}

	result := FormatInstallation(inst)

	if result != "Java 21 (OpenJDK, 64-bit)" {
		t.Errorf("FormatInstallation = %q, want %q", result, "Java 21 (OpenJDK, 64-bit)")
	}
}

func TestFormatInstallation_Unknown(t *testing.T) {
	inst := &Installation{
		Path:         "/usr/bin/java",
		MajorVersion: 17,
		Is64Bit:      false,
		Vendor:       "",
	}

	result := FormatInstallation(inst)

	if result != "Java 17 (Unknown, 32-bit)" {
		t.Errorf("FormatInstallation = %q, want %q", result, "Java 17 (Unknown, 32-bit)")
	}
}

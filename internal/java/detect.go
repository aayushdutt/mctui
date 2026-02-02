// Package java handles Java runtime detection and validation.
package java

import (
	"bufio"
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"time"
)

// Cached regex for version parsing (compiled once)
var versionRegex = regexp.MustCompile(`(?:java|openjdk) version "([^"]+)"`)

// Installation represents a Java installation
type Installation struct {
	Path         string // Path to java executable
	Version      string // Full version string
	MajorVersion int    // Major version (8, 11, 17, 21, etc.)
	Is64Bit      bool   // Whether it's 64-bit
	Vendor       string // OpenJDK, Oracle, Adoptium, etc.
}

// Detector finds Java installations on the system
type Detector struct {
	searchPaths []string
}

// NewDetector creates a new Java detector
func NewDetector() *Detector {
	d := &Detector{}
	d.searchPaths = d.getDefaultPaths()
	return d
}

// FindAll finds all Java installations
func (d *Detector) FindAll() []Installation {
	var installations []Installation
	seen := make(map[string]bool)

	// Check JAVA_HOME first
	if javaHome := os.Getenv("JAVA_HOME"); javaHome != "" {
		if inst := d.checkJavaHome(javaHome); inst != nil {
			installations = append(installations, *inst)
			seen[inst.Path] = true
		}
	}

	// Check PATH
	if javaPath, err := exec.LookPath("java"); err == nil {
		if inst := d.checkJava(javaPath); inst != nil && !seen[inst.Path] {
			installations = append(installations, *inst)
			seen[inst.Path] = true
		}
	}

	// Check common locations
	for _, searchPath := range d.searchPaths {
		entries, err := os.ReadDir(searchPath)
		if err != nil {
			continue
		}
		for _, entry := range entries {
			if !entry.IsDir() {
				continue
			}
			javaPath := d.findJavaInDir(filepath.Join(searchPath, entry.Name()))
			if javaPath == "" {
				continue
			}
			if inst := d.checkJava(javaPath); inst != nil && !seen[inst.Path] {
				installations = append(installations, *inst)
				seen[inst.Path] = true
			}
		}
	}

	return installations
}

// FindBest finds the best Java installation for a given version requirement
func (d *Detector) FindBest(minVersion int) *Installation {
	installations := d.FindAll()
	if len(installations) == 0 {
		return nil
	}

	var best *Installation
	for i := range installations {
		inst := &installations[i]
		if inst.MajorVersion < minVersion {
			continue
		}
		if !inst.Is64Bit {
			continue
		}
		if best == nil || inst.MajorVersion < best.MajorVersion {
			best = inst
		}
	}

	// If no perfect match, return newest
	if best == nil {
		for i := range installations {
			inst := &installations[i]
			if inst.Is64Bit && (best == nil || inst.MajorVersion > best.MajorVersion) {
				best = inst
			}
		}
	}

	return best
}

func (d *Detector) getDefaultPaths() []string {
	switch runtime.GOOS {
	case "darwin":
		return []string{
			"/Library/Java/JavaVirtualMachines",
			"/System/Library/Java/JavaVirtualMachines",
			filepath.Join(os.Getenv("HOME"), ".sdkman/candidates/java"),
			filepath.Join(os.Getenv("HOME"), ".jenv/versions"),
		}
	case "linux":
		return []string{
			"/usr/lib/jvm",
			"/usr/lib64/jvm",
			"/usr/java",
			filepath.Join(os.Getenv("HOME"), ".sdkman/candidates/java"),
			filepath.Join(os.Getenv("HOME"), ".jenv/versions"),
		}
	case "windows":
		return []string{
			`C:\Program Files\Java`,
			`C:\Program Files\Eclipse Adoptium`,
			`C:\Program Files\Zulu`,
			`C:\Program Files\Microsoft\jdk`,
		}
	default:
		return nil
	}
}

func (d *Detector) findJavaInDir(dir string) string {
	var javaName string
	if runtime.GOOS == "windows" {
		javaName = "java.exe"
	} else {
		javaName = "java"
	}

	// Try common layouts
	candidates := []string{
		filepath.Join(dir, "bin", javaName),
		filepath.Join(dir, "Contents", "Home", "bin", javaName), // macOS .jdk structure
	}

	for _, candidate := range candidates {
		if info, err := os.Stat(candidate); err == nil && !info.IsDir() {
			return candidate
		}
	}

	return ""
}

func (d *Detector) checkJavaHome(javaHome string) *Installation {
	javaPath := d.findJavaInDir(javaHome)
	if javaPath == "" {
		return nil
	}
	return d.checkJava(javaPath)
}

func (d *Detector) checkJava(javaPath string) *Installation {
	// Get real path (resolve symlinks)
	realPath, err := filepath.EvalSymlinks(javaPath)
	if err != nil {
		realPath = javaPath
	}

	// Run java -version with timeout
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	cmd := exec.CommandContext(ctx, realPath, "-version")
	output, err := cmd.CombinedOutput()
	if err != nil {
		return nil
	}

	return d.parseVersionOutput(realPath, string(output))
}

func (d *Detector) parseVersionOutput(path, output string) *Installation {
	inst := &Installation{Path: path}

	// Parse version line
	// Examples:
	// openjdk version "21.0.1" 2023-10-17
	// java version "1.8.0_391"
	// openjdk version "17.0.9" 2023-10-17 LTS
	scanner := bufio.NewScanner(strings.NewReader(output))
	for scanner.Scan() {
		line := scanner.Text()

		// Extract version (using cached regex)
		if matches := versionRegex.FindStringSubmatch(line); len(matches) > 1 {
			inst.Version = matches[1]
			inst.MajorVersion = parseMajorVersion(matches[1])
		}

		// Check for 64-bit
		if strings.Contains(line, "64-Bit") || strings.Contains(line, "amd64") || strings.Contains(line, "x86_64") {
			inst.Is64Bit = true
		}

		// Try to determine vendor (check specific vendors first, then generic)
		lineLower := strings.ToLower(line)
		switch {
		case strings.Contains(lineLower, "graalvm"):
			inst.Vendor = "GraalVM"
		case strings.Contains(lineLower, "azul"):
			inst.Vendor = "Azul Zulu"
		case strings.Contains(lineLower, "adoptium") || strings.Contains(lineLower, "temurin"):
			inst.Vendor = "Eclipse Adoptium"
		case strings.Contains(lineLower, "oracle"):
			inst.Vendor = "Oracle"
		case strings.Contains(lineLower, "microsoft"):
			inst.Vendor = "Microsoft"
		case strings.Contains(lineLower, "openjdk") && inst.Vendor == "":
			// Only set to generic OpenJDK if no specific vendor found
			inst.Vendor = "OpenJDK"
		}
	}

	// On macOS/Linux, assume 64-bit if not explicitly stated (modern systems)
	if runtime.GOOS != "windows" && !inst.Is64Bit {
		inst.Is64Bit = true
	}

	if inst.Version == "" {
		return nil
	}

	return inst
}

func parseMajorVersion(version string) int {
	// Handle old format: 1.8.0_xxx -> 8
	if strings.HasPrefix(version, "1.") {
		parts := strings.Split(version, ".")
		if len(parts) >= 2 {
			v, _ := strconv.Atoi(parts[1])
			return v
		}
	}

	// Handle new format: 17.0.1 -> 17
	parts := strings.Split(version, ".")
	if len(parts) >= 1 {
		v, _ := strconv.Atoi(parts[0])
		return v
	}

	return 0
}

// FormatInstallation returns a display string for a Java installation
func FormatInstallation(inst *Installation) string {
	arch := "32-bit"
	if inst.Is64Bit {
		arch = "64-bit"
	}

	vendor := inst.Vendor
	if vendor == "" {
		vendor = "Unknown"
	}

	return fmt.Sprintf("Java %d (%s, %s)", inst.MajorVersion, vendor, arch)
}

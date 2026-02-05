// Package launch handles the Minecraft launch pipeline.
package launch

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"github.com/quasar/mctui/internal/config"
	"github.com/quasar/mctui/internal/core"
	"github.com/quasar/mctui/internal/download"
	"github.com/quasar/mctui/internal/java"
)

// Status represents the current launch step
type Status struct {
	Step       string  // Current step name
	Progress   float64 // 0.0 - 1.0
	Message    string  // Human-readable message
	IsComplete bool
	Error      error
	LogLine    *LogLine // Streamed log output
}

// Options contains launch configuration
type Options struct {
	Instance    *core.Instance
	VersionInfo *core.VersionDetails
	JavaPath    string // Override Java path
	Offline     bool   // Skip online auth
	PlayerName  string // For offline mode
	UUID        string // Player UUID
	AccessToken string // Auth Token
	Config      *config.Config
	
	// Callbacks
	UpdateLastPlayed func(id string) error
	UpdateInstance   func(inst *core.Instance) error
}

// LogLine represents a line of log output
type LogLine struct {
	Text string
	Type string // "stdout" or "stderr"
}

// Launcher manages the game launch process
type Launcher struct {
	opts       *Options
	statusChan chan<- Status
	cfg        *config.Config
}

// NewLauncher creates a new launcher
func NewLauncher(opts *Options, statusChan chan<- Status) *Launcher {
	return &Launcher{
		opts:       opts,
		statusChan: statusChan,
		cfg:        opts.Config,
	}
}

// Launch executes the full launch pipeline
func (l *Launcher) Launch(ctx context.Context) error {
	steps := []struct {
		name string
		fn   func(context.Context) error
	}{
		{"Checking Java", l.checkJava},
		{"Downloading libraries", l.downloadLibraries},
		{"Downloading assets", l.downloadAssets},
		{"Preparing game", l.prepareGame},
		{"Launching", l.launchGame},
	}

	for i, step := range steps {
		l.sendStatus(Status{
			Step:     step.name,
			Progress: float64(i) / float64(len(steps)),
			Message:  step.name + "...",
		})

		if err := step.fn(ctx); err != nil {
			l.sendStatus(Status{
				Step:    step.name,
				Message: err.Error(),
				Error:   err,
			})
			return fmt.Errorf("%s: %w", step.name, err)
		}
	}

	// Mark instance as fully downloaded for future offline launches
	if l.opts.Instance != nil && l.opts.UpdateInstance != nil {
		l.opts.Instance.IsFullyDownloaded = true
		l.opts.Instance.CachedAt = time.Now()
		_ = l.opts.UpdateInstance(l.opts.Instance)
	}

	l.sendStatus(Status{
		Step:       "Complete",
		Progress:   1.0,
		Message:    "Game closed.",
		IsComplete: true,
	})

	return nil
}

func (l *Launcher) sendStatus(s Status) {
	if l.statusChan != nil {
		select {
		case l.statusChan <- s:
		default:
		}
	}
}

func (l *Launcher) checkJava(ctx context.Context) error {
	// 1. Check explicit override or instance setting
	if l.opts.JavaPath != "" {
		return nil
	}
	
	if l.opts.Instance != nil && l.opts.Instance.JavaPath != "" {
		if _, err := os.Stat(l.opts.Instance.JavaPath); err == nil {
			l.opts.JavaPath = l.opts.Instance.JavaPath
			l.sendStatus(Status{Step: "Checking Java", Message: "Using instance Java"})
			return nil
		}
	}

	// Determine required Java version
	requiredVersion := 8
	if l.opts.VersionInfo != nil && l.opts.VersionInfo.JavaVersion.MajorVersion > 0 {
		requiredVersion = l.opts.VersionInfo.JavaVersion.MajorVersion
	}

	// 2. Check managed java directory
	configDir, _ := os.UserConfigDir()
	if configDir != "" {
		managedJavaDir := filepath.Join(configDir, "mctui", "java", fmt.Sprintf("%d", requiredVersion))
		if exe, err := java.NewDownloader().FindJavaExecutable(managedJavaDir); err == nil {
			l.commitJavaPath(exe)
			l.sendStatus(Status{Step: "Checking Java", Message: fmt.Sprintf("Using managed Java %d", requiredVersion)})
			return nil
		}
	}

	// 3. System-wide detection
	if inst := java.NewDetector().FindBest(requiredVersion); inst != nil {
		l.commitJavaPath(inst.Path)
		l.sendStatus(Status{Step: "Checking Java", Message: fmt.Sprintf("Using %s", java.FormatInstallation(inst))})
		return nil
	}

	// 4. Download Java
	l.sendStatus(Status{Step: "Downloading Java", Message: fmt.Sprintf("Downloading Java %d...", requiredVersion)})

	if configDir == "" {
		return fmt.Errorf("could not determine config directory for java download")
	}

	javaBaseDir := filepath.Join(configDir, "mctui", "java")
	exePath, err := java.NewDownloader().DownloadRuntime(ctx, requiredVersion, javaBaseDir, func(msg string) {
		l.sendStatus(Status{Step: "Downloading Java", Message: msg})
	})
	if err != nil {
		return fmt.Errorf("failed to download java %d: %w", requiredVersion, err)
	}

	l.commitJavaPath(exePath)
	l.sendStatus(Status{Step: "Checking Java", Message: fmt.Sprintf("Downloaded Java %d", requiredVersion)})

	return nil
}

func (l *Launcher) commitJavaPath(path string) {
	l.opts.JavaPath = path
	if l.opts.Instance != nil && l.opts.UpdateInstance != nil {
		l.opts.Instance.JavaPath = path
		_ = l.opts.UpdateInstance(l.opts.Instance)
	}
}

func (l *Launcher) downloadLibraries(ctx context.Context) error {
	// Check for cancellation
	select {
	case <-ctx.Done():
		return ctx.Err()
	default:
	}

	if l.opts.VersionInfo == nil {
		return nil
	}

	// Optimization: Skip if already fully downloaded
	if l.opts.Instance != nil && l.opts.Instance.IsFullyDownloaded {
		return nil
	}

	var items []download.Item
	for _, lib := range l.opts.VersionInfo.Libraries {
		// Check rules
		if !l.libraryApplies(&lib) {
			continue
		}

		if lib.Downloads == nil || lib.Downloads.Artifact == nil {
			continue
		}

		artifact := lib.Downloads.Artifact
		destPath := filepath.Join(l.cfg.LibrariesDir, artifact.Path)

		items = append(items, download.Item{
			URL:  artifact.URL,
			Path: destPath,
			SHA1: artifact.SHA1,
			Size: artifact.Size,
		})
	}

	// Download client jar
	if l.opts.VersionInfo.Downloads.Client != nil {
		client := l.opts.VersionInfo.Downloads.Client
		clientPath := filepath.Join(l.cfg.LibrariesDir, "com", "mojang", "minecraft",
			l.opts.VersionInfo.ID, fmt.Sprintf("minecraft-%s-client.jar", l.opts.VersionInfo.ID))

		items = append(items, download.Item{
			URL:  client.URL,
			Path: clientPath,
			SHA1: client.SHA1,
			Size: client.Size,
		})
	}

	return l.performDownload(ctx, "Downloading libraries", items, 4)
}

func (l *Launcher) downloadAssets(ctx context.Context) error {
	// Check for cancellation
	select {
	case <-ctx.Done():
		return ctx.Err()
	default:
	}

	if l.opts.VersionInfo == nil {
		return nil
	}

	// Optimization: Skip if already fully downloaded
	if l.opts.Instance != nil && l.opts.Instance.IsFullyDownloaded {
		return nil
	}

	assetIndex := l.opts.VersionInfo.AssetIndex
	indexPath := filepath.Join(l.cfg.AssetsDir, "indexes", assetIndex.ID+".json")

	// Download asset index if needed
	if _, err := os.Stat(indexPath); os.IsNotExist(err) {
		mgr := download.NewManager(1)
		_, err := mgr.Download(ctx, []download.Item{{
			URL:  assetIndex.URL,
			Path: indexPath,
			SHA1: assetIndex.SHA1,
			Size: assetIndex.Size,
		}}, nil)
		if err != nil {
			return fmt.Errorf("downloading asset index: %w", err)
		}
	}

	// Parse asset index
	indexData, err := os.ReadFile(indexPath)
	if err != nil {
		return fmt.Errorf("reading asset index: %w", err)
	}

	var index struct {
		Objects map[string]struct {
			Hash string `json:"hash"`
			Size int64  `json:"size"`
		} `json:"objects"`
	}
	if err := json.Unmarshal(indexData, &index); err != nil {
		return fmt.Errorf("parsing asset index: %w", err)
	}

	// Build download list
	var items []download.Item
	for _, obj := range index.Objects {
		prefix := obj.Hash[:2]
		destPath := filepath.Join(l.cfg.AssetsDir, "objects", prefix, obj.Hash)

		items = append(items, download.Item{
			URL:  fmt.Sprintf("https://resources.download.minecraft.net/%s/%s", prefix, obj.Hash),
			Path: destPath,
			SHA1: obj.Hash,
			Size: obj.Size,
		})
	}

	return l.performDownload(ctx, "Downloading assets", items, 8)
}

func (l *Launcher) prepareGame(ctx context.Context) error {
	inst := l.opts.Instance

	// Create game directories
	dirs := []string{
		inst.Path,
		filepath.Join(inst.Path, ".minecraft"),
		filepath.Join(inst.Path, ".minecraft", "mods"),
		filepath.Join(inst.Path, ".minecraft", "resourcepacks"),
		filepath.Join(inst.Path, ".minecraft", "saves"),
	}

	for _, dir := range dirs {
		if err := os.MkdirAll(dir, 0755); err != nil {
			return fmt.Errorf("creating directory %s: %w", dir, err)
		}
	}

	return nil
}

func (l *Launcher) launchGame(ctx context.Context) error {
	args := l.buildArguments()
	inst := l.opts.Instance

	gameDir := filepath.Join(inst.Path, ".minecraft")

	cmd := exec.CommandContext(ctx, l.opts.JavaPath, args...)
	cmd.Dir = gameDir
	
	// Capture output
	stdout, _ := cmd.StdoutPipe()
	stderr, _ := cmd.StderrPipe()

	if err := cmd.Start(); err != nil {
		return err
	}

	l.sendStatus(Status{
		Step:    "Playing",
		Message: "Game running...",
	})

	// Update last played struct
	if l.opts.UpdateLastPlayed != nil {
		l.opts.UpdateLastPlayed(inst.ID)
	}

	// Stream logs
	go l.streamLog(stdout, "stdout")
	go l.streamLog(stderr, "stderr")

	// Wait for game to finish
	err := cmd.Wait()
	
	// Send final message
	if err != nil {
		return fmt.Errorf("game exited with error: %w", err)
	}
	
	// We return nil here so the pipeline considers this step "done".
	// The launcher will then send the "Complete" status.
	return nil
}

func (l *Launcher) streamLog(r io.Reader, apiType string) {
	scanner := bufio.NewScanner(r)
	for scanner.Scan() {
		text := scanner.Text()
		
		isImportant := apiType == "stderr" ||
			strings.Contains(text, "[FATAL]") || 
			strings.Contains(text, "[ERROR]") || 
			strings.Contains(text, "[WARN]") ||
			strings.Contains(text, "Exception") || 
			strings.Contains(text, "Error")

		if isImportant {
			l.sendStatus(Status{
				Step: "Launching",
				LogLine: &LogLine{
					Text: text,
					Type: apiType,
				},
			})
		}
	}
}

func (l *Launcher) buildArguments() []string {
	var args []string
	version := l.opts.VersionInfo
	inst := l.opts.Instance

	// JVM arguments - use instance settings first, then config defaults
	if len(inst.JVMArgs) > 0 {
		args = append(args, inst.JVMArgs...)
	} else if len(l.cfg.JVMArgs) > 0 {
		args = append(args, l.cfg.JVMArgs...)
	} else {
		// Sensible defaults
		args = append(args, "-Xmx2G", "-Xms512M")
	}

	// MacOS specific flags for LWJGL
	if runtime.GOOS == "darwin" {
		args = append(args, "-XstartOnFirstThread")
	}

	// Native library path
	nativesDir := filepath.Join(inst.Path, "natives")
	args = append(args, fmt.Sprintf("-Djava.library.path=%s", nativesDir))

	// Classpath
	classpath := l.buildClasspath()
	args = append(args, "-cp", classpath)

	// Main class
	args = append(args, version.MainClass)

	// Game arguments
	gameArgs := l.buildGameArguments()
	args = append(args, gameArgs...)

	return args
}

func (l *Launcher) buildClasspath() string {
	var paths []string
	version := l.opts.VersionInfo

	// Add libraries
	for _, lib := range version.Libraries {
		if !l.libraryApplies(&lib) {
			continue
		}
		if lib.Downloads == nil || lib.Downloads.Artifact == nil {
			continue
		}
		path := filepath.Join(l.cfg.LibrariesDir, lib.Downloads.Artifact.Path)
		paths = append(paths, path)
	}

	// Add client jar
	clientPath := filepath.Join(l.cfg.LibrariesDir, "com", "mojang", "minecraft",
		version.ID, fmt.Sprintf("minecraft-%s-client.jar", version.ID))
	paths = append(paths, clientPath)

	separator := ":"
	if runtime.GOOS == "windows" {
		separator = ";"
	}

	return strings.Join(paths, separator)
}

func (l *Launcher) buildGameArguments() []string {
	var args []string
	version := l.opts.VersionInfo
	inst := l.opts.Instance

	// Determine auth values
	uuid := l.opts.UUID
	if uuid == "" {
		uuid = "00000000-0000-0000-0000-000000000000"
	}
	token := l.opts.AccessToken
	if token == "" {
		token = "0"
	}
	userType := "legacy"
	if !l.opts.Offline {
		userType = "msa"
	}

	// Replacement map
	replacements := map[string]string{
		"${auth_player_name}":  l.getPlayerName(),
		"${version_name}":      version.ID,
		"${game_directory}":    filepath.Join(inst.Path, ".minecraft"),
		"${assets_root}":       l.cfg.AssetsDir,
		"${assets_index_name}": version.AssetIndex.ID,
		"${auth_uuid}":         uuid,
		"${auth_access_token}": token,
		"${user_type}":         userType,
		"${version_type}":      string(version.Type),
		"${user_properties}":   "{}",
	}

	// Process arguments
	if version.Arguments != nil && len(version.Arguments.Game) > 0 {
		for _, arg := range version.Arguments.Game {
			switch v := arg.(type) {
			case string:
				args = append(args, l.replaceVars(v, replacements))
			case map[string]interface{}:
				// Complex rules - skip for now
			}
		}
	} else if version.MinecraftArguments != "" {
		// Legacy format
		for _, arg := range strings.Split(version.MinecraftArguments, " ") {
			args = append(args, l.replaceVars(arg, replacements))
		}
	}

	return args
}

func (l *Launcher) replaceVars(s string, replacements map[string]string) string {
	result := s
	for k, v := range replacements {
		result = strings.ReplaceAll(result, k, v)
	}
	return result
}

func (l *Launcher) getPlayerName() string {
	if l.opts.PlayerName != "" {
		return l.opts.PlayerName
	}
	return "Player"
}

func (l *Launcher) libraryApplies(lib *core.Library) bool {
	if len(lib.Rules) == 0 {
		return true
	}

	allowed := false
	for _, rule := range lib.Rules {
		applies := true

		if rule.OS != nil {
			osName := runtime.GOOS
			if rule.OS.Name != "" {
				// Map Go OS names to Mojang names
				osMap := map[string]string{
					"darwin":  "osx",
					"linux":   "linux",
					"windows": "windows",
				}
				if mapped, ok := osMap[osName]; ok {
					osName = mapped
				}
				if rule.OS.Name != osName {
					applies = false
				}
			}
		}

		if applies {
			allowed = rule.Action == "allow"
		}
	}

	return allowed
}
func (l *Launcher) performDownload(ctx context.Context, stepName string, items []download.Item, workerCount int) error {
	if len(items) == 0 {
		return nil
	}

	mgr := download.NewManager(workerCount)
	progressChan := make(chan download.Progress, 10)

	// Forward progress
	go func() {
		for p := range progressChan {
			percent := 0.0
			if p.TotalBytes > 0 {
				percent = float64(p.DownloadedBytes) / float64(p.TotalBytes)
			} else if p.TotalItems > 0 {
				percent = float64(p.CompletedItems) / float64(p.TotalItems)
			}
			l.sendStatus(Status{
				Step:     stepName,
				Progress: percent,
				Message:  fmt.Sprintf("Downloading %s (%s)", p.CurrentItem, download.FormatSpeed(p.Speed)),
			})
		}
	}()

	result, err := mgr.Download(ctx, items, progressChan)
	close(progressChan)

	if err != nil {
		return err
	}
	if result.Failed > 0 {
		return fmt.Errorf("%d items failed to download", result.Failed)
	}

	return nil
}

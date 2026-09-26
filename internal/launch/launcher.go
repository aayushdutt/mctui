// Package launch handles the Minecraft launch pipeline.
package launch

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"

	"github.com/aayushdutt/mctui/internal/config"
	"github.com/aayushdutt/mctui/internal/core"
	"github.com/aayushdutt/mctui/internal/download"
	"github.com/aayushdutt/mctui/internal/java"
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

	// LogFilter is the live game-log verbosity, shared with the launch screen.
	// Nil means "fixed at Config.LaunchLogVerbosity as of NewLauncher".
	LogFilter *LogFilter

	// Callbacks
	UpdateLastPlayed func(id string) error
	AddPlayTime      func(id string, seconds int64) error
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
	// retryMax is the download retry budget passed to download managers. Defaults
	// to download.DefaultRetryMax; tests set it to 0 so failed-download paths fail
	// fast instead of incurring retryablehttp's multi-second backoff.
	retryMax int
	// cancelled is the launch context's Done channel: once closed nobody reads
	// statusChan any more, so sendStatus stops waiting.
	cancelled <-chan struct{}
}

// NewLauncher creates a new launcher
func NewLauncher(opts *Options, statusChan chan<- Status) *Launcher {
	if opts.LogFilter == nil {
		v := LogVerbosityError
		if opts.Config != nil {
			v = ParseLaunchLogVerbosity(opts.Config.LaunchLogVerbosity)
		}
		opts.LogFilter = NewLogFilter(v)
	}
	return &Launcher{
		opts:       opts,
		statusChan: statusChan,
		cfg:        opts.Config,
		retryMax:   download.DefaultRetryMax,
	}
}

// Launch executes the full launch pipeline
func (l *Launcher) Launch(ctx context.Context) error {
	l.cancelled = ctx.Done()
	// Invalidate download skip if instance version/loader changed (before any download step).
	l.invalidateStaleDownloadCache()

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
		inst := l.opts.Instance
		inst.IsFullyDownloaded = true
		inst.CachedAt = time.Now()
		inst.DownloadCacheKey = core.LaunchDownloadKey(inst)
		_ = l.opts.UpdateInstance(inst)
	}

	l.sendStatus(Status{
		Step:       "Complete",
		Progress:   1.0,
		Message:    "Game closed.",
		IsComplete: true,
	})

	return nil
}

// sendStatus delivers s to the launch screen, waiting until it's read — dropping
// statuses when the channel was full lost the "Playing" step (the screen then
// treated a running game as still launching) and the tail of crash logs. After
// the launch is cancelled nobody reads the channel, so it stops waiting.
func (l *Launcher) sendStatus(s Status) {
	if l.statusChan == nil {
		return
	}
	select {
	case l.statusChan <- s:
	case <-l.cancelled:
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

// invalidateStaleDownloadCache clears IsFullyDownloaded when version/loader/LoaderVer changed
// so switching e.g. vanilla→Fabric cannot skip downloading new libraries.
func (l *Launcher) invalidateStaleDownloadCache() {
	inst := l.opts.Instance
	if inst == nil || l.opts.UpdateInstance == nil {
		return
	}
	if !inst.IsFullyDownloaded {
		return
	}
	want := core.LaunchDownloadKey(inst)
	if inst.DownloadCacheKey == want {
		return
	}
	inst.IsFullyDownloaded = false
	inst.CachedAt = time.Time{}
	inst.DownloadCacheKey = ""
	_ = l.opts.UpdateInstance(inst)
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
		mgr := download.NewManagerWithRetryMax(1, l.retryMax)
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
	l.cancelled = ctx.Done()
	// Cancelled after the previous step: don't start a game only to stop it.
	if err := ctx.Err(); err != nil {
		return err
	}
	args := l.buildArguments()
	inst := l.opts.Instance

	gameDir := filepath.Join(inst.Path, ".minecraft")

	// Not exec.CommandContext: its cancel is Process.Kill, which gives the game
	// no chance to shut down. stopOnCancel interrupts first (see below).
	cmd := exec.Command(l.opts.JavaPath, args...)
	cmd.Dir = gameDir

	// Capture output through in-memory pipes rather than StdoutPipe: exec then
	// copies the game's output into them and Wait joins that copying, bounded
	// by WaitDelay — so a child process that inherited the game's stdout can't
	// keep this launch "playing" after the game itself has exited.
	stdout, stdoutW := io.Pipe()
	stderr, stderrW := io.Pipe()
	cmd.Stdout = stdoutW
	cmd.Stderr = stderrW
	cmd.WaitDelay = outputDrainDelay

	if err := cmd.Start(); err != nil {
		return err
	}
	exited := make(chan struct{})
	defer close(exited)
	go stopOnCancel(ctx, cmd.Process, exited, stopGracePeriod)

	l.sendStatus(Status{
		Step:    "Playing",
		Message: "Game running...",
	})

	// Update last played struct
	if l.opts.UpdateLastPlayed != nil {
		l.opts.UpdateLastPlayed(inst.ID)
	}
	// Mark the session start so we can credit playtime once the game exits.
	startedAt := time.Now()

	// Stream logs until EOF. The readers are joined before returning, so no
	// status is sent after Launch returns — the caller closes statusChan then.
	var streams sync.WaitGroup
	streams.Add(2)
	go func() { defer streams.Done(); l.streamLog(stdout, "stdout") }()
	go func() { defer streams.Done(); l.streamLog(stderr, "stderr") }()

	// Wait for the game to exit and exec to finish copying its output (all of
	// it has been read by then — the pipes are synchronous), then end the
	// streams.
	err := cmd.Wait()
	if errors.Is(err, exec.ErrWaitDelay) {
		err = nil // the game exited cleanly; a lingering child held its output
	}
	_ = stdoutW.Close()
	_ = stderrW.Close()
	streams.Wait()

	// Credit the session's wall-clock duration to the instance's total playtime,
	// regardless of how the game exited (clean quit, crash, or ctx cancel).
	if l.opts.AddPlayTime != nil {
		if secs := int64(time.Since(startedAt).Seconds()); secs > 0 {
			l.opts.AddPlayTime(inst.ID, secs)
		}
	}

	// Send final message
	if err != nil {
		return fmt.Errorf("game exited with error: %w", err)
	}

	// We return nil here so the pipeline considers this step "done".
	// The launcher will then send the "Complete" status.
	return nil
}

// outputDrainDelay bounds how long Wait keeps copying the game's output after
// the game has exited (see launchGame). A var so tests can shorten it.
var outputDrainDelay = 2 * time.Second

// stopGracePeriod is how long a cancelled game gets to shut down after the
// interrupt before it is killed. A var so tests can shorten it.
var stopGracePeriod = 15 * time.Second

// stopOnCancel stops the game when ctx is cancelled (the user chose "Stop
// game"): it interrupts the JVM, which runs its shutdown hooks, and kills it
// only if it is still running after grace. Returns once the game has exited
// (exited is closed after Wait) or been killed.
func stopOnCancel(ctx context.Context, p *os.Process, exited <-chan struct{}, grace time.Duration) {
	select {
	case <-exited:
		return
	case <-ctx.Done():
	}
	if err := p.Signal(os.Interrupt); err != nil {
		// Windows can't deliver an interrupt to another process.
		_ = p.Kill()
		return
	}
	select {
	case <-exited:
	case <-time.After(grace):
		_ = p.Kill()
	}
}

// maxLogLineBytes caps how much of one game log line is kept; the rest of an
// overlong line is discarded (the launch screen shows a line, not a blob).
const maxLogLineBytes = 16 << 10

// streamLog forwards the game's output to the launch screen until EOF. It must
// keep draining no matter what the game prints: if it stopped reading, the pipe
// would fill and the game would block writing its log (a bufio.Scanner quits
// for good on a line over 64 KiB, which froze the game).
func (l *Launcher) streamLog(r io.Reader, apiType string) {
	br := bufio.NewReaderSize(r, 64<<10)
	for {
		text, err := readLogLine(br, maxLogLineBytes)
		if err != nil {
			return
		}
		if !shouldEmitGameLogLine(l.opts.LogFilter.Get(), text) {
			continue
		}
		l.sendStatus(Status{
			Step: "Playing",
			LogLine: &LogLine{
				Text: text,
				Type: apiType,
			},
		})
	}
}

// readLogLine reads one line (without its line ending), keeping at most max
// bytes and discarding the remainder of a longer line. It returns an error only
// when no line was read (io.EOF at the end of the stream).
func readLogLine(br *bufio.Reader, max int) (string, error) {
	var buf []byte
	for {
		chunk, isPrefix, err := br.ReadLine()
		if err != nil {
			if len(buf) > 0 {
				return string(buf), nil
			}
			return "", err
		}
		if room := max - len(buf); room > 0 {
			buf = append(buf, chunk[:min(len(chunk), room)]...)
		}
		if !isPrefix {
			return string(buf), nil
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
		args = append(args, config.DefaultJVMArgs()...)
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
	seen := make(map[string]struct{})
	addPath := func(p string) {
		p = filepath.Clean(p)
		if _, ok := seen[p]; ok {
			return
		}
		seen[p] = struct{}{}
		paths = append(paths, p)
	}

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
		addPath(path)
	}

	// Add client jar
	clientPath := filepath.Join(l.cfg.LibrariesDir, "com", "mojang", "minecraft",
		version.ID, fmt.Sprintf("minecraft-%s-client.jar", version.ID))
	addPath(clientPath)

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

	mgr := download.NewManagerWithRetryMax(workerCount, l.retryMax)
	progressChan := make(chan download.Progress, 10)

	// Forward progress. Joined below so no status is sent after the step
	// returns (Download stops sending before it returns).
	forwarded := make(chan struct{})
	go func() {
		defer close(forwarded)
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
	<-forwarded

	if err != nil {
		return err
	}
	if result.Failed > 0 {
		return fmt.Errorf("%d items failed to download", result.Failed)
	}

	return nil
}

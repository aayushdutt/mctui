package launch

import (
	"bufio"
	"context"
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/aayushdutt/mctui/internal/config"
	"github.com/aayushdutt/mctui/internal/core"
)

func TestReadLogLine(t *testing.T) {
	long := strings.Repeat("x", 100)
	br := bufio.NewReaderSize(strings.NewReader(long+"\nnext\r\nlast"), 16)

	want := []string{long[:40], "next", "last"}
	for i, w := range want {
		got, err := readLogLine(br, 40)
		if err != nil {
			t.Fatalf("line %d: unexpected error %v", i, err)
		}
		if got != w {
			t.Errorf("line %d = %q, want %q", i, got, w)
		}
	}
	if _, err := readLogLine(br, 40); err == nil {
		t.Error("expected EOF after the last line")
	}
}

// The game's output must be drained to EOF no matter what it prints, and both
// readers must be done before launchGame returns (the app closes statusChan
// right after). A >64 KiB line used to stop the reader for good: the pipe
// filled, the game blocked writing, and the tail of the log was lost.
func TestLaunchGame_DrainsOutputAndJoinsReaders(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("uses a /bin/sh stand-in for java")
	}
	dir := t.TempDir()
	fakeJava := filepath.Join(dir, "java")
	script := "#!/bin/sh\n" +
		// One 200 KB line, then enough output to overflow the pipe if unread.
		"head -c 200000 /dev/zero | tr '\\0' 'x'\n" +
		"echo\n" +
		"i=0; while [ $i -lt 2000 ]; do echo \"[main/ERROR]: line $i\"; i=$((i+1)); done\n" +
		"echo '[main/ERROR]: last line' 1>&2\n"
	if err := os.WriteFile(fakeJava, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(dir, ".minecraft"), 0o755); err != nil {
		t.Fatal(err)
	}

	statusChan := make(chan Status, 5000) // roomy: sendStatus drops when full
	l := NewLauncher(&Options{
		Instance:    &core.Instance{ID: "t", Name: "t", Path: dir, Version: "1.21.4"},
		VersionInfo: &core.VersionDetails{ID: "1.21.4", MainClass: "net.minecraft.client.main.Main"},
		Config:      &config.Config{DataDir: dir, AssetsDir: dir, LibrariesDir: dir},
		JavaPath:    fakeJava,
	}, statusChan)

	var lines []string
	collected := make(chan struct{})
	go func() {
		defer close(collected)
		for s := range statusChan {
			if s.LogLine != nil {
				lines = append(lines, s.LogLine.Text)
			}
		}
	}()

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	err := l.launchGame(ctx)
	close(statusChan) // panics if a log reader were still sending
	<-collected

	if ctx.Err() != nil {
		t.Fatal("the game hung writing its log (output not drained)")
	}
	if err != nil {
		t.Fatalf("launchGame: %v", err)
	}
	if !slices.Contains(lines, "[main/ERROR]: last line") {
		t.Errorf("the log tail was lost; got %d lines", len(lines))
	}
	for _, line := range lines {
		if len(line) > maxLogLineBytes {
			t.Errorf("line of %d bytes exceeds the %d cap", len(line), maxLogLineBytes)
		}
	}
}

func TestLogFilter_NilIsErrorOnly(t *testing.T) {
	var f *LogFilter
	if f.Get() != LogVerbosityError {
		t.Errorf("nil filter = %v, want LogVerbosityError", f.Get())
	}
	f.Set(LogVerbosityAll) // must not panic
	g := NewLogFilter(LogVerbosityWarn)
	g.Set(LogVerbosityAll)
	if g.Get() != LogVerbosityAll {
		t.Errorf("Get = %v after Set(All)", g.Get())
	}
}

// Stopping a running game must interrupt it (so the JVM runs its shutdown
// hooks) instead of SIGKILLing it, and kill it only if it ignores that.
func TestLaunchGame_StopInterruptsThenKills(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("uses a /bin/sh stand-in for java")
	}
	orig := stopGracePeriod
	t.Cleanup(func() { stopGracePeriod = orig })

	cases := []struct {
		name       string
		trap       string
		grace      time.Duration // generous when the game should exit on its own
		wantMarker bool
	}{
		{"graceful", `trap 'echo stopped > "$0.marker"; exit 0' INT`, 10 * time.Second, true},
		{"ignores interrupt", `trap '' INT`, 300 * time.Millisecond, false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			stopGracePeriod = tc.grace
			dir := t.TempDir()
			fakeJava := filepath.Join(dir, "java")
			script := "#!/bin/sh\n" + tc.trap + "\n" +
				"echo '[main/ERROR]: running'\n" +
				"while true; do sleep 0.05; done\n"
			if err := os.WriteFile(fakeJava, []byte(script), 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.MkdirAll(filepath.Join(dir, ".minecraft"), 0o755); err != nil {
				t.Fatal(err)
			}
			statusChan := make(chan Status, 100)
			l := NewLauncher(&Options{
				Instance:    &core.Instance{ID: "t", Name: "t", Path: dir, Version: "1.21.4"},
				VersionInfo: &core.VersionDetails{ID: "1.21.4", MainClass: "net.minecraft.client.main.Main"},
				Config:      &config.Config{DataDir: dir, AssetsDir: dir, LibrariesDir: dir},
				JavaPath:    fakeJava,
			}, statusChan)

			ctx, cancel := context.WithCancel(context.Background())
			done := make(chan error, 1)
			go func() { done <- l.launchGame(ctx) }()

			// Wait until the game is running (its first log line arrives).
			deadline := time.After(5 * time.Second)
			for running := false; !running; {
				select {
				case s := <-statusChan:
					running = s.LogLine != nil
				case <-deadline:
					t.Fatal("game never started")
				}
			}
			cancel()

			select {
			case <-done:
			case <-time.After(8 * time.Second):
				t.Fatal("launchGame didn't return after stop")
			}
			_, err := os.Stat(fakeJava + ".marker")
			if got := err == nil; got != tc.wantMarker {
				t.Errorf("shutdown handler ran = %v, want %v", got, tc.wantMarker)
			}
		})
	}
}

// fakeJava writes a /bin/sh stand-in for java and returns a launcher for it.
func fakeJava(t *testing.T, body string, statusChan chan Status) *Launcher {
	t.Helper()
	if runtime.GOOS == "windows" {
		t.Skip("uses a /bin/sh stand-in for java")
	}
	dir := t.TempDir()
	java := filepath.Join(dir, "java")
	if err := os.WriteFile(java, []byte("#!/bin/sh\n"+body+"\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(dir, ".minecraft"), 0o755); err != nil {
		t.Fatal(err)
	}
	return NewLauncher(&Options{
		Instance:    &core.Instance{ID: "t", Name: "t", Path: dir, Version: "1.21.4"},
		VersionInfo: &core.VersionDetails{ID: "1.21.4", MainClass: "net.minecraft.client.main.Main"},
		Config:      &config.Config{DataDir: dir, AssetsDir: dir, LibrariesDir: dir},
		JavaPath:    java,
		LogFilter:   NewLogFilter(LogVerbosityAll),
	}, statusChan)
}

// A child process that inherited the game's stdout must not keep the launch
// "playing" after the game itself exits (the readers used to wait for EOF).
func TestLaunchGame_LingeringChildDoesntHoldTheLaunch(t *testing.T) {
	orig := outputDrainDelay
	outputDrainDelay = 200 * time.Millisecond
	t.Cleanup(func() { outputDrainDelay = orig })

	statusChan := make(chan Status, 100)
	l := fakeJava(t, "sleep 3 &\necho started", statusChan)
	start := time.Now()
	if err := l.launchGame(context.Background()); err != nil {
		t.Fatalf("launchGame: %v", err)
	}
	if d := time.Since(start); d > 2*time.Second {
		t.Errorf("launchGame took %v; a lingering child held it past the game's exit", d)
	}
}

// Statuses used to be dropped whenever the (10-slot) channel was full: the
// tail of a crash log, even the "Playing" step. With a slow reader and a tiny
// buffer, every line must still arrive.
func TestLaunchGame_DeliversEveryLogLine(t *testing.T) {
	statusChan := make(chan Status, 1)
	l := fakeJava(t, `i=0; while [ $i -lt 200 ]; do echo "line $i" 1>&2; i=$((i+1)); done`, statusChan)

	var lines []string
	playing := false
	collected := make(chan struct{})
	go func() {
		defer close(collected)
		for s := range statusChan {
			time.Sleep(200 * time.Microsecond) // a UI that's a bit slower than the game
			if s.Step == "Playing" && s.LogLine == nil {
				playing = true
			}
			if s.LogLine != nil {
				lines = append(lines, s.LogLine.Text)
			}
		}
	}()
	if err := l.launchGame(context.Background()); err != nil {
		t.Fatalf("launchGame: %v", err)
	}
	close(statusChan)
	<-collected

	if !playing {
		t.Error("the Playing status was dropped")
	}
	if len(lines) != 200 || lines[len(lines)-1] != "line 199" {
		t.Errorf("got %d lines (last %q), want all 200 ending with line 199", len(lines), lines[len(lines)-1])
	}
}

// A cancel that lands between steps must not start the game.
func TestLaunchGame_CancelledBeforeStartDoesntStart(t *testing.T) {
	l := fakeJava(t, `echo started > "$0.marker"`, nil)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := l.launchGame(ctx); !errors.Is(err, context.Canceled) {
		t.Errorf("err = %v, want context.Canceled", err)
	}
	if _, err := os.Stat(l.opts.JavaPath + ".marker"); err == nil {
		t.Error("the game was started despite the cancel")
	}
}

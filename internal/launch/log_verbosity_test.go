package launch

import "testing"

func TestParseLaunchLogVerbosity(t *testing.T) {
	tests := []struct {
		in   string
		want LogVerbosity
	}{
		{"", LogVerbosityError},
		{"error", LogVerbosityError},
		{"warn", LogVerbosityWarn},
		{"WARNING", LogVerbosityWarn},
		{"all", LogVerbosityAll},
		{"verbose", LogVerbosityAll},
	}
	for _, tt := range tests {
		if got := ParseLaunchLogVerbosity(tt.in); got != tt.want {
			t.Errorf("ParseLaunchLogVerbosity(%q) = %v, want %v", tt.in, got, tt.want)
		}
	}
}

func TestCycleLaunchLogVerbosity(t *testing.T) {
	s := "error"
	s = CycleLaunchLogVerbosity(s)
	if ParseLaunchLogVerbosity(s) != LogVerbosityWarn {
		t.Fatalf("after one cycle: %q", s)
	}
	s = CycleLaunchLogVerbosity(s)
	if ParseLaunchLogVerbosity(s) != LogVerbosityAll {
		t.Fatalf("after two cycles: %q", s)
	}
	s = CycleLaunchLogVerbosity(s)
	if ParseLaunchLogVerbosity(s) != LogVerbosityError {
		t.Fatalf("after three cycles: %q", s)
	}
}

func TestShouldEmitGameLogLine(t *testing.T) {
	if !shouldEmitGameLogLine(LogVerbosityError, "[01:00:00] [main/ERROR]: boom") {
		t.Fatal("want error line in error mode")
	}
	if shouldEmitGameLogLine(LogVerbosityError, "[01:00:00] [main/WARN]: nah") {
		t.Fatal("do not want warn line in error mode")
	}
	if !shouldEmitGameLogLine(LogVerbosityWarn, "[01:00:00] [main/WARN]: nah") {
		t.Fatal("want warn line in warn mode")
	}
	if shouldEmitGameLogLine(LogVerbosityWarn, "Some info text without marker") {
		t.Fatal("plain info hidden in warn mode")
	}
	if !shouldEmitGameLogLine(LogVerbosityAll, "Some info text without marker") {
		t.Fatal("plain info visible in all mode")
	}
}

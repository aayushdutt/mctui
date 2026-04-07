package launch

import (
	"strings"
)

// LogVerbosity controls which game log lines are surfaced in the TUI during play.
type LogVerbosity int

const (
	LogVerbosityError LogVerbosity = iota // default: errors / stack traces only
	LogVerbosityWarn                      // + warnings
	LogVerbosityAll                       // full stdout / stderr
)

type logSeverity int

const (
	logSeverityDebug logSeverity = iota
	logSeverityInfo
	logSeverityWarn
	logSeverityError
)

// ParseLaunchLogVerbosity maps config / JSON values to LogVerbosity.
func ParseLaunchLogVerbosity(s string) LogVerbosity {
	switch strings.ToLower(strings.TrimSpace(s)) {
	case "warn", "warning", "warnings":
		return LogVerbosityWarn
	case "all", "verbose", "debug":
		return LogVerbosityAll
	default:
		return LogVerbosityError
	}
}

// ConfigString returns the value persisted in config.json.
func (v LogVerbosity) ConfigString() string {
	switch v {
	case LogVerbosityWarn:
		return "warn"
	case LogVerbosityAll:
		return "all"
	default:
		return "error"
	}
}

// ShortLabel is a compact string for the launch screen footer.
func (v LogVerbosity) ShortLabel() string {
	switch v {
	case LogVerbosityWarn:
		return "errors+warnings"
	case LogVerbosityAll:
		return "all"
	default:
		return "errors"
	}
}

// CycleLaunchLogVerbosity advances error → warn → all → error.
func CycleLaunchLogVerbosity(s string) string {
	v := ParseLaunchLogVerbosity(s)
	switch v {
	case LogVerbosityError:
		return LogVerbosityWarn.ConfigString()
	case LogVerbosityWarn:
		return LogVerbosityAll.ConfigString()
	default:
		return LogVerbosityError.ConfigString()
	}
}

func (v LogVerbosity) minSeverity() logSeverity {
	switch v {
	case LogVerbosityAll:
		return logSeverityDebug
	case LogVerbosityWarn:
		return logSeverityWarn
	default:
		return logSeverityError
	}
}

func classifyJavaLogLine(line string) logSeverity {
	t := strings.TrimSpace(line)
	lower := strings.ToLower(line)

	if strings.Contains(lower, "[debug]") || strings.Contains(lower, "[trace]") ||
		strings.Contains(lower, "/debug]") || strings.Contains(lower, "/trace]") ||
		strings.Contains(lower, "[finer]") || strings.Contains(lower, "[fine]") {
		return logSeverityDebug
	}
	if strings.Contains(lower, "[warn]") || strings.Contains(lower, "[warning]") ||
		strings.Contains(lower, "/warn]") || strings.Contains(lower, "/warning]") ||
		strings.HasPrefix(lower, "warning:") {
		return logSeverityWarn
	}
	if strings.Contains(lower, "[error]") || strings.Contains(lower, "[fatal]") ||
		strings.Contains(lower, "[severe]") || strings.Contains(lower, "/error]") ||
		strings.Contains(lower, "/fatal]") || strings.Contains(lower, "/severe]") {
		return logSeverityError
	}
	if strings.Contains(lower, "exception") || strings.Contains(lower, "caused by:") {
		return logSeverityError
	}
	if strings.HasPrefix(t, "at ") && strings.Contains(line, "(") {
		return logSeverityError
	}
	if strings.HasPrefix(t, "\tat ") {
		return logSeverityError
	}
	return logSeverityInfo
}

func shouldEmitGameLogLine(verb LogVerbosity, line string) bool {
	sev := classifyJavaLogLine(line)
	return sev >= verb.minSeverity()
}

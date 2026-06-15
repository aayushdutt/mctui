package ui

import (
	"fmt"
	"os/exec"
	"runtime"
	"strings"
)

// isHTTPURL reports whether target is a plain http(s) URL. Used to gate
// browser-open of catalog-supplied links: it rejects empty strings and values
// starting with "-" (which open/xdg-open would parse as a flag, not a URL).
func isHTTPURL(target string) bool {
	return strings.HasPrefix(target, "https://") || strings.HasPrefix(target, "http://")
}

// openURL opens target (a URL or path) in the OS default handler. Unlike
// openBrowser it surfaces the error, so callers can flash a notice on failure.
func openURL(target string) error {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		cmd = exec.Command("open", target)
	case "windows":
		cmd = exec.Command("rundll32", "url.dll,FileProtocolHandler", target)
	case "linux":
		cmd = exec.Command("xdg-open", target)
	default:
		return fmt.Errorf("unsupported platform")
	}
	return cmd.Start()
}

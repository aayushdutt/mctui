//go:build live

package java

import (
	"context"
	"net/url"
	"testing"
	"time"
)

// TestLive_ResolveAdoptiumURL hits the REAL Adoptium API. It is excluded from the
// normal suite by the `live` build tag; run it with `go test -tags live`.
func TestLive_ResolveAdoptiumURL(t *testing.T) {
	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()

	d := NewDownloader()
	link, name, err := d.resolveAdoptiumURL(ctx, 21)
	if err != nil {
		t.Fatalf("resolveAdoptiumURL: %v", err)
	}
	if link == "" {
		t.Error("link is empty")
	}
	if name == "" {
		t.Error("name is empty")
	}
	u, err := url.Parse(link)
	if err != nil || u.Scheme == "" || u.Host == "" {
		t.Errorf("link %q does not look like a URL (err=%v)", link, err)
	}
}

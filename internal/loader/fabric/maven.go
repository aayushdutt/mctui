package fabric

import (
	"fmt"
	"strings"
)

// mavenArtifactPath returns the repository-relative path (forward slashes) for a Maven coordinate.
func mavenArtifactPath(coord string) (string, error) {
	parts := strings.Split(coord, ":")
	if len(parts) < 3 || len(parts) > 4 {
		return "", fmt.Errorf("invalid maven coordinate %q", coord)
	}
	group := strings.ReplaceAll(parts[0], ".", "/")
	artifact := parts[1]
	version := parts[2]
	classifier := ""
	if len(parts) == 4 {
		classifier = parts[3]
	}
	fileStem := artifact + "-" + version
	if classifier != "" {
		fileStem += "-" + classifier
	}
	fileStem += ".jar"
	return fmt.Sprintf("%s/%s/%s/%s", group, artifact, version, fileStem), nil
}

// joinRepoURL joins a Maven repo base URL with a repository-relative artifact path.
func joinRepoURL(base, rel string) string {
	return strings.TrimSuffix(base, "/") + "/" + rel
}

package fabric

import (
	"fmt"
	"strings"
)

// parseMavenLibraryParts splits a Gradle-style Maven coordinate: "group:artifact:version" or
// "group:artifact:version:classifier".
func parseMavenLibraryParts(name string) (group, artifact, version, classifier string, ok bool) {
	parts := strings.Split(name, ":")
	switch len(parts) {
	case 3:
		return parts[0], parts[1], parts[2], "", true
	case 4:
		return parts[0], parts[1], parts[2], parts[3], true
	default:
		return "", "", "", "", false
	}
}

// mavenArtifactPath returns the repository-relative path (forward slashes) for a Maven coordinate.
func mavenArtifactPath(coord string) (string, error) {
	g, artifact, version, classifier, ok := parseMavenLibraryParts(coord)
	if !ok {
		return "", fmt.Errorf("invalid maven coordinate %q", coord)
	}
	groupPath := strings.ReplaceAll(g, ".", "/")
	fileStem := artifact + "-" + version
	if classifier != "" {
		fileStem += "-" + classifier
	}
	fileStem += ".jar"
	return fmt.Sprintf("%s/%s/%s/%s", groupPath, artifact, version, fileStem), nil
}

// joinRepoURL joins a Maven repo base URL with a repository-relative artifact path.
func joinRepoURL(base, rel string) string {
	return strings.TrimSuffix(base, "/") + "/" + rel
}

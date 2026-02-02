package main

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"runtime"
)

func main() {
	version := 21
	osName := runtime.GOOS
	if osName == "darwin" {
		osName = "mac"
	}

	arch := runtime.GOARCH
	if arch == "amd64" {
		arch = "x64"
	} else if arch == "arm64" {
		arch = "aarch64"
	}

	url := fmt.Sprintf("https://api.adoptium.net/v3/assets/feature_releases/%d/ga?architecture=%s&heap_size=normal&image_type=jre&jvm_impl=hotspot&os=%s&page=0&page_size=1&project=jdk&sort_method=DEFAULT&sort_order=DESC&vendor=eclipse", version, arch, osName)

	fmt.Printf("Querying: %s\n", url)

	resp, err := http.Get(url)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != 200 {
		body, _ := io.ReadAll(resp.Body)
		panic(fmt.Sprintf("Status %d: %s", resp.StatusCode, string(body)))
	}

	var releases []interface{}
	if err := json.NewDecoder(resp.Body).Decode(&releases); err != nil {
		panic(err)
	}

	if len(releases) == 0 {
		fmt.Println("No releases found")
		return
	}

	fmt.Printf("Found %d release(s)\n", len(releases))

	// Print the first one's download link
	rel := releases[0].(map[string]interface{})
	binaries := rel["binaries"].([]interface{})
	binary := binaries[0].(map[string]interface{})
	pkg := binary["package"].(map[string]interface{})
	downloadURL := pkg["link"].(string)

	fmt.Printf("Download URL: %s\n", downloadURL)
	fmt.Printf("Name: %s\n", pkg["name"])
	fmt.Printf("Config: %v\n", releaseName(rel))
}

func releaseName(rel map[string]interface{}) string {
	return rel["release_name"].(string)
}

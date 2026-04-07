.PHONY: build run test lint clean dev reset-auth damage-auth reset-all

# Build the binary
build:
	go build -o mctui .

# Run in development mode
run:
	go run .

# Run with hot reload (requires air: go install github.com/air-verse/air@latest)
dev:
	air

# Run tests
test:
	go test -v ./...

# Run tests with coverage
test-cover:
	go test -coverprofile=coverage.out ./...
	go tool cover -html=coverage.out -o coverage.html

# Lint (tries golangci-lint, falls back to go vet)
lint:
	@if command -v golangci-lint >/dev/null; then \
		golangci-lint run; \
	else \
		echo "golangci-lint not found, running go vet instead..."; \
		go vet ./...; \
	fi

# Format code
fmt:
	go fmt ./...
	@if command -v goimports >/dev/null; then \
		goimports -w .; \
	fi

# Tidy dependencies
tidy:
	go mod tidy

# Clean build artifacts
clean:
	rm -f mctui
	rm -f coverage.out coverage.html

# Build for all platforms
build-all:
	GOOS=darwin GOARCH=amd64 go build -o dist/mctui-darwin-amd64 .
	GOOS=darwin GOARCH=arm64 go build -o dist/mctui-darwin-arm64 .
	GOOS=linux GOARCH=amd64 go build -o dist/mctui-linux-amd64 .
	GOOS=windows GOARCH=amd64 go build -o dist/mctui-windows-amd64.exe .

# Default app data dir (override for portable layout: make damage-auth MCTUI_DATA_DIR=./data)
# Matches config.getDefaultDataDir when XDG_DATA_HOME is unset and not using APPDATA.
MCTUI_DATA_DIR ?= $(HOME)/.local/share/mctui
ACCOUNTS_JSON := $(MCTUI_DATA_DIR)/accounts.json

# Reset authentication (deletes accounts.json)
reset-auth:
	rm -f $(ACCOUNTS_JSON)

# Corrupt stored MSA tokens for testing session validation / re-login (keeps accounts.json)
damage-auth:
	@test -f $(ACCOUNTS_JSON) || (echo "No accounts file at $(ACCOUNTS_JSON); sign in once, or set MCTUI_DATA_DIR." && false)
	@command -v jq >/dev/null || (echo "damage-auth needs jq (e.g. brew install jq)" && false)
	@jq -e '[.accounts[]? | select(.type == "msa")] | length > 0' "$(ACCOUNTS_JSON)" >/dev/null \
		|| (echo "No MSA accounts in $(ACCOUNTS_JSON)" && false)
	jq '.accounts |= map(if .type == "msa" then . + {accessToken: "__mctui_damage_auth__", expiresAt: "2099-01-01T00:00:00Z"} else . end)' \
		"$(ACCOUNTS_JSON)" > "$(ACCOUNTS_JSON).tmp" && mv "$(ACCOUNTS_JSON).tmp" "$(ACCOUNTS_JSON)"
	@echo Damaged MSA tokens in $(ACCOUNTS_JSON)

# Reset all data (instances, cache, auth)
reset-all:
	rm -rf $(MCTUI_DATA_DIR)

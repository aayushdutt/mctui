.PHONY: build run dev test fmt-check verify lint fmt tidy clean build-all reset-auth damage-auth reset-all

# Build the binary
build:
	go build -o mctui .

# Run in development mode
run:
	go run .

# Run with hot reload (requires air: go install github.com/air-verse/air@latest)
dev:
	air

# Run tests. One target, composable modifiers:
#   make test                      whole suite (unit + integration + e2e)
#   make test SCOPE=unit           all packages except the e2e package
#   make test SCOPE=e2e            the teatest end-to-end scenarios
#   make test RACE=1               under the race detector
#   make test COVER=1              with coverage -> coverage.{out,html}
#   make test V=1                  verbose
#   make test RUN=TestName         filter by test name (regex)
#   make test SCOPE=e2e RACE=1 V=1 modifiers compose
SCOPE ?= all
ifeq ($(SCOPE),unit)
  TEST_PKGS := $(shell go list ./... | grep -v '/internal/app')
else ifeq ($(SCOPE),e2e)
  TEST_PKGS := ./internal/app
  TEST_RUN  := E2E
else
  TEST_PKGS := ./...
endif
RUN ?= $(TEST_RUN)

test:
	go test \
		$(if $(filter 1,$(RACE)),-race,) \
		$(if $(filter 1,$(V)),-v,) \
		$(if $(filter 1,$(COVER)),-coverprofile=coverage.out,) \
		$(if $(RUN),-run $(RUN),) \
		$(TEST_PKGS)
	@if [ "$(COVER)" = "1" ]; then go tool cover -html=coverage.out -o coverage.html; fi

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

# Fail if any file is not gofmt-clean (CI-friendly)
fmt-check:
	@unformatted=$$(gofmt -l .); \
	if [ -n "$$unformatted" ]; then \
		echo "Not gofmt-clean:"; echo "$$unformatted"; exit 1; \
	fi

# Full pre-PR / CI gate: format check, vet, and race-enabled tests
verify: fmt-check
	go vet ./...
	go test -race ./...

# Tidy dependencies
tidy:
	go mod tidy

# Clean build artifacts (binary, coverage, cross-build output in dist/)
clean:
	rm -f mctui coverage.out coverage.html
	rm -rf dist

# Build for all platforms (writes to dist/)
build-all:
	mkdir -p dist
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

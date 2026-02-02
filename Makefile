.PHONY: build run test lint clean dev

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

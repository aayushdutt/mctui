.PHONY: build run test test-live lint fmt fmt-check verify screenshots clean reset-auth damage-auth reset-all

# Release build: target/release/mctui
build:
	cargo build --release --locked -p mctui

# Run from source
run:
	cargo run -p mctui

# Every offline test. Filter by name with RUN=, e.g. `make test RUN=wizard`.
test:
	cargo test --workspace --locked $(RUN)

# Tests that talk to the real services (Mojang, Fabric, Modrinth, Microsoft,
# Adoptium, Vanilla Tweaks); they need the network. `live_launch` downloads a
# whole game, so it's left out: run it with `cargo test -p mctui-core --test
# launch live_launch -- --ignored`.
test-live:
	cargo test -p mctui-core --locked -- --ignored --skip real_data_loads_cleanly --skip live_launch

lint:
	cargo clippy --workspace --all-targets --locked -- -D warnings

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all --check

# What CI runs
verify: fmt-check lint test

# Renders every screen to target/screenshots/*.png (needs Python with Pillow).
screenshots:
	rm -rf target/screenshots
	MCTUI_SCREENSHOTS=$(CURDIR)/target/screenshots cargo test -p mctui screenshots -- --ignored
	for f in target/screenshots/*.ansi; do python3 scripts/ansi2png.py "$$f" "$${f%.ansi}.png"; done

clean:
	cargo clean

# The default data folder (override for a portable or scratch one:
# make damage-auth MCTUI_DATA_DIR=./data)
MCTUI_DATA_DIR ?= $(HOME)/.local/share/mctui
ACCOUNTS_JSON := $(MCTUI_DATA_DIR)/accounts.json

# Sign out of everything (deletes accounts.json)
reset-auth:
	rm -f $(ACCOUNTS_JSON)

# Corrupt stored Microsoft tokens, to test session checks and signing in again
# (keeps accounts.json)
damage-auth:
	@test -f $(ACCOUNTS_JSON) || (echo "No accounts file at $(ACCOUNTS_JSON); sign in once, or set MCTUI_DATA_DIR." && false)
	@command -v jq >/dev/null || (echo "damage-auth needs jq (e.g. brew install jq)" && false)
	@jq -e '[.accounts[]? | select(.type == "msa")] | length > 0' "$(ACCOUNTS_JSON)" >/dev/null \
		|| (echo "No MSA accounts in $(ACCOUNTS_JSON)" && false)
	jq '.accounts |= map(if .type == "msa" then . + {accessToken: "__mctui_damage_auth__", expiresAt: "2099-01-01T00:00:00Z"} else . end)' \
		"$(ACCOUNTS_JSON)" > "$(ACCOUNTS_JSON).tmp" && mv "$(ACCOUNTS_JSON).tmp" "$(ACCOUNTS_JSON)"
	@echo Damaged MSA tokens in $(ACCOUNTS_JSON)

# Delete all data: instances, cache, accounts
reset-all:
	rm -rf $(MCTUI_DATA_DIR)

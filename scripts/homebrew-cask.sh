#!/usr/bin/env bash
# Prints the Homebrew cask for a release: homebrew-cask.sh <version> <checksums.txt>
# The release workflow writes it to aayushdutt/homebrew-tap as Casks/mctui.rb.
set -euo pipefail

version="$1"
checksums="$2"

sha() {
  local sum
  sum=$(awk -v f="$1" '$2 == f { print $1 }' "$checksums")
  if [ -z "$sum" ]; then
    echo "no checksum for $1 in $checksums" >&2
    exit 1
  fi
  echo "$sum"
}

url="https://github.com/aayushdutt/mctui/releases/download/v#{version}"

cat <<EOF
cask "mctui" do
  version "$version"

  on_macos do
    on_intel do
      url "$url/mctui_Darwin_x86_64.tar.gz"
      sha256 "$(sha mctui_Darwin_x86_64.tar.gz)"
    end
    on_arm do
      url "$url/mctui_Darwin_arm64.tar.gz"
      sha256 "$(sha mctui_Darwin_arm64.tar.gz)"
    end
  end

  on_linux do
    on_intel do
      url "$url/mctui_Linux_x86_64.tar.gz"
      sha256 "$(sha mctui_Linux_x86_64.tar.gz)"
    end
    on_arm do
      url "$url/mctui_Linux_arm64.tar.gz"
      sha256 "$(sha mctui_Linux_arm64.tar.gz)"
    end
  end

  name "mctui"
  desc "Fast terminal-based Minecraft launcher"
  homepage "https://github.com/aayushdutt/mctui"

  binary "mctui"

  # mctui ships unsigned: clear the quarantine flag so it starts without a
  # Gatekeeper prompt.
  postflight do
    if OS.mac?
      system_command "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "#{staged_path}/mctui"]
    end
  end
end
EOF

#!/bin/sh
# Nectar install script
# Usage: curl -fsSL https://buildnectar.com/install.sh | sh

set -e

REPO="BlakeBurnette/nectar-lang"
INSTALL_DIR="${NECTAR_INSTALL_DIR:-$HOME/.nectar/bin}"

# Detect OS and architecture
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)  OS="unknown-linux-gnu" ;;
        Darwin) OS="apple-darwin" ;;
        MINGW*|MSYS*|CYGWIN*) OS="pc-windows-msvc" ;;
        *) echo "Error: unsupported OS: $OS"; exit 1 ;;
    esac

    case "$ARCH" in
        x86_64|amd64) ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *) echo "Error: unsupported architecture: $ARCH"; exit 1 ;;
    esac

    PLATFORM="${ARCH}-${OS}"
}

# Get latest version from GitHub
get_latest_version() {
    VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
    if [ -z "$VERSION" ]; then
        echo "Error: could not determine latest version"
        exit 1
    fi
}

main() {
    echo "Installing Nectar..."
    echo ""

    detect_platform
    get_latest_version

    ARCHIVE="nectar-${VERSION}-${PLATFORM}.tar.gz"
    URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARCHIVE}"

    echo "  Platform: ${PLATFORM}"
    echo "  Version:  ${VERSION}"
    echo "  URL:      ${URL}"
    echo ""

    # Create install directory
    mkdir -p "$INSTALL_DIR"

    # Download and extract
    TMPDIR=$(mktemp -d)
    trap "rm -rf $TMPDIR" EXIT

    echo "Downloading..."
    curl -fsSL "$URL" -o "$TMPDIR/$ARCHIVE"

    echo "Extracting..."
    tar xzf "$TMPDIR/$ARCHIVE" -C "$INSTALL_DIR"
    chmod +x "$INSTALL_DIR/nectar"

    echo ""
    echo "Nectar ${VERSION} installed to ${INSTALL_DIR}/nectar"
    echo ""

    # Check if install dir is on PATH
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *)
            echo "Add Nectar to your PATH by adding this to your shell profile:"
            echo ""
            SHELL_NAME=$(basename "$SHELL")
            case "$SHELL_NAME" in
                zsh)  echo "  echo 'export PATH=\"\$HOME/.nectar/bin:\$PATH\"' >> ~/.zshrc" ;;
                bash) echo "  echo 'export PATH=\"\$HOME/.nectar/bin:\$PATH\"' >> ~/.bashrc" ;;
                fish) echo "  set -Ux fish_user_paths \$HOME/.nectar/bin \$fish_user_paths" ;;
                *)    echo "  export PATH=\"\$HOME/.nectar/bin:\$PATH\"" ;;
            esac
            echo ""
            echo "Then restart your shell or run: export PATH=\"\$HOME/.nectar/bin:\$PATH\""
            ;;
    esac

    echo ""
    echo "Get started:"
    echo "  nectar init my-app"
    echo "  cd my-app"
    echo "  nectar dev"
    echo ""
}

main

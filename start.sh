#!/usr/bin/env bash
set -euo pipefail

# recode development/run script

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

usage() {
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  dev     Run in development mode (Tauri + Vite HMR)"
    echo "  build   Build the production application"
    echo "  test    Run all frontend tests (Vitest)"
    echo "  lint    Run TypeScript type checking"
    echo "  clean   Remove build artifacts and cache"
    echo ""
    exit 1
}

if [[ $# -lt 1 ]]; then
    usage
fi

COMMAND=$1

case "$COMMAND" in
    dev)
        info "Launching development environment..."
        # Note: Using the existing scripth that handles port checks and vite startup
        bash scripts/tauri-dev-with-vite.sh
        ;;
    build)
        info "Building production application..."
        npm run build
        info "Running Tauri build..."
        npm run tauri:build
        success "Build complete."
        ;;
    test)
        info "Running frontend tests..."
        npm run test
        ;;
    lint)
        info "Running TypeScript type check..."
        npm run type-check
        ;;
    clean)
        info "Cleaning build artifacts..."
        rm -rf dist
        rm -rf target
        rm -rf src-tauri/target
        rm -rf node_modules/.vite
        success "Cleanup complete."
        ;;
    *)
        error "Unknown command: $COMMAND"
        usage
        ;;
esac

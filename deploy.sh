#!/usr/bin/env bash
set -euo pipefail

# ============================================================
# CEX Arbitrage System - One-click Deployment Script
# Usage:
#   ./deploy.sh                                              # Local deploy
#   REPO_URL=https://github.com/user/arbclaw.git ./deploy.sh # Remote clone
# Frontend auto-detects host from browser, no domain config needed.
# ============================================================

REPO_URL="${REPO_URL:-}"
INSTALL_DIR="${INSTALL_DIR:-/opt/arbclaw}"
BRANCH="${BRANCH:-main}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log()  { echo -e "${GREEN}[+]${NC} $*"; }
warn() { echo -e "${YELLOW}[!]${NC} $*"; }
err()  { echo -e "${RED}[✗]${NC} $*"; exit 1; }

# --- Pre-flight checks ---

command -v docker >/dev/null 2>&1 || err "Docker not found. Install: https://docs.docker.com/engine/install/"
command -v git    >/dev/null 2>&1 || err "Git not found. Install: apt install git / yum install git"

if ! docker compose version >/dev/null 2>&1; then
    err "Docker Compose V2 not found. Update Docker or install compose plugin."
fi

# --- Determine project directory ---

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ -f "$SCRIPT_DIR/docker-compose.yml" ]; then
    PROJECT_DIR="$SCRIPT_DIR"
    log "Using local repo at $PROJECT_DIR"
else
    if [ -z "$REPO_URL" ]; then
        err "Not in repo directory and REPO_URL not set.\n  Usage: REPO_URL=https://github.com/user/arbclaw.git ./deploy.sh"
    fi
    PROJECT_DIR="$INSTALL_DIR"
    if [ -d "$PROJECT_DIR/.git" ]; then
        log "Pulling latest code..."
        cd "$PROJECT_DIR"
        git fetch origin "$BRANCH"
        git reset --hard "origin/$BRANCH"
    else
        log "Cloning repository..."
        mkdir -p "$(dirname "$PROJECT_DIR")"
        git clone -b "$BRANCH" "$REPO_URL" "$PROJECT_DIR"
    fi
fi

cd "$PROJECT_DIR"

# --- Build & Deploy ---

log "Stopping existing containers (if any)..."
docker compose down --remove-orphans 2>/dev/null || true

log "Building images..."
docker compose build

log "Starting services..."
docker compose up -d

# --- Health check ---

log "Waiting for services to start..."
sleep 5

RETRIES=10
for i in $(seq 1 $RETRIES); do
    if curl -sf --noproxy localhost "http://localhost:80/health" >/dev/null 2>&1; then
        break
    fi
    if [ "$i" -eq "$RETRIES" ]; then
        warn "Health check failed after ${RETRIES} attempts. Check logs: docker compose logs"
    fi
    sleep 2
done

# --- Detect server IP ---

SERVER_IP=$(hostname -I 2>/dev/null | awk '{print $1}' || echo "localhost")
[ -z "$SERVER_IP" ] && SERVER_IP="localhost"

echo ""
log "========================================="
log "  Deployment complete!"
log "========================================="
log "  Dashboard:  http://${SERVER_IP}"
log "  Health:     http://${SERVER_IP}/health"
log "  Memory:     http://${SERVER_IP}/api/memory"
echo ""
log "  Frontend auto-detects host from browser URL."
log "  Access via IP, domain, or localhost - all work."
echo ""
log "  Useful commands:"
log "    docker compose logs -f          # View live logs"
log "    docker compose logs engine      # Engine logs only"
log "    docker compose ps               # Service status"
log "    docker compose restart           # Restart all"
log "    docker compose down              # Stop all"
echo ""

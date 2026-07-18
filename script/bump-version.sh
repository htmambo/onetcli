#!/bin/bash

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

# Check if version argument is provided
new_version=$1
if [ -z "$new_version" ]
then
  echo -e "${RED}${BOLD}Error:${RESET} Version argument is required"
  echo -e "${YELLOW}USAGE:${RESET} ./bump.sh [VERSION]"
  exit 1
fi

# 安全断言：仅 bump main 应用版本，库 crate（0.5.x 系）与内部 crate（0.1.x）保持独立版本线。
if ! grep -q '^name = "main"' main/Cargo.toml 2>/dev/null; then
  echo -e "${RED}${BOLD}Error:${RESET} main/Cargo.toml not found or package not named 'main'"
  exit 1
fi

# Logging functions
function log_header() {
  local message=$1
  echo ""
  echo -e "${BOLD}${BLUE}╔════════════════════════════════════════════════════════╗${RESET}"
  echo -e "${BOLD}${BLUE}║${RESET}  ${CYAN}${BOLD}$message${RESET}"
  echo -e "${BOLD}${BLUE}╚════════════════════════════════════════════════════════╝${RESET}"
  echo ""
}

function log_step() {
  local step=$1
  local message=$2
  echo -e "${MAGENTA}${BOLD}[$step]${RESET} ${message}"
}

function log_success() {
  local message=$1
  echo -e "${GREEN}${BOLD}✓${RESET} ${message}"
}

function log_info() {
  local message=$1
  echo -e "${CYAN}ℹ${RESET} ${message}"
}

function log_error() {
  local message=$1
  echo -e "${RED}${BOLD}✗${RESET} ${message}"
}

# Start release process
log_header "Starting Release Process for v$new_version"

# Step 1: Update main app version（仅 main，库 crate 与内部 crate 保持独立版本线）
log_step "1/4" "Updating main app to version ${BOLD}v$new_version${RESET}"
if cargo set-version -p main "$new_version"; then
  log_success "Main app version updated successfully"
else
  log_error "Failed to update crates version"
  exit 1
fi
echo ""

# Step 2: Stage changes
log_step "2/4" "Staging modified files"
if git add -u .; then
  log_success "Files staged successfully"
else
  log_error "Failed to stage files"
  exit 1
fi
echo ""

# Step 3: Create commit and tag
log_step "3/4" "Creating commit and tag"
if git commit -m "Bump v$new_version"; then
  log_success "Commit created: ${BOLD}Bump v$new_version${RESET}"
else
  log_error "Failed to create commit"
  exit 1
fi

if git tag "v$new_version"; then
  log_success "Tag created: ${BOLD}v$new_version${RESET}"
else
  log_error "Failed to create tag"
  exit 1
fi
echo ""

# Step 4: Push to remote
log_step "4/4" "Pushing tag to remote"
log_info "Pushing ${BOLD}v$new_version${RESET} to origin..."
if git push origin "v$new_version"; then
  log_success "Tag pushed to remote successfully"
else
  log_error "Failed to push tag to remote"
  exit 1
fi
echo ""

# Success message
echo -e "${GREEN}${BOLD}╔════════════════════════════════════════════════════════╗${RESET}"
echo -e "${GREEN}${BOLD}║${RESET}  ${BOLD}🚀 Release v$new_version standby!${RESET}"
echo -e "${GREEN}${BOLD}║${RESET}  ${GREEN}Let's ship it!${RESET}"
echo -e "${GREEN}${BOLD}╚════════════════════════════════════════════════════════╝${RESET}"
echo ""

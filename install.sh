#!/bin/sh
set -eu

BINARY_NAME="slack"
REPO_URL="https://github.com/TeamCadenceAI/slack-cli"
INSTALL_DIR="${HOME}/.slack/bin"
LINK_DIR="${HOME}/.local/bin"
INSTALLED_BINARY="${INSTALL_DIR}/${BINARY_NAME}"
LINK_BINARY="${LINK_DIR}/${BINARY_NAME}"
TMP_DIR=""

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  BOLD="$(printf '\033[1m')"
  RED="$(printf '\033[31m')"
  YELLOW="$(printf '\033[33m')"
  RESET="$(printf '\033[0m')"
else
  BOLD=""
  RED=""
  YELLOW=""
  RESET=""
fi

cleanup() {
  if [ -n "${TMP_DIR}" ] && [ -d "${TMP_DIR}" ]; then
    rm -rf "${TMP_DIR}"
  fi
}

trap cleanup EXIT HUP INT TERM

status() {
  printf '%s%s%s\n' "${BOLD}" "$*" "${RESET}"
}

warn() {
  printf '%swarning:%s %s\n' "${YELLOW}" "${RESET}" "$*" >&2
}

die() {
  printf '%serror:%s %s\n' "${RED}" "${RESET}" "$*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command '$1' was not found"
}

script_dir() {
  if [ -f "$0" ]; then
    (CDPATH=; cd "$(dirname "$0")" && pwd -P)
  else
    pwd -P
  fi
}

detect_target() {
  OS="$(uname -s)"
  ARCH="$(uname -m)"

  case "${OS}:${ARCH}" in
    Darwin:arm64|Darwin:aarch64)
      printf 'aarch64-apple-darwin'
      ;;
    Darwin:x86_64)
      printf 'x86_64-apple-darwin'
      ;;
    Linux:aarch64|Linux:arm64)
      printf 'aarch64-unknown-linux-gnu'
      ;;
    Linux:x86_64|Linux:amd64)
      printf 'x86_64-unknown-linux-gnu'
      ;;
    *)
      die "unsupported platform ${OS}/${ARCH}. Install from source: ${REPO_URL}"
      ;;
  esac
}

latest_release_tag() {
  LATEST_URL="${REPO_URL}/releases/latest"
  EFFECTIVE_URL="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "${LATEST_URL}")" \
    || die "failed to resolve latest release from ${LATEST_URL}"
  TAG="${EFFECTIVE_URL##*/}"

  case "${TAG}" in
    v[0-9]*)
      printf '%s' "${TAG}"
      ;;
    *)
      die "could not determine latest release tag from ${EFFECTIVE_URL}"
      ;;
  esac
}

download_file() {
  URL="$1"
  DEST="$2"
  HTTP_STATUS="$(curl -L -sS --connect-timeout 10 --retry 2 -w '%{http_code}' -o "${DEST}" "${URL}")" \
    || die "failed to download ${URL}"

  case "${HTTP_STATUS}" in
    2??)
      ;;
    *)
      rm -f "${DEST}"
      die "failed to download ${URL}: HTTP ${HTTP_STATUS}"
      ;;
  esac
}

install_binary() {
  SOURCE="$1"
  mkdir -p "${INSTALL_DIR}" "${LINK_DIR}" \
    || die "failed to create install directories"
  install -m 0755 "${SOURCE}" "${INSTALLED_BINARY}" \
    || die "failed to install binary to ${INSTALLED_BINARY}"
  ln -sfn "${INSTALLED_BINARY}" "${LINK_BINARY}" \
    || die "failed to create symlink at ${LINK_BINARY}"

  if [ "$(uname -s)" = "Darwin" ]; then
    if command -v xattr >/dev/null 2>&1; then
      xattr -d com.apple.quarantine "${INSTALLED_BINARY}" 2>/dev/null || true
    else
      warn "xattr unavailable; if macOS blocks ${BINARY_NAME}, run: xattr -d com.apple.quarantine ${INSTALLED_BINARY}"
    fi
  fi
}

run_source_install() {
  SCRIPT_DIR="$1"
  SOURCE_BINARY="${SCRIPT_DIR}/target/release/${BINARY_NAME}"

  require_cmd cargo

  status "Building ${BINARY_NAME} from source..."
  (cd "${SCRIPT_DIR}" && cargo build --release) \
    || die "cargo build failed"

  [ -x "${SOURCE_BINARY}" ] || die "expected built binary at ${SOURCE_BINARY}"
  install_binary "${SOURCE_BINARY}"
}

run_download_install() {
  require_cmd curl
  require_cmd tar
  require_cmd uname
  require_cmd install
  require_cmd mktemp

  TARGET="$(detect_target)"
  TAG="$(latest_release_tag)"
  ASSET="slack-${TARGET}.tar.gz"
  URL="${REPO_URL}/releases/download/${TAG}/${ASSET}"

  TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/slack-install.XXXXXX")" \
    || die "failed to create temporary directory"
  ARCHIVE="${TMP_DIR}/${ASSET}"

  status "Downloading ${ASSET} (${TAG})..."
  download_file "${URL}" "${ARCHIVE}"

  BINARY_PATH="$(tar -tzf "${ARCHIVE}" | awk -v name="${BINARY_NAME}" '
    {
      n = split($0, parts, "/")
      if (parts[n] == name) { print $0; exit }
    }
  ')"
  [ -n "${BINARY_PATH}" ] || die "archive did not contain a ${BINARY_NAME} binary"

  tar -xzf "${ARCHIVE}" -C "${TMP_DIR}" "${BINARY_PATH}" \
    || die "failed to extract ${BINARY_NAME} from archive"

  install_binary "${TMP_DIR}/${BINARY_PATH}"
}

path_contains_link_dir() {
  case ":${PATH:-}:" in
    *":${LINK_DIR}:"*) return 0 ;;
    *) return 1 ;;
  esac
}

print_path_hint() {
  SHELL_NAME="$(basename "${SHELL:-sh}")"
  case "${SHELL_NAME}" in
    fish)
      warn "${LINK_DIR} is not on PATH; add to ~/.config/fish/config.fish:"
      printf '  fish_add_path %s\n' "${LINK_DIR}" >&2
      ;;
    zsh)
      warn "${LINK_DIR} is not on PATH; add to ~/.zshrc:"
      printf '  export PATH="%s:$PATH"\n' "${LINK_DIR}" >&2
      ;;
    bash)
      RC="${HOME}/.bashrc"
      [ "$(uname -s)" = "Darwin" ] && RC="${HOME}/.bash_profile"
      warn "${LINK_DIR} is not on PATH; add to ${RC}:"
      printf '  export PATH="%s:$PATH"\n' "${LINK_DIR}" >&2
      ;;
    *)
      warn "${LINK_DIR} is not on PATH; add to ~/.profile:"
      printf '  export PATH="%s:$PATH"\n' "${LINK_DIR}" >&2
      ;;
  esac
}

verify_install() {
  status "Installed ${BINARY_NAME} to ${INSTALLED_BINARY}"
  status "Linked   ${LINK_BINARY}"
  "${LINK_BINARY}" --version || die "installed binary failed to run"

  if path_contains_link_dir; then
    RESOLVED="$(command -v "${BINARY_NAME}" || true)"
    if [ -n "${RESOLVED}" ] && [ "${RESOLVED}" != "${LINK_BINARY}" ]; then
      warn "${BINARY_NAME} resolves to ${RESOLVED} — move ${LINK_DIR} earlier in PATH to use this install"
    fi
  else
    print_path_hint
  fi
}

SCRIPT_DIR="$(script_dir)"

if [ -f "${SCRIPT_DIR}/Cargo.toml" ] && [ -f "${SCRIPT_DIR}/install.sh" ]; then
  run_source_install "${SCRIPT_DIR}"
else
  run_download_install
fi

verify_install

status ""
status "Installation complete! Run 'slack --help' to get started."
status "Docs and source: ${REPO_URL}"

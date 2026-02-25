#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. && pwd)"
SRC="${DESKTOP_SRC:-${DIR}/scripts/jollypad-dev.desktop}"
SYSTEM_DIR="/usr/share/wayland-sessions"
DEST="${SYSTEM_DIR}/jollypad-dev.desktop"
NAME="${NAME_OVERRIDE:-JollyPad (dev)}"
DESKTOP_NAME="${DESKTOP_NAME_OVERRIDE:-JollyPad-dev}"
SUDO=""
if [ "${EUID:-$(id -u)}" -ne 0 ]; then SUDO="sudo"; fi
BIN_DIR="${BIN_DIR:-/usr/local/bin}"
BIN_NAMES=("jolly-home" "jolly-launcher" "jolly-nav" "jolly-settings")
build() {
  (cd "${DIR}" && cargo build --release)
}
install_binaries() {
  ${SUDO} mkdir -p "${BIN_DIR}"
  for b in "${BIN_NAMES[@]}"; do
    src="${DIR}/target/release/${b}"
    if [ ! -x "${src}" ]; then
      printf "missing binary after build: %s\n" "${src}" >&2
      exit 1
    fi
    ${SUDO} install -m 755 "${src}" "${BIN_DIR}/${b}"
  done
}
install_session() {
  ${SUDO} mkdir -p "${SYSTEM_DIR}"
  build
  install_binaries
  if [ ! -f "${SRC}" ]; then
    printf "missing template: %s\n" "${SRC}" >&2
    exit 1
  fi
  tmp="$(mktemp)"
  cp -f "${SRC}" "${tmp}"
  sed -i \
    -e "s|^Name=.*$|Name=${NAME}|" \
    -e "s|^DesktopNames=.*$|DesktopNames=${DESKTOP_NAME}|" \
    -e "s|^Exec=.*$|Exec=${BIN_DIR}/jolly-launcher|" \
    "${tmp}"
  sed -i '/^TryExec=/d' "${tmp}"
  ${SUDO} mv -f "${tmp}" "${DEST}"
  ${SUDO} chmod 644 "${DEST}"
  printf "%s\n" "${DEST}"
}
remove_session() {
  if [ -f "${DEST}" ]; then
    ${SUDO} rm -f "${DEST}"
  fi
}
status() {
  if [ -f "${DEST}" ]; then
    printf "%s\n" "${DEST}"
  else
    printf "not installed\n"
  fi
}
print() {
  if [ -f "${DEST}" ]; then
    cat "${DEST}"
  fi
}
case "${1:-install}" in
  install) install_session ;;
  remove|uninstall) remove_session ;;
  build) build ;;
  status) status ;;
  print) print ;;
  *) install_session ;;
esac

#!/usr/bin/env bash
set -euo pipefail
trap 'echo "error at line $LINENO" >&2; exit 1' ERR

DIR=$(cd "$(dirname "$0")" && pwd)
PROJ=$(cd "$DIR/.." && pwd)
cd "$PROJ"

BASE_VERSION="${BASE_VERSION:-0.1.0}"
DEB_REV="${DEB_REV:-1}"
TS="$(date -u +%Y%m%d%H%M)"
GIT_SHA="$(git rev-parse --short HEAD 2>/dev/null || true)"
if [ -z "${VERSION:-}" ]; then
  if [ -n "${PHS_DEV_BUILD:-}" ]; then
    VERSION="${BASE_VERSION}-${DEB_REV}~dev${TS}${GIT_SHA:++g${GIT_SHA}}"
  else
    VERSION="${BASE_VERSION}-${DEB_REV}${GIT_SHA:++g${GIT_SHA}}"
  fi
fi
ARCH="$(dpkg --print-architecture 2>/dev/null || echo amd64)"

# 明确列出需要构建与打包的 apps 二进制
APPS=("jolly-launcher" "jolly-home" "jolly-settings" "jolly-nav")

# 优先一次性构建所有 workspace 中的所有 bins，加速并尽量覆盖
cargo build --workspace --release --bins || true

# 对于目标二进制，逐个确保构建成功
for a in "${APPS[@]}"; do
  if [ ! -x "target/release/${a}" ]; then
    cargo build --release --bin "$a" || cargo build --release -p "$a"
  fi
  [ -x "target/release/${a}" ] || { echo "missing binary: $a" >&2; exit 1; }
done

WORK="$(mktemp -d)"
PKG="${WORK}/pkg"
mkdir -p "${PKG}/DEBIAN" "${PKG}/usr/bin"

for a in "${APPS[@]}"; do
  install -m 0755 "target/release/${a}" "${PKG}/usr/bin/${a}"
done

# 严格使用指定路径的 control 文件，不再内联生成
CONTROL_SRC="$DIR/debian/control"
[ -f "$CONTROL_SRC" ] || { echo "control not found: $CONTROL_SRC" >&2; exit 1; }
cp "$CONTROL_SRC" "${PKG}/DEBIAN/control"
sed -i "s/^Version:.*/Version: ${VERSION}/" "${PKG}/DEBIAN/control"
sed -i "s/^Architecture:.*/Architecture: ${ARCH}/" "${PKG}/DEBIAN/control"

OUT="$(cd "$PROJ/artifacts" 2>/dev/null || mkdir -p "$PROJ/artifacts"; echo "$PROJ/artifacts")"
dpkg-deb -b "$PKG" "${OUT}/jollypad_${VERSION}_${ARCH}.deb" >/dev/null
echo "${OUT}/jollypad_${VERSION}_${ARCH}.deb"

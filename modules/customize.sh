#!/system/bin/sh

SKIPUNZIP=1

unzip -o "${ZIPFILE}" 'common/*' -d "${TMPDIR}" >&2

. "${TMPDIR}/common/functions.sh"
. "${TMPDIR}/common/install.sh"

install_main

#!/system/bin/sh
SKIPUNZIP=1

unzip -o "${ZIPFILE}" 'module.prop' 'common/functions.sh' 'common/install.sh' -d "${MODPATH}" >&2
. "${MODPATH}/common/install.sh"

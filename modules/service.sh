#!/system/bin/sh
MODDIR=${0%/*}

"${MODDIR}/system/bin/cluxd" >/dev/null 2>&1 &

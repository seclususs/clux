#!/system/bin/sh

SKIPUNZIP=1
MODID="sys_qos"

unzip -o "$ZIPFILE" 'module.prop' 'common/utils.sh' -d "$MODPATH" >&2

. "$MODPATH/common/utils.sh"

ui_print_header

ui_print_log "[1/4] Verifying system environment..."
if [ "$ARCH" != "arm64" ]; then
    ui_print_warn "Incompatible architecture ($ARCH). Daemon targets arm64."
fi

ui_print_log "[2/4] Extracting module files..."
unzip -o "$ZIPFILE" 'service.sh' 'system/bin/qos_daemon' 'system.prop' 'uninstall.sh' -d "$MODPATH" >&2

ui_print_log "[3/4] Setting file permissions..."
set_perm_recursive "$MODPATH" 0 0 0755 0644
set_perm "$MODPATH/service.sh" 0 0 0755
set_perm "$MODPATH/system/bin/qos_daemon" 0 0 0755

ui_print_log "[4/4] Finalizing installation..."
rm -f "$MODPATH/common/utils.sh" "$MODPATH/customize.sh" "$MODPATH/update.json" 2>/dev/null
find "$MODPATH" -empty -type d -delete
[ -e /data/system/package_cache ] && rm -rf /data/system/package_cache/*

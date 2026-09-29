#!/system/bin/sh

ACTIVE_DIR="/data/adb/modules/$MODID"

grep_prop() { sed -n "s/^$1=//p" "$2"; }
get_prop() { getprop "$1"; }

ui_print_header() {
    ui_print "***********************************************"
    ui_print "  Version : v$(grep_prop version "$MODPATH/module.prop")"
    ui_print "***********************************************"
}

ui_print_log() { ui_print "- $1"; }
ui_print_info() { ui_print "  ➜ $1"; }
ui_print_warn() { ui_print "  ! $1"; }
ui_print_err() { ui_print "  X $1"; }


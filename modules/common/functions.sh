#!/system/bin/sh

read_prop() {
    sed -n "s/^$1=//p" "$2"
}

log_step() {
    ui_print "- $1"
}

log_warn() {
    ui_print "  ! $1"
}

log_fail() {
    abort "  X $1"
}

unpack_files() {
    unzip -o "${ZIPFILE}" "$@" -d "${MODPATH}" >&2
}

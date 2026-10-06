#!/system/bin/sh

DAEMON_PATH="system/bin/cluxd"
MIN_API=29

print_banner() {
    version="$(read_prop version "${MODPATH}/module.prop")"
    ui_print "*************************************"
    ui_print "  cluxd v${version}"
    ui_print "*************************************"
}

check_platform() {
    log_step "[1/3] Verifying system environment..."
    [ "${ARCH}" = "arm64" ] || log_fail "Unsupported architecture (${ARCH}). cluxd requires arm64."
    [ "${API}" -ge "${MIN_API}" ] || log_fail "Unsupported Android API (${API}). cluxd requires API ${MIN_API} or newer."
}

extract_payload() {
    log_step "[2/3] Extracting module files..."
    unpack_files module.prop service.sh system.prop uninstall.sh "${DAEMON_PATH}"
}

apply_permissions() {
    log_step "[3/3] Setting file permissions..."
    set_perm_recursive "${MODPATH}" 0 0 0755 0644
    set_perm "${MODPATH}/service.sh" 0 0 0755
    set_perm "${MODPATH}/uninstall.sh" 0 0 0755
    set_perm "${MODPATH}/${DAEMON_PATH}" 0 0 0755
}

install_main() {
    unpack_files module.prop
    print_banner
    check_platform
    extract_payload
    apply_permissions
}

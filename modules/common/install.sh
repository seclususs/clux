#!/system/bin/sh

. "${MODPATH}/common/functions.sh"

print_header

ui_print "- Checking system requirements..."
check_cpu_arch
check_api_compat
check_kernel_compat
check_psi_nodes

ui_print "- Extracting binaries..."
extract_binary

ui_print "- Configuring service..."
extract_services

ui_print "- Setting permissions..."
set_permissions

ui_print "- Cleaning up..."
cleanup

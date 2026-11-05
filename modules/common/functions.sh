#!/system/bin/sh

check_cpu_arch() {
  local abi
  abi="$(getprop ro.product.cpu.abi)"
  if [ "${abi}" != "arm64-v8a" ]; then
    abort "err: cpu arch unsupported -> ${abi}"
  fi
}

check_api_compat() {
  local api_level
  api_level="$(getprop ro.build.version.sdk)"

  if [ -z "${api_level}" ]; then
    abort "err: sdk prop unreadable"
  fi

  if [ "${api_level}" -lt 29 ]; then
    abort "err: api level low -> required >= 29 got ${api_level}"
  fi
}

check_kernel_compat() {
  local kernel_ver major minor
  kernel_ver="$(uname -r | cut -d'-' -f1)"
  major="$(echo "${kernel_ver}" | cut -d'.' -f1)"
  minor="$(echo "${kernel_ver}" | cut -d'.' -f2)"

  if [ -z "${major}" ] || [ -z "${minor}" ]; then
    abort "err: kernel version parse failed"
  fi

  if [ "${major}" -lt 4 ] || { [ "${major}" -eq 4 ] && [ "${minor}" -lt 14 ]; }; then
    abort "err: kernel obsolete -> required >= 4.14 got ${kernel_ver}"
  fi
}

check_psi_nodes() {
  if [ ! -r "/proc/pressure/cpu" ] || [ ! -w "/proc/pressure/cpu" ]; then
    abort "err: psi node inaccessible -> /proc/pressure/cpu"
  fi
}

print_header() {
  local mod_id mod_name mod_version mod_versioncode mod_author mod_description
  mod_id="$(sed -n "s/^id=//p" "${MODPATH}/module.prop")"
  mod_name="$(sed -n "s/^name=//p" "${MODPATH}/module.prop")"
  mod_version="$(sed -n "s/^version=//p" "${MODPATH}/module.prop")"
  mod_versioncode="$(sed -n "s/^versionCode=//p" "${MODPATH}/module.prop")"
  mod_author="$(sed -n "s/^author=//p" "${MODPATH}/module.prop")"
  mod_description="$(sed -n "s/^description=//p" "${MODPATH}/module.prop")"

  ui_print "*******************************************"
  ui_print "  ID           : ${mod_id}"
  ui_print "  Name         : ${mod_name}"
  ui_print "  Version      : ${mod_version}"
  ui_print "  Version Code : ${mod_versioncode}"
  ui_print "  Author       : ${mod_author}"
  ui_print "  Description  : ${mod_description}"
  ui_print "*******************************************"
}

extract_binary() {
  mkdir -p "${MODPATH}/system/bin"
  unzip -j -o "${ZIPFILE}" 'system/bin/cluxd' -d "${MODPATH}/system/bin" >&2
}

extract_services() {
  unzip -o "${ZIPFILE}" 'service.sh' 'system.prop' 'uninstall.sh' -d "${MODPATH}" >&2
}

set_permissions() {
  set_perm_recursive "${MODPATH}" 0 0 0755 0644
  set_perm "${MODPATH}/system/bin/cluxd" 0 0 0755 u:object_r:system_file:s0
  set_perm "${MODPATH}/service.sh" 0 0 0755 u:object_r:script_exec:s0
  set_perm "${MODPATH}/uninstall.sh" 0 0 0755
}

cleanup() {
  rm -rf "${MODPATH}/common"
}

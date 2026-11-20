#!/bin/sh
# Prints the adb serial of the Android device to install on: $ANDROID_SERIAL when it is set (and that
# device is attached), otherwise the only attached device. Several devices and no ANDROID_SERIAL, or
# none at all, is an error that lists what adb sees. What goes wrong goes to stderr: stdout is only
# the serial.
#
#   serial=$(./scripts/android-device.sh)
set -eu

if ! command -v adb >/dev/null 2>&1; then
  echo "android: no adb on PATH — it is in \$ANDROID_HOME/platform-tools (mise.toml puts it there)" >&2
  exit 1
fi

devices=$(adb devices | awk 'NR > 1 && $2 == "device" { print $1 }')

if [ -n "${ANDROID_SERIAL:-}" ]; then
  if printf '%s\n' "$devices" | grep -qx "$ANDROID_SERIAL"; then
    echo "$ANDROID_SERIAL"
    exit 0
  fi
  echo "android: ANDROID_SERIAL=$ANDROID_SERIAL is not attached; adb sees:" >&2
  adb devices -l >&2
  exit 1
fi

count=$(printf '%s\n' "$devices" | grep -c . || true)
if [ "$count" -eq 1 ]; then
  echo "$devices"
  exit 0
fi
if [ "$count" -eq 0 ]; then
  echo "android: no device attached (adb devices lists none ready)" >&2
else
  echo "android: $count devices attached; pick one with ANDROID_SERIAL=<serial>:" >&2
fi
adb devices -l >&2
exit 1

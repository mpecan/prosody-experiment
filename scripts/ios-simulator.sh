#!/bin/sh
# Prints the UDID of the iOS simulator to run the app on: $PROSODY_IOS_SIMULATOR when it is set — a
# UDID, or a device's name — and otherwise an "iPhone 17"; of a name, the device with that name on the
# newest iOS runtime that has one. None found is an error that lists the iPhones there are. What goes
# wrong goes to stderr: stdout is only the UDID.
#
#   udid=$(./scripts/ios-simulator.sh)
set -eu

if ! command -v xcrun >/dev/null 2>&1 || ! xcrun simctl help >/dev/null 2>&1; then
  echo "ios: no simctl — install Xcode, then: sudo xcode-select -s /Applications/Xcode.app" >&2
  exit 1
fi

wanted="${PROSODY_IOS_SIMULATOR:-iPhone 17}"
devices=$(xcrun simctl list devices available)

# A UDID as it is.
if printf '%s\n' "$devices" | grep -q "($wanted)"; then
  echo "$wanted"
  exit 0
fi

# `simctl list` lists the runtimes oldest first, so the last device of that name is on the newest.
udid=$(printf '%s\n' "$devices" | awk -v name="$wanted" '
  index($0, "    " name " (") == 1 {
    rest = substr($0, length(name) + 7)
    print substr(rest, 1, index(rest, ")") - 1)
  }' | tail -n 1)
if [ -n "$udid" ]; then
  echo "$udid"
  exit 0
fi

echo "ios: no simulator called \"$wanted\" — create one in Xcode (Window > Devices and Simulators), or pick one with PROSODY_IOS_SIMULATOR=<name or UDID>:" >&2
printf '%s\n' "$devices" | grep -e '^--' -e 'iPhone' >&2
exit 1

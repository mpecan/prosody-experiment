#!/bin/sh
# Prints the UDID of the iPhone or iPad to install the app on: $PROSODY_IOS_DEVICE when it is set — a
# UDID, or a device's name — and otherwise the only paired physical iPhone. None, or several and no
# PROSODY_IOS_DEVICE, is an error that lists the physical devices there are. Whether the device is
# reachable right now is left to devicectl, which says so itself: its tunnel state reads "unavailable"
# for a phone that is plugged in but asleep. What goes wrong goes to stderr: stdout is only the UDID.
#
#   udid=$(./scripts/ios-device.sh)
set -eu

if ! command -v xcrun >/dev/null 2>&1 || ! xcrun devicectl help >/dev/null 2>&1; then
  echo "ios: no devicectl — install Xcode, then: sudo xcode-select -s /Applications/Xcode.app" >&2
  exit 1
fi

list=$(mktemp)
trap 'rm -f "$list"' EXIT
xcrun devicectl list devices --quiet --json-output "$list" >/dev/null

node - "$list" "${PROSODY_IOS_DEVICE:-}" <<'EOF'
const [file, wanted] = process.argv.slice(2);
const devices = JSON.parse(require('node:fs').readFileSync(file, 'utf8')).result.devices
  .filter((d) => d.hardwareProperties.reality === 'physical')
  .map((d) => ({
    name: d.deviceProperties.name,
    udid: d.hardwareProperties.udid,
    type: d.hardwareProperties.deviceType,
  }));
const listing = () =>
  devices.map((d) => `ios:   ${d.name} (${d.type}, ${d.udid})`).join('\n') || 'ios:   (none)';
const found = wanted
  ? devices.filter((d) => d.udid === wanted || d.name === wanted)
  : devices.filter((d) => d.type === 'iPhone');
if (found.length === 1) {
  console.log(found[0].udid);
} else {
  console.error(
    wanted
      ? `ios: PROSODY_IOS_DEVICE=${wanted} names ${found.length} devices; the physical ones paired here:`
      : found.length === 0
        ? 'ios: no physical iPhone paired here (pair it in Xcode > Window > Devices and Simulators):'
        : `ios: ${found.length} iPhones paired; pick one with PROSODY_IOS_DEVICE=<name or UDID>:`,
  );
  console.error(listing());
  process.exit(1);
}
EOF

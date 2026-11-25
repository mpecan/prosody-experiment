#!/usr/bin/env node
// Generates icons for Prosody on desktop, iOS, Android, and store marketing assets.
import { execSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { opaque } from './opaque-png.mjs';

const root = path.resolve(fileURLToPath(import.meta.url), '../..');
const iconDir = path.join(root, 'store-assets/icon');
const tauriIcons = path.join(root, 'src-tauri/icons');
const iosAppIconSet = path.join(root, 'src-tauri/gen/apple/Assets.xcassets/AppIcon.appiconset');

console.log('1. Generating platform icons with cargo tauri icon...');
execSync(
  `cargo tauri icon "${path.join(iconDir, 'manifest.json')}" -o "${tauriIcons}" --ios-color "#15141c"`,
  { cwd: path.join(root, 'src-tauri'), stdio: 'inherit' }
);

console.log('2. Removing alpha channel from 1024x1024 iOS marketing icon...');
const ios1024 = path.join(iosAppIconSet, 'AppIcon-512@2x.png');
const flat = opaque(fs.readFileSync(ios1024));
fs.writeFileSync(ios1024, flat);
fs.writeFileSync(path.join(iconDir, 'app-icon-ios-1024.png'), flat);

console.log('3. Generating Google Play 512x512 icon...');
execSync(
  `sips -z 512 512 "${path.join(iconDir, 'app-icon-ios-1024.png')}" --out "${path.join(root, 'store-assets/android/en-US/images/icon.png')}"`,
  { stdio: 'ignore' }
);

console.log('✓ Icon generation complete for iOS, Android, Desktop, and Stores.');

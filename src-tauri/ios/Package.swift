// swift-tools-version:5.3
// The app's own iOS plugin (docs/player.md §14): MediaPlugin.swift, the iOS twin of the Android
// project's MediaPlugin.kt and PlaybackService.kt. src-tauri/build.rs builds this package and links
// it into the app's library when the target is iOS, as Tauri builds its own plugins' Swift; `Tauri`
// is the tauri crate's Swift API, which build.rs copies into ../.tauri/tauri-api first.

import PackageDescription

let package = Package(
  name: "prosody-media",
  platforms: [
    .iOS(.v13)
  ],
  products: [
    .library(
      name: "prosody-media",
      type: .static,
      targets: ["ProsodyMedia"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "ProsodyMedia",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)

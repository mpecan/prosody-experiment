// The bridge between the native player (Rust, src-tauri/src/media.rs) and what iOS shows and asks of
// an app that plays music, as a Tauri plugin of the app's own (docs/player.md §14) — the iOS twin of
// the Android project's MediaPlugin.kt and PlaybackService.kt. Rust tells it when the music starts
// and stops; it sends the lock screen's, Control Center's and a headset's buttons back down a channel
// Rust gave it, and stops the music when iOS takes the audio away (a phone call, Siri, an alarm) or
// headphones are pulled out.
//
// The music itself is the native player's (Rust, on a thread of the app, through Core Audio). What
// keeps it going with the screen off is the audio session's `playback` category and the Info.plist's
// `UIBackgroundModes: audio`: iOS lets an app run in the background for as long as it plays.
//
// Prosody has no pause: Play starts the piece from the top and Stop ends it, as on the page. The lock
// screen's Pause stops the music and the lock screen then shows Play, which asks the page to play
// again — only the page can make the piece, so with the page asleep it does nothing, as on Android.
//
// Only the platform's own frameworks are used (AVFoundation, MediaPlayer).

import AVFoundation
import MediaPlayer
import Tauri
import UIKit
import WebKit

class UpdateArgs: Decodable {
  let playing: Bool
  let title: String
}

class ListenArgs: Decodable {
  let channel: Channel
}

class MediaPlugin: Plugin {
  // All of it read and written on the main queue: the commands arrive on Tauri's IPC queue and hop
  // there, and the notifications and remote commands are delivered there.

  /** Where the buttons go: "play", "pause" or "stop", to the native player. */
  private var channel: Channel?
  /** Whether the music plays, as Rust last said. */
  private var playing = false
  private var title = "Prosody"
  private var observers: [NSObjectProtocol] = []

  override func load(webview: WKWebView) {
    onMain { self.setUp() }
  }

  /** Where the buttons go. */
  @objc public func listen(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ListenArgs.self)
    onMain { self.channel = args.channel }
    invoke.resolve()
  }

  /** The music started (with what the lock screen calls it) or stopped. */
  @objc public func update(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(UpdateArgs.self)
    onMain { self.show(playing: args.playing, title: args.title.isEmpty ? "Prosody" : args.title) }
    invoke.resolve()
  }

  private func setUp() {
    // `playback`: heard with the silent switch on, and kept running in the background (with
    // UIBackgroundModes: audio). Not mixed with others: starting it pauses another app's music, as
    // Android's audio focus does, and iOS interrupts it in turn for a call.
    configure()
    let centre = NotificationCenter.default
    let session = AVAudioSession.sharedInstance()
    observers.append(
      centre.addObserver(
        forName: AVAudioSession.interruptionNotification, object: session, queue: .main
      ) { [weak self] note in self?.interrupted(note) })
    observers.append(
      centre.addObserver(
        forName: AVAudioSession.routeChangeNotification, object: session, queue: .main
      ) { [weak self] note in self?.routeChanged(note) })

    let commands = MPRemoteCommandCenter.shared()
    commands.playCommand.addTarget { [weak self] _ in self?.send("play") ?? .commandFailed }
    commands.pauseCommand.addTarget { [weak self] _ in self?.send("pause") ?? .commandFailed }
    commands.stopCommand.addTarget { [weak self] _ in self?.send("stop") ?? .commandFailed }
    commands.togglePlayPauseCommand.addTarget { [weak self] _ in
      guard let self = self else { return .commandFailed }
      return self.send(self.playing ? "pause" : "play")
    }
  }

  /** The session's category, set at start and again at each Play: a web view's own audio may have
   * changed it in between. Setting it to what it already is changes nothing. */
  private func configure() {
    do {
      try AVAudioSession.sharedInstance().setCategory(.playback, mode: .default, options: [])
    } catch {
      Logger.error("prosody: the audio session's category could not be set: \(error)")
    }
  }

  private func show(playing: Bool, title: String) {
    self.playing = playing
    if playing {
      self.title = title
      configure()
      do {
        try AVAudioSession.sharedInstance().setActive(true)
      } catch {
        // Refused (a phone call is on): it does not play over it. The stop comes back here as an
        // update to stopped.
        Logger.error("prosody: the audio session would not start: \(error)")
        _ = send("pause")
      }
    }
    // Left on the lock screen after a stop, showing Play, as Android leaves its notification.
    MPNowPlayingInfoCenter.default().nowPlayingInfo = [
      MPMediaItemPropertyTitle: self.title,
      MPMediaItemPropertyArtist: "Prosody",
      MPNowPlayingInfoPropertyIsLiveStream: true,
      MPNowPlayingInfoPropertyPlaybackRate: playing ? 1.0 : 0.0,
    ]
  }

  /** A phone call, Siri, an alarm: iOS has stopped the output, and the music stops with it. When the
   * interruption ends nothing resumes — Prosody has no pause to resume from — as on Android, where
   * a loss of the audio focus stops it too. */
  private func interrupted(_ note: Notification) {
    guard
      let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
      AVAudioSession.InterruptionType(rawValue: raw) == .began,
      // An app that was suspended hears of that as an interruption when it wakes; it was not
      // playing, or it would not have been suspended.
      playing
    else { return }
    _ = send("pause")
  }

  /** Headphones pulled out (or a Bluetooth headset gone): the music stops rather than carry on out
   * of the speaker, as Android's "becoming noisy" stops it. */
  private func routeChanged(_ note: Notification) {
    guard
      let raw = note.userInfo?[AVAudioSessionRouteChangeReasonKey] as? UInt,
      AVAudioSession.RouteChangeReason(rawValue: raw) == .oldDeviceUnavailable,
      playing
    else { return }
    _ = send("pause")
  }

  private func send(_ action: String) -> MPRemoteCommandHandlerStatus {
    guard let channel = channel else { return .noActionableNowPlayingItem }
    let button: JsonObject = ["action": action]
    channel.send(button)
    return .success
  }

  private func onMain(_ work: @escaping () -> Void) {
    if Thread.isMainThread {
      work()
    } else {
      DispatchQueue.main.async(execute: work)
    }
  }
}

@_cdecl("init_plugin_media")
func initPlugin() -> Plugin {
  return MediaPlugin()
}

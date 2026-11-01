// The phone's own speech engine as one of the reading's voices (`system`, web/src/reading/engines.ts)
// — the iOS twin of the Android project's SpeechPlugin.kt. AVSpeechSynthesizer renders a line with
// `write`, not speaking it, so the player can place it on the beat and mix it as any rendered line;
// it is written to a WAV file in the app's temporary directory, which the Rust side
// (src-tauri/src/speech.rs) reads back. Nothing is downloaded and no model is held in the app: the
// voices are the ones on the phone (the user may add Enhanced and Premium ones in Settings).
//
// The speed is not the utterance's `rate`: measured on Samantha, that moves in steps (0.40 to 0.45
// is one speed, 0.907; 0.46 to 0.50 the next, 1.0) and far from linearly, and may differ from voice
// to voice. The line is said at the voice's own rate and then time-stretched to the speed asked
// for, pitch kept (AVAudioUnitTimePitch, offline), as Android's speech rate does.
//
// Only the platform's own frameworks are used (AVFoundation).

import AVFoundation
import Tauri
import UIKit

class RenderArgs: Decodable {
  let text: String
  let voice: String
  let speed: Float
}

class SpeechPlugin: Plugin {
  // Read and written on the main queue, where the synthesizer's callbacks arrive.
  private var synthesizer: AVSpeechSynthesizer?
  private var count = 0

  /** The voices on the phone, each with its language tag, and the best one in the phone's language. */
  @objc public func voices(_ invoke: Invoke) {
    let usable = AVSpeechSynthesisVoice.speechVoices().filter { voice in
      if #available(iOS 17.0, *) {
        // Novelty voices (Bells, Bubbles…) are effects, not readers; a Personal Voice needs the
        // user's permission, which a reading has no reason to ask.
        return !voice.voiceTraits.contains(.isNoveltyVoice) && !voice.voiceTraits.contains(.isPersonalVoice)
      }
      return true
    }
    let language = AVSpeechSynthesisVoice.currentLanguageCode()
    let fallback = AVSpeechSynthesisVoice(language: language)?.identifier
    let best = usable.filter { $0.language == language }
      .max { a, b in a.quality.rawValue != b.quality.rawValue ? a.quality.rawValue < b.quality.rawValue : b.identifier == fallback }
    let list = usable.sorted { $0.identifier < $1.identifier }.map { voice -> [String: Any] in
      ["id": voice.identifier, "lang": voice.language, "quality": voice.quality.rawValue]
    }
    invoke.resolve(["voices": list, "default": (best?.identifier ?? fallback) as Any, "engine": "AVSpeechSynthesizer"])
  }

  /** Renders `text` in `voice` at `speed` (1 the voice's own) to a WAV file, answering its path. */
  @objc public func render(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(RenderArgs.self)
    DispatchQueue.main.async {
      guard let voice = AVSpeechSynthesisVoice(identifier: args.voice) else {
        invoke.reject("the phone has no voice \(args.voice)")
        return
      }
      let synthesizer = self.synthesizer ?? AVSpeechSynthesizer()
      self.synthesizer = synthesizer
      let utterance = AVSpeechUtterance(string: args.text)
      utterance.voice = voice
      utterance.rate = AVSpeechUtteranceDefaultSpeechRate
      if #available(iOS 14.0, *) { utterance.prefersAssistiveTechnologySettings = false }
      self.count += 1
      let file = FileManager.default.temporaryDirectory.appendingPathComponent("speech-line-\(self.count).wav")
      var samples: [Float] = []
      var rate = 0.0
      var done = false
      synthesizer.write(utterance) { buffer in
        guard !done, let pcm = buffer as? AVAudioPCMBuffer else { return }
        // The last buffer is empty: the line is all here.
        if pcm.frameLength == 0 {
          done = true
          guard rate > 0, !samples.isEmpty else {
            invoke.reject("the phone's speech engine gave no audio for the line")
            return
          }
          DispatchQueue.global(qos: .userInitiated).async {
            do {
              let stretched = try SpeechPlugin.stretched(samples, rate: rate, speed: args.speed)
              try SpeechPlugin.writeWav(stretched, rate: rate, to: file)
              invoke.resolve(["path": file.path])
            } catch {
              invoke.reject("the line could not be made: \(error.localizedDescription)")
            }
          }
          return
        }
        rate = pcm.format.sampleRate
        samples.append(contentsOf: SpeechPlugin.firstChannel(pcm))
      }
    }
  }

  /** Lets the synthesizer go (the page is hidden and silent); the next line makes another. */
  @objc public func release(_ invoke: Invoke) {
    DispatchQueue.main.async {
      self.synthesizer?.stopSpeaking(at: .immediate)
      self.synthesizer = nil
      invoke.resolve()
    }
  }

  /** A buffer's first channel as floats, from float or 16-bit samples, the two `write` gives. */
  private static func firstChannel(_ pcm: AVAudioPCMBuffer) -> [Float] {
    let frames = Int(pcm.frameLength)
    if let data = pcm.floatChannelData {
      return Array(UnsafeBufferPointer(start: data[0], count: frames))
    }
    if let data = pcm.int16ChannelData {
      return UnsafeBufferPointer(start: data[0], count: frames).map { Float($0) / 32768 }
    }
    return []
  }

  /** `samples` played `speed` times as fast, pitch kept; as they are at 1. */
  private static func stretched(_ samples: [Float], rate: Double, speed: Float) throws -> [Float] {
    if abs(speed - 1) < 0.0005 { return samples }
    guard let format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 1),
      let input = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(samples.count))
    else { throw SpeechError.audio("no buffer for the line") }
    input.frameLength = input.frameCapacity
    samples.withUnsafeBufferPointer { source in
      input.floatChannelData![0].update(from: source.baseAddress!, count: samples.count)
    }
    let engine = AVAudioEngine()
    let player = AVAudioPlayerNode()
    let pitch = AVAudioUnitTimePitch()
    pitch.rate = max(1.0 / 32, min(32, speed))
    engine.attach(player)
    engine.attach(pitch)
    engine.connect(player, to: pitch, format: format)
    engine.connect(pitch, to: engine.mainMixerNode, format: format)
    try engine.enableManualRenderingMode(.offline, format: format, maximumFrameCount: 4096)
    try engine.start()
    player.scheduleBuffer(input, completionHandler: nil)
    player.play()
    // The stretched line's length, and a little more for what the time-stretcher still holds.
    let wanted = Int(Double(samples.count) / Double(pitch.rate)) + Int(rate * 0.1)
    guard let chunk = AVAudioPCMBuffer(pcmFormat: engine.manualRenderingFormat, frameCapacity: 4096) else {
      throw SpeechError.audio("no buffer for the stretched line")
    }
    var out: [Float] = []
    out.reserveCapacity(wanted)
    while out.count < wanted {
      let frames = AVAudioFrameCount(min(4096, wanted - out.count))
      let status = try engine.renderOffline(frames, to: chunk)
      if status != .success { break }
      out.append(contentsOf: firstChannel(chunk))
    }
    player.stop()
    engine.stop()
    return out
  }

  /** `samples` as a mono 32-bit float WAV. */
  private static func writeWav(_ samples: [Float], rate: Double, to file: URL) throws {
    var data = Data()
    func put<T>(_ value: T) { withUnsafeBytes(of: value) { data.append(contentsOf: $0) } }
    let bytes = UInt32(samples.count * 4)
    data.append(contentsOf: Array("RIFF".utf8)); put(UInt32(36 + bytes).littleEndian)
    data.append(contentsOf: Array("WAVE".utf8))
    data.append(contentsOf: Array("fmt ".utf8)); put(UInt32(16).littleEndian)
    put(UInt16(3).littleEndian); put(UInt16(1).littleEndian)   // float, mono
    put(UInt32(rate).littleEndian); put(UInt32(rate * 4).littleEndian)
    put(UInt16(4).littleEndian); put(UInt16(32).littleEndian)
    data.append(contentsOf: Array("data".utf8)); put(bytes.littleEndian)
    samples.withUnsafeBytes { data.append(contentsOf: $0) }   // little-endian, as every iPhone is
    try data.write(to: file)
  }
}

enum SpeechError: LocalizedError {
  case audio(String)
  var errorDescription: String? {
    switch self { case .audio(let text): return text }
  }
}

@_cdecl("init_plugin_speech")
func initSpeechPlugin() -> Plugin {
  return SpeechPlugin()
}

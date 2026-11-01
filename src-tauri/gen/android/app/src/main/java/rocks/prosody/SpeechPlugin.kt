package rocks.prosody

// The phone's own speech engine as one of the reading's voices (`system`, web/src/reading/engines.ts):
// Android's TextToSpeech, rendered to a WAV file a line at a time rather than spoken, so the player
// can place it on the beat and mix it as it does a neural voice's line. The WebView has no speech
// synthesis of its own, so without this a line nobody has rendered is not read at all. Nothing is
// downloaded or held in the app: the engine (Google's, or the maker's) runs in its own process, with
// the voices the phone already has. The Rust side (src-tauri/src/speech.rs) reads the file back.

import android.app.Activity
import android.os.Bundle
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.speech.tts.Voice
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger

@InvokeArg
class RenderArgs {
  var text: String = ""
  var voice: String = ""
  var speed: Float = 1f
}

@TauriPlugin
class SpeechPlugin(private val activity: Activity) : Plugin(activity) {
  private var tts: TextToSpeech? = null
  private var ready = false
  /** Calls made before the engine has started: answered once it has (or has failed to). */
  private val starting = mutableListOf<(String?) -> Unit>()
  /** Each line being rendered, by its utterance id: its call and its file. */
  private val rendering = ConcurrentHashMap<String, Pair<Invoke, File>>()
  private val count = AtomicInteger()

  /** Runs `then` once the engine is up, with an error if it could not start. */
  @Synchronized
  private fun started(then: (String?) -> Unit) {
    if (ready) { then(null); return }
    starting.add(then)
    if (tts != null) return
    tts = TextToSpeech(activity.applicationContext) { status ->
      synchronized(this) {
        val error = if (status == TextToSpeech.SUCCESS) null else "the phone's speech engine did not start ($status)"
        ready = error == null
        if (!ready) { tts?.shutdown(); tts = null }
        else tts?.setOnUtteranceProgressListener(listener)
        starting.forEach { it(error) }
        starting.clear()
      }
    }
  }

  private val listener = object : UtteranceProgressListener() {
    override fun onStart(id: String) {}
    override fun onDone(id: String) {
      val (invoke, file) = rendering.remove(id) ?: return
      val answer = JSObject()
      answer.put("path", file.absolutePath)
      invoke.resolve(answer)
    }
    @Deprecated("the one-argument form, which the engine may still call")
    override fun onError(id: String) = onError(id, TextToSpeech.ERROR)
    override fun onError(id: String, code: Int) {
      val (invoke, file) = rendering.remove(id) ?: return
      file.delete()
      invoke.reject("the phone's speech engine could not read the line ($code)")
    }
  }

  // A voice the reading may use: one on the phone (not needing the network, which an offline app
  // cannot count on) and installed (an engine lists voices it would first have to download).
  private fun usable(voice: Voice) = !voice.isNetworkConnectionRequired &&
    !voice.features.contains(TextToSpeech.Engine.KEY_FEATURE_NOT_INSTALLED)

  /** The voices on the phone, each with its language tag, and which one the phone reads with. */
  @Command
  fun voices(invoke: Invoke) {
    started { error ->
      val engine = tts
      if (error != null || engine == null) { invoke.reject(error ?: "the phone's speech engine is not running"); return@started }
      val list = JSArray()
      for (voice in (engine.voices ?: emptySet()).filter(::usable).sortedBy { it.name }) {
        val entry = JSObject()
        entry.put("id", voice.name)
        entry.put("lang", voice.locale.toLanguageTag())
        // Android names a voice only by an id (en-us-x-iob-local): the page names it by its language.
        entry.put("rank", voice.quality)
        entry.put("good", voice.quality >= Voice.QUALITY_HIGH)
        list.put(entry)
      }
      val answer = JSObject()
      answer.put("voices", list)
      answer.put("default", engine.defaultVoice?.name)
      answer.put("engine", engine.defaultEngine)
      invoke.resolve(answer)
    }
  }

  /** Renders `text` in `voice` at `speed` (1 the voice's own) to a WAV file, answering its path. */
  @Command
  fun render(invoke: Invoke) {
    val args = invoke.parseArgs(RenderArgs::class.java)
    started { error ->
      val engine = tts
      if (error != null || engine == null) { invoke.reject(error ?: "the phone's speech engine is not running"); return@started }
      val voice = engine.voices?.firstOrNull { it.name == args.voice }
      if (voice == null) { invoke.reject("the phone has no voice ${args.voice}"); return@started }
      val id = "line-${count.incrementAndGet()}"
      val file = File(activity.cacheDir, "speech-$id.wav")
      rendering[id] = Pair(invoke, file)
      // The voice and rate go with the request as it is queued, so lines asked for in turn keep theirs.
      synchronized(this) {
        engine.voice = voice
        engine.setSpeechRate(args.speed)
        engine.setPitch(1f)
        if (engine.synthesizeToFile(args.text, Bundle(), file, id) != TextToSpeech.SUCCESS) {
          rendering.remove(id)
          invoke.reject("the phone's speech engine would not take the line")
        }
      }
    }
  }

  /** Lets the engine go (the page is hidden and silent); the next call starts it again. */
  @Command
  fun release(invoke: Invoke) {
    synchronized(this) {
      tts?.shutdown()
      tts = null
      ready = false
      for ((_, pending) in rendering) { pending.second.delete(); pending.first.reject("released") }
      rendering.clear()
    }
    invoke.resolve()
  }
}

package rocks.prosody

// The phone's own speech engine as one of the reading's voices (`system`, web/src/reading/engines.ts):
// Android's TextToSpeech, rendered to a WAV file a line at a time rather than spoken, so the player
// can place it on the beat and mix it as it does a neural voice's line. The WebView has no speech
// synthesis of its own, so without this a line nobody has rendered is not read at all. Nothing is
// downloaded or held in the app: the engine (Google's, or the maker's) runs in its own process, with
// the voices the phone already has. The Rust side (src-tauri/src/speech.rs) reads the file back.

import android.app.Activity
import android.os.Bundle
import android.os.Handler
import android.os.Looper
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
  /** Set by release() until the page lists the voices again: a render sent before the release can
   * arrive after it (each call has a thread of its own, speech.rs), and must not start an engine
   * the page has let go. */
  private var released = false
  private val watchdog = Handler(Looper.getMainLooper())

  /** Runs `then` once the engine is up, with an error if it could not start. A render (`render`)
   * is refused once the engine was let go, in the same lock that would start a new one, so a line
   * that crosses a release cannot start an engine behind the hidden page's back; the next voices()
   * (the page shown again, or Play) starts it. */
  @Synchronized
  private fun started(render: Boolean, then: (String?) -> Unit) {
    if (render && released) { then("released"); return }
    if (!render) released = false
    if (ready) { then(null); return }
    starting.add(then)
    if (tts != null) return
    // With no engine to bind, TextToSpeech answers from inside its constructor, before it is
    // assigned: that answer is kept and handled once it is. An engine released before its answer
    // came must not mark its successor ready.
    var made: TextToSpeech? = null
    var early: Int? = null
    val engine = TextToSpeech(activity.applicationContext) { status ->
      synchronized(this) {
        val mine = made
        if (mine == null) early = status
        else if (tts === mine) answered(mine, status)
      }
    }
    made = engine
    tts = engine
    early?.let { answered(engine, it) }
  }

  /** The engine's answer to starting: every call waiting on it is told. Called holding the lock. */
  private fun answered(engine: TextToSpeech, status: Int) {
    val error = if (status == TextToSpeech.SUCCESS) null else "the phone's speech engine did not start ($status)"
    if (error == null) { ready = true; engine.setOnUtteranceProgressListener(listener) }
    else { engine.shutdown(); tts = null }
    starting.forEach { it(error) }
    starting.clear()
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
    started(render = false) { error ->
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
    started(render = true) { error ->
      val engine = tts
      if (error != null || engine == null) { invoke.reject(error ?: "the phone's speech engine is not running"); return@started }
      val voice = engine.voices?.firstOrNull { it.name == args.voice }
      if (voice == null) { invoke.reject("the phone has no voice ${args.voice}"); return@started }
      val id = "line-${count.incrementAndGet()}"
      val file = File(activity.cacheDir, "speech-$id.wav")
      rendering[id] = Pair(invoke, file)
      // An engine whose process dies mid-line (it crashed, or was updated) sends neither onDone nor
      // onError: without this the line, and every line queued behind it on the page, waits for good.
      watchdog.postDelayed({ lost(id) }, 15_000L + 100L * args.text.length)
      // The voice and rate go with the request as it is queued, so lines asked for in turn keep theirs.
      synchronized(this) {
        engine.voice = voice
        engine.setSpeechRate(args.speed)
        engine.setPitch(1f)
        if (engine.synthesizeToFile(args.text, Bundle(), file, id) != TextToSpeech.SUCCESS) {
          rendering.remove(id)
          file.delete()   // made before the engine refused the line
          invoke.reject("the phone's speech engine would not take the line")
        }
      }
    }
  }

  /** Lets the engine go (the page is hidden and silent); the next call starts it again. */
  @Command
  fun release(invoke: Invoke) {
    synchronized(this) {
      released = true
      letGo("released")
    }
    invoke.resolve()
  }

  /** A line the engine never answered: rejected, and the engine let go as release() lets it go, so
   * the next call starts a fresh one. */
  private fun lost(id: String) {
    synchronized(this) {
      val (invoke, file) = rendering.remove(id) ?: return
      file.delete()
      invoke.reject("the phone's speech engine stopped answering")
      letGo("the phone's speech engine stopped answering")
    }
  }

  /** The engine shut down and every line it was rendering rejected with `why`. Called holding the lock. */
  private fun letGo(why: String) {
    tts?.shutdown()
    tts = null
    ready = false
    for ((_, pending) in rendering) { pending.second.delete(); pending.first.reject(why) }
    rendering.clear()
    // Calls waiting on an engine still starting: its answer, if it comes, is ignored (started).
    starting.forEach { it(why) }
    starting.clear()
  }
}

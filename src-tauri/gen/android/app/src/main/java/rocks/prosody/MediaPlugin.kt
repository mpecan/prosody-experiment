package rocks.prosody

// The bridge between the native player (Rust, src-tauri/src/media.rs) and the phone's media
// notification (PlaybackService), as a Tauri plugin of the app's own (docs/player.md §13). Rust
// tells it when the music starts and stops; it sends the notification's, the lock screen's and a
// headset's buttons back down a channel Rust gave it.

import android.app.Activity
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class UpdateArgs {
  var playing: Boolean = false
  var title: String = ""
}

@InvokeArg
class ListenArgs {
  lateinit var channel: Channel
}

@TauriPlugin
class MediaPlugin(private val activity: Activity) : Plugin(activity) {
  /** Where the buttons go: "play", "pause" or "stop", to the native player. */
  @Command
  fun listen(invoke: Invoke) {
    val args = invoke.parseArgs(ListenArgs::class.java)
    PlaybackService.listener = { action ->
      val button = JSObject()
      button.put("action", action)
      args.channel.send(button)
    }
    invoke.resolve()
  }

  /** The music started (with what the notification calls it) or stopped. */
  @Command
  fun update(invoke: Invoke) {
    val args = invoke.parseArgs(UpdateArgs::class.java)
    val context = activity.applicationContext
    activity.runOnUiThread {
      PlaybackService.update(context, args.playing, args.title.ifEmpty { "Prosody" })
    }
    invoke.resolve()
  }
}

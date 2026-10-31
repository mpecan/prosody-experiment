package rocks.prosody

// What lets Prosody play with the screen off (docs/player.md §13). Android stops, or freezes, an
// app that has nothing in the foreground; an app that plays music says so with a foreground service
// of type mediaPlayback and the media notification that comes with it. The music itself is the
// native player's (Rust, on a thread of the app, through AAudio): this service only keeps the app
// alive and awake while it plays, shows what plays, and hands the buttons — the notification's,
// the lock screen's, a headset's — to the native player through MediaPlugin.
//
// Prosody has no pause: Play starts the piece from the top and Stop ends it, as on the page. The
// notification's Pause stops the music and stays, showing Play, which asks the page to play again.
//
// Only the platform's own media classes are used (MediaSession, Notification.MediaStyle), so the
// app gains no library for this.

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.ServiceInfo
import android.graphics.drawable.Icon
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.IBinder
import android.os.PowerManager

class PlaybackService : Service() {
  companion object {
    private const val CHANNEL = "playback"
    private const val NOTIFICATION = 1
    private const val UPDATE = "rocks.prosody.UPDATE"
    private const val PLAY = "rocks.prosody.PLAY"
    private const val PAUSE = "rocks.prosody.PAUSE"

    /** Where a button goes: set by MediaPlugin to the native player's channel. */
    @Volatile
    var listener: ((String) -> Unit)? = null

    /** The service while it runs: a stop reaches it directly, since an app in the background may
     * not start a service, and by then the app may be. */
    private var running: PlaybackService? = null

    /** The music started or stopped. Called on the main thread. */
    fun update(context: Context, playing: Boolean, title: String) {
      val service = running
      if (service != null) {
        service.show(playing, title)
      } else if (playing) {
        // Started for the foreground; it must call startForeground within seconds, and does, in
        // onStartCommand.
        val intent = Intent(context, PlaybackService::class.java)
          .setAction(UPDATE)
          .putExtra("playing", true)
          .putExtra("title", title)
        context.startForegroundService(intent)
      }
    }
  }

  private lateinit var session: MediaSession
  private lateinit var audio: AudioManager
  private var wake: PowerManager.WakeLock? = null
  private var focus: AudioFocusRequest? = null
  private var noisy: BroadcastReceiver? = null
  private var title = "Prosody"

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onCreate() {
    super.onCreate()
    running = this
    audio = getSystemService(AudioManager::class.java)
    val channel = NotificationChannel(CHANNEL, "Playing", NotificationManager.IMPORTANCE_LOW)
    channel.description = "What Prosody is playing, with its controls"
    channel.setShowBadge(false)
    getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
    session = MediaSession(this, "Prosody")
    session.setCallback(object : MediaSession.Callback() {
      override fun onPlay() { listener?.invoke("play") }
      override fun onPause() { listener?.invoke("pause") }
      override fun onStop() { listener?.invoke("stop") }
    })
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    when (intent?.action) {
      UPDATE -> show(intent.getBooleanExtra("playing", false), intent.getStringExtra("title") ?: title)
      PLAY -> listener?.invoke("play")
      PAUSE -> listener?.invoke("pause")
    }
    return START_NOT_STICKY
  }

  /** Shows the music playing, called `title`, or stopped. */
  fun show(playing: Boolean, title: String) {
    if (playing) this.title = title
    val state = PlaybackState.Builder()
      .setActions(
        PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or
          PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_STOP
      )
      .setState(
        if (playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED,
        PlaybackState.PLAYBACK_POSITION_UNKNOWN,
        if (playing) 1f else 0f
      )
      .build()
    session.setPlaybackState(state)
    session.setMetadata(
      MediaMetadata.Builder()
        .putString(MediaMetadata.METADATA_KEY_TITLE, this.title)
        .putString(MediaMetadata.METADATA_KEY_ARTIST, "Prosody")
        .build()
    )
    session.isActive = true
    val notification = notification(playing)
    if (playing) {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        startForeground(NOTIFICATION, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK)
      } else {
        startForeground(NOTIFICATION, notification)
      }
      hold()
    } else {
      // Out of the foreground, the notification left behind with Play on it.
      stopForeground(STOP_FOREGROUND_DETACH)
      getSystemService(NotificationManager::class.java).notify(NOTIFICATION, notification)
      release()
    }
  }

  private fun notification(playing: Boolean): Notification {
    val open = PendingIntent.getActivity(
      this, 0,
      Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
    )
    val action = if (playing) {
      Notification.Action.Builder(
        Icon.createWithResource(this, android.R.drawable.ic_media_pause), "Stop", button(PAUSE)
      ).build()
    } else {
      Notification.Action.Builder(
        Icon.createWithResource(this, android.R.drawable.ic_media_play), "Play", button(PLAY)
      ).build()
    }
    return Notification.Builder(this, CHANNEL)
      .setSmallIcon(android.R.drawable.ic_media_play)
      .setContentTitle(title)
      .setContentText(if (playing) "Playing" else "Stopped")
      .setContentIntent(open)
      .setOngoing(playing)
      .setVisibility(Notification.VISIBILITY_PUBLIC)
      .addAction(action)
      .setStyle(Notification.MediaStyle().setMediaSession(session.sessionToken).setShowActionsInCompactView(0))
      .build()
  }

  private fun button(action: String): PendingIntent = PendingIntent.getService(
    this, action.hashCode(),
    Intent(this, PlaybackService::class.java).setAction(action),
    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
  )

  /** While it plays: the CPU kept awake for the render thread, which renders ahead of the output
   * and must run between the output's own wake-ups; the audio focus, so other players pause (and
   * this one stops when another takes it); and headphones pulled out stop it, as they should. */
  private fun hold() {
    if (wake == null) {
      wake = getSystemService(PowerManager::class.java)
        .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "prosody:playback")
        .apply { setReferenceCounted(false); acquire() }
    }
    if (focus == null) {
      val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
        .setAudioAttributes(
          AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_MEDIA)
            .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
            .build()
        )
        .setOnAudioFocusChangeListener { change ->
          if (change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) {
            listener?.invoke("pause")
          }
        }
        .build()
      audio.requestAudioFocus(request)
      focus = request
    }
    if (noisy == null) {
      val receiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) { listener?.invoke("pause") }
      }
      val filter = IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY)
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        // A system broadcast reaches a receiver that takes none from other apps.
        registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED)
      } else {
        registerReceiver(receiver, filter)
      }
      noisy = receiver
    }
  }

  private fun release() {
    wake?.release()
    wake = null
    focus?.let { audio.abandonAudioFocusRequest(it) }
    focus = null
    noisy?.let { unregisterReceiver(it) }
    noisy = null
  }

  /** The app swiped away: the music stops with it. */
  override fun onTaskRemoved(rootIntent: Intent?) {
    listener?.invoke("stop")
    release()
    stopSelf()
    super.onTaskRemoved(rootIntent)
  }

  override fun onDestroy() {
    release()
    session.release()
    getSystemService(NotificationManager::class.java).cancel(NOTIFICATION)
    running = null
    super.onDestroy()
  }
}

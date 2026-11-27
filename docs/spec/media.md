# Media controls and picture-in-picture

Magnetite knows what each tab plays. A tab you are not looking at gets a
row of controls in the sidebar, a tab with sound gets a mute button, and a
playing video follows you into a floating window when you leave its tab.

## Media rows

A row appears above the favorites for each tab that has media and is not on
screen, in any space. There are at most two rows, playing ones first.

| Control | Action |
|---|---|
| Source (icon and title) | Jumps to the tab |
| Previous | The page's "previous track" action; without one, back to the start |
| Play / Pause | Through the page's own handler when it has one |
| Next | Only when the page offers a "next track" action |
| Mute | Mutes or unmutes the tab |
| Picture-in-picture | Videos only: pops the video out, or puts it back |

- The title is the page's Media Session title, else the tab's title. The
  tooltip adds the artist.
- A tab row shows a speaker button while the tab plays media (see "What
  counts as media") or is muted; it mutes and unmutes the tab. The mute is forgotten when the tab is
  unloaded.

## What counts as media

- An audio or video element that is audible (not muted, volume above zero)
  and really played its current source.
- A muted video too, while it plays or is in the floating window, so that
  it can pop out (Instagram starts its videos muted). Its tab row gets no
  speaker button.
- Not a sound effect, such as a chat's message sound: a clip shorter than
  5 seconds gets no row and no speaker button. It does count when it
  loops (a ringtone), when it is in the floating window, and once it has
  played on for 5 seconds: short clips that follow each other within a
  second count as one (a reader playing a text sentence by sentence), and
  so does a stream of unknown or endless length (a live stream), after its
  first 5 seconds. Media that counted keeps counting when resumed; a new
  source in the same player counts on its own, unless it follows within
  that second.
- Of several paused elements, the one that last played as media keeps the
  row; a sound effect playing in between does not take it away.
- Paused media keeps its row, so it can be resumed. Media that was loaded
  but never started (a queued next video, an ad in a background tab) gets
  none. Ended media loses its row.
- Tabs with media, playing or paused, are never unloaded or auto-archived
  in the background.

## Picture-in-picture

- Settings → General → "Pop out playing videos" (on by default): when a tab
  with a playing video leaves the screen, the video continues in a floating
  window. A page that stays on screen in a split keeps its video.
- Showing the tab again puts the video back into the page, whatever the
  setting says.
- The picture-in-picture button of a media row does both by hand.
- While a page's video is in the floating window, the page reads as
  visible, although its tab is hidden. Sites that pause their video when
  the tab is hidden (Instagram) so keep playing there; once the window
  goes, they pause it as they do in any hidden tab.

On macOS the floating video has Magnetite's own controls:

| Control | Action |
|---|---|
| Back to Tab | Shows the tab, puts the video back into the page and brings Magnetite to the front |
| Minimize | The window goes; the video keeps playing in its tab |
| Close | Pauses the video; the window goes |
| 15 s back, 15 s forward | Seek; live streams have no seek buttons |
| Play / Pause | As in the media row |

- The controls show while the pointer is over the video and briefly when it
  pops out. They hide when the pointer rests for 2.5 s. A narrow window
  (an upright video) shows the top buttons as icons only and moves the
  seek buttons closer to play/pause; under 140 pt it has no seek buttons.
- Dragging the video moves the window. Dragging an edge or corner resizes
  it with the video's proportions, never narrower than 160 pt or wider than
  the screen; the resize cursor shows there, also while another app is in
  front. A two-finger swipe flings it to the screen edge in that
  direction; a diagonal swipe reaches a corner.
- The video window shows no page address.

## How it works

- `media_hook.js` runs in the main frame of every web page (http, https
  and file addresses), in the page's own JavaScript world and in no other
  (not in the worlds extensions run content scripts in), before the page's
  scripts. It wraps `MediaSession.setActionHandler` and
  `HTMLMediaElement.play` to learn the page's handlers and media elements,
  and reports `{playing, title, artist, video, pip, live, silent, can_next,
  can_prev}` on change through a native function only its closure can
  reach. It adds nothing to the page's globals. It holds the elements
  weakly: one the page let go of (a post a feed took out of the page) can
  be freed. It also replaces
  `document.visibilityState` and `document.hidden` so that they say
  "visible" while `document.pictureInPictureElement` is set, and sends the
  page a `visibilitychange` event when a hidden tab's video enters or
  leaves picture-in-picture.
- A report goes on to the sidebar only when the sidebar shows the change:
  sound starting or stopping (the speaker on the tab's row), or media of a
  tab that is not on screen in a window (the media row). A muted video
  starting and stopping in the tab on screen, as a feed's videos do while
  it scrolls, waits for the next state the sidebar gets.
- Commands go back to the renderer as a process message. It calls the
  closure's `control` function: the page's own Media Session handler when
  there is one, else the element.
- Entering picture-in-picture needs a user gesture, so the engine runs
  `Runtime.evaluate` with `userGesture` on CEF's in-process DevTools
  channel. It takes the playing video, else the loaded one most of which
  is in the viewport (a page may have paused it when the tab was hidden). Mute is `CefBrowserHost::SetAudioMuted`; a page muted before it
  has its window is muted as soon as it has one.
- The macOS controls are a nearly clear panel that lies over Chromium's
  video window as its child window. It takes the pointer, so Chromium's own
  controls never show. Chromium would show the page address until the user
  clicks into its window; the feature
  `VideoPipForceTrustedForMediaPlaybackForTesting`, enabled at start, keeps
  it off. As its name says, the feature is meant for testing, so a later
  Chromium version may drop it.
- Reports are untrusted, because the page can shape them. They are capped
  at 4 KiB and built on objects without a prototype. A page gets the
  floating controls only if it has them already, Magnetite asked it into
  picture-in-picture in the last five seconds, or it is on screen
  (`pip::may_own`). A background page cannot claim another page's video.

## Data

Nothing leaves the machine. Title and artist come from the page's own Media
Session metadata, are cut to 200 characters and live only in memory, like
the mute state. The pop-out setting is `settings.auto_pip` in
`<data-dir>/state.json`; it is not synced.

## Not built yet

- Controls for media in iframes (embedded players). The hook runs in main
  frames only.

Code: `crates/engine/src/media.rs`, `media_hook.js`, `pip.rs`, `cef_app.rs`
(the feature switch) and `mac/mod.rs` (swipe); `crates/app/src/app/tabs.rs`
(`follow_pictures`, `in_use`), `crates/app/src/state/tabs.rs`
(`set_media`); `crates/ui/web/src/MediaBar.tsx` and
`Rows.tsx`.

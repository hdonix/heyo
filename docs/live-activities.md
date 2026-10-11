# Live activities (a Dynamic Island): technical design (draft v0.3)

Status: draft, awaiting review. Date: 2026-10-10.

Once the palette is closed, what is going on has nowhere to be shown:
dictation is recording, a timer is running, the AI is still answering, a
background command is at work.
This document proposes "live activities": something in progress, started by
Heyo or by an extension, and shown **in the place most native to the current
desktop environment**.

**What changed from v0.1 to v0.2.** In v0.1 what was said about each
platform was written from memory, and every desktop used the same floating
window. This version is rewritten on four sets of research notes with
sources (the interfaces of the phone platforms, the Linux desktop
environments, similar products that already exist on the desktop, and
window experiments on this technology stack). The notes are in
`research_notes/live-activities/` (`mobile.md`, `linux.md`, `products.md`,
`window.md`, with a link to the source after every fact; the directory is
not in the repository). The main changes:

- The presentation went from "one floating window" to **one interface, and
  one presentation per desktop** (section 5).
- GNOME takes a hybrid: **the collapsed state drawn in the top bar, the
  expanded state right below it** (5.2).
- Five statements of v0.1 that did not match the facts are corrected
  (listed in section 13).
- Three things that several platforms have and v0.1 lacked are added:
  protection of the order of updates, the fallback being chosen by whoever
  starts the activity, and expanding once on first appearance.
- Handling of accidental triggering, screen sharing and Do Not Disturb is
  added; the priorities of the built-in activities are reordered.

Section 12, at the end, has the questions that need your decision.

**What changed from v0.2 to v0.3** (from three rounds of discussion on
2026-10-11, not from new research):

- New section 0: the essential difference between the phone and the
  desktop, and the "real needs on the desktop" that follow from it. The
  desktop is not short of screen space; it is short of attention, so the
  default is quiet.
- **The status bar becomes the main ground**, and carries two kinds of
  thing at once: **status items**, which are permanent (Raycast's menu bar
  extensions), and **activities**, which have a beginning and an end. Menu
  bar extensions go from "a narrowed exception" to first-class citizens
  (sections 3 and 7).
- **The expanded state allows an extension its own interface**: the
  collapsed state still takes only data (the native places of each desktop
  can only draw data), and the expanded state uses our set of components; a
  default card is generated when no interface is written (4.1). The
  non-goal of v0.2, "no arbitrary interface", is withdrawn.
- **The palette gains an "In Progress" section**, for keyboard users (5.0).
- The card that expands on hover is demoted to "an enhancement of the
  indication in the status bar", and is no longer the heart of the feature
  (section 0).
- The expanded interface is drawn with the web view (Flutter is shelved,
  `docs/flutter-evaluation.md`).

---

## 0. Why the island works on the phone, and why the desktop is different

The island on the phone solves four problems at once that only the phone
has: only one application can be seen at a time, and what happens in the
background is entirely invisible; around the camera cutout there is a piece
of screen that was wasted anyway; people are not looking at the phone most
of the time, and the main ground of live activities is in fact the lock
screen; the content is things of the real world (a food delivery, a ride, a
route).

None of the four holds on the desktop:

| Phone | Desktop |
|---|---|
| One application at a time, the background invisible | Several windows, several monitors, a taskbar; what happens in the background can often be seen anyway |
| There are wasted pixels to use | Every pixel has an owner: the middle of the top is title bars, tabs, the clock |
| A glance now and then | Hours of looking at the screen without a break; what is scarce is attention, and the cost of interrupting is far higher |
| Events of the real world | Mostly work the computer itself is doing: builds, downloads, AI tasks, meetings, recordings |

There are also three constraints only the desktop has: the keyboard focus
is sacred; we are not the operating system, and all we can put in is the
content of Heyo itself and of Heyo's extensions; the screen gets shared.

From this, the real needs on the desktop are three things, not "watching
the progress all the time": **tell me when it is done or has gone wrong**;
**call me when I am needed**; **let me see it all the time while a sensitive
resource such as the microphone is in use**. Heyo's users are also people
who call up the palette from the keyboard, and something that can only be
expanded by hovering with the mouse goes against their habits.

So in this design the "activity" is the data model (the life cycle from the
phone, timers the host counts by itself, limits, being started by the user:
all of it applies on the desktop), and showing it is divided among four
outlets that each have their own job: a small indication in the status bar
(the minimum that is always visible); the "In Progress" section in the
palette (reachable from the keyboard); an alert when it ends or needs
dealing with; brief feedback on an action. The card that expands on hover
is only an enhancement of the first outlet. The default is quiet: a task
shorter than a few seconds is not shown, something in progress takes only
one icon, and only a change of state actively draws attention; the weight
of the actions is on "take me there" (focus the window concerned, or open
that command), and buttons come second.

The desktop also has two opportunities the phone does not, which this
design records but does not implement in the first version: handing the
activity data to the user's own status bar (users of Hyprland and Sway all
have one; a command or a D-Bus interface would do); gathering the progress
other applications already broadcast (taskbar progress, media playback,
KDE's jobs). The latter decides whether this feature is "Heyo's status
display" or "the desktop's activity center", and needs research of its own.

---

## 1. What the research found

### 1.1 How the phone platforms do it (`mobile.md`)

| | Is the content interface or data | Who counts timers and progress | Limit on duration | Several activities at once |
|---|---|---|---|---|
| iOS ActivityKit | Interface (SwiftUI), the only exception | The system (text and progress that advance by themselves from a start and an end time) | The activity 8 hours, then 4 more hours on the lock screen; attributes plus state 4 KB in all | Both are shown in the minimal state; relevance only orders within one application |
| Android 16 Live Updates | Data (a progress-style notification); a notification with a custom interface cannot be promoted | The system | — | The system decides which one is promoted |
| Huawei Live View | Data, five templates: progress, emphasized text, left and right text, score, navigation | The system (a timer placeholder) | 8 hours; the capsule is hidden after 2 idle hours and the activity ends after 4 | — |
| Xiaomi Super Island (focus notifications) | Data, templates | The system | — | — |

Four points that bear directly on the design:

1. **Taking only data is the mainstream**, not a compromise. Google states
   in so many words that a notification with a custom interface cannot be
   promoted. iOS is the only one to allow a free interface, and the price
   is that the interface has to be archived and animations are limited to
   2 seconds.
2. **Timers counted by the system is what every platform does.** The shape
   iOS has is the cleanest: a span from a start to an end time, plus an
   optional moment of pausing.
3. **"Stale" and "ended" are two things.** The stale date of iOS only marks
   the content as possibly inaccurate and does not end the activity; Huawei
   degrades in two steps.
4. **Starting has to be triggered by the user**: Apple and Huawei require
   the application to be in the foreground or a clear intent of the user,
   and Android excludes "permanent ambient information".

Could not be verified: iOS's quota for update frequency and its limit on
concurrent activities are not published officially; what is said of OPPO
and vivo comes from third-party documents; no developer documentation was
found for Honor.

### 1.2 Similar products that already exist on the desktop (`products.md`)

- **Not one product with a user base floats a capsule below the bar.** The
  similar applications on macOS all sit against the top edge of the screen,
  in the band the menu bar is in; Apple put the iPhone's live activities
  into the Mac's menu bar; the only similar extension on GNOME replaces the
  clock in the top bar; the most-starred project on Windows embeds itself
  in the taskbar.
- **The position v0.1 chose would collide on GNOME**: notification banners
  appear centered right below the top bar.
- **Content ordered by the evidence**: media that is playing > timers and
  focus > brief system feedback (volume, copied) > long tasks with a result
  (downloads, AI tasks) > the state of audio and screen recording > the
  next meeting. The states of an AI task that are worth something are "in
  progress", "needs you to confirm or type" and "done".
- **What is complained about most**: power use when idle (the worst for
  reputation), expanding by accident while a tab is dragged, covering the
  content below, fullscreen detection failing, springy shape-changing
  animations.
- **The only third-party desktop island interface that has shipped so far**
  (AtollExtensionKit) also chose, independently, "described by data, fixed
  slots, countdowns counted by the host".
- **Hover times**: 300 ms to expand and 600 ms to collapse, which agrees
  with the defaults of existing products and with what usability research
  recommends.

### 1.3 The Linux desktop environments (`linux.md`)

- **GNOME already has a native live activity, and it is the top bar item**:
  the screen recording indicator is a button in the top bar, with a timer
  the Shell counts by itself, and a click stops it. The extension interface
  allows adding items to the middle area of the top bar (on GNOME 45 and
  on 50), and the review rules do not forbid it.
- **KDE has a job progress interface** (`org.kde.JobViewServerV2` /
  `JobViewV3`): a title, one line of description, a percentage, cancel and
  pause; Plasma mirrors the progress onto the taskbar icon.
- **Every mainstream Wayland compositor except GNOME's implements the
  layer-shell protocol** (KWin, Hyprland, niri, COSMIC, Sway, labwc,
  river); Mutter does not.
- **Notifications cannot carry progress**: the notification specification
  has no progress field, and neither GNOME nor Plasma shows it.
- **Cheap channels that work across desktops**: the progress bar on a Dock
  icon (`com.canonical.Unity.LauncherEntry`, which Dash to Dock, the Plasma
  taskbar and Plank all honor), and the tray title.

### 1.4 What this technology stack can do (`window.md`, with experiments on this machine)

Measured on a separate virtual display (xfwm4, and an X11 session of GNOME
Shell 46):

- **Click-through holds on X11**: with a transparent window of fixed size,
  once an input region is set on the top-level window, clicks outside the
  region land on the application below; hovering, clicks and the wheel all
  arrive without the window gaining focus. Changing the region once takes
  about 36 microseconds.
- **On X11, use the "dock" window type and do not set keep-above**: it is
  above ordinary windows, sinks by itself when it meets a fullscreen
  window, and GNOME does not give it opening and closing animations either.
  A window with keep-above set, or of the "notification" type, covers
  fullscreen windows.
- **A fixed window is better than resizing the window**: with resizing,
  every expanding or collapsing shows one or two wrong frames; a fixed
  window with a changing input region shows none.
- **The cost of rendering depends only on the area that changes, not on the
  size of the window**; it is zero when idle; a timer redraws only one
  small rectangle a second.
- **One premise of v0.1 is wrong**: there are no GPU layers on this
  rendering path, and `transform` is a redraw on the CPU just as changing
  width and height is. What is really expensive is blurred shadows and
  animations that never stop (one small pulsing dot keeps about 8% of a
  core busy).
- **Not possible with an application window on GNOME Wayland**: GTK 3's
  "does not accept focus, keep above, not in the taskbar" are empty
  implementations in the Wayland backend; in the experiment the window took
  the keyboard focus as soon as it was shown. The extension can place it,
  adjust its layer and track fullscreen, but there is no way to make the
  window unfocusable.
- **What v0.1 does on Windows is wrong**: a hit test that returns
  "transparent" only works for windows of the same thread. The top-level
  window should be clipped with a window region (`SetWindowRgn`).
- **Only Windows and macOS have an interface for excluding a window from
  screen capture**; no way was found on X11 or GNOME.

Not measured: Windows, any layer-shell compositor, your real desktop.

### 1.5 The Flutter trial

Drawing the interface with Flutter was tried (on the development machine it
can use the GPU, and the island's change of shape took under 3 ms a frame)
and later shelved; the reasons and the figures are in
`docs/flutter-evaluation.md`.
The conclusion for this design: the expanded interface is drawn as a web
page. The experiments of 1.4 have already confirmed that the cost of the
web view depends only on the area that changes, and is zero when idle.

---

## 2. Goals and non-goals

**Goals**

1. The user can see what is in progress without opening the palette, and
   can take one or two actions on it.
2. In each desktop environment it appears where that environment itself
   shows this kind of thing.
3. Heyo's own activities and those of extensions use the same mechanism.
4. An extension only fills in data, writes no interface, and does not have
   to stay running for a timer to keep counting.
5. It never takes the keyboard focus (except for keyboard actions the user
   triggers on purpose), does not intercept input, and gets out of the way
   of fullscreen.
6. Zero overhead when idle.

**Non-goals**

- It is not a notification center: no history is kept.
- No arbitrary interface in the collapsed state (the reasons are in 4.1);
  the expanded state may have one.
- Permanent information (the weather, an unread count) is not an
  **activity**, but can exist as a **status item** (section 3): the two
  have different rules and the same place.
- v1 does no remote push updates and nothing on the lock screen.

---

## 3. Concepts

- **An item on the status bar**: one concept in the host, which the
  presenters of each desktop only have to draw. It comes from two sources,
  with different rules:
  - **Status item**: permanent; once the user has enabled it, it is always
    there. The source is Raycast's menu bar extensions (`MenuBarExtra`);
    the collapsed state is an icon plus a title, and the expanded state is
    a menu (sections, submenus, icons) or a custom panel. The user decides
    which are shown and in what order; by default the present rule stays
    (a menu bar command only appears after the user has run it once).
  - **Activity**: something with a beginning and an end, whose state
    changes in between. The source is `LiveActivity` and what is built into
    Heyo. It is bound by these rules: a limit on duration, having to be
    started by the user, and not interrupting under Do Not Disturb.
- **Activity**: something with a beginning and an end, whose state changes
  in between.
- **Attributes**: the part that is fixed at the start and does not change
  afterwards. **Content**: the part that changes.
- **Three sizes**: minimal (one icon, possibly with a progress ring),
  compact (an icon plus a small piece of content that changes by itself),
  expanded (a title, a body, at most three buttons).
- **Presenter**: the piece of code that draws "the activities that should
  be shown now" at some place on some desktop. Each session chooses one
  main presenter, and there are several auxiliary channels besides.

---

## 4. Three key trade-offs

### 4.1 The collapsed state takes data; the expanded state can be the extension's own interface

**The collapsed state takes only data.** There is just one decisive reason:
the native places of each desktop (an item in GNOME's top bar, a KDE job
entry, the tray title, a notification) can only draw data, and cannot draw
our components. The other reasons either do not hold on the desktop or can
be solved: saving power matters little on the desktop, and the background
runtime keeps menu bar extensions resident anyway; "an activity has to
outlive the command that started it" is solved by the host keeping the last
rendered result; the appearance is assured by the set of components, and
does not need interfaces to be forbidden.

**The expanded state allows an extension its own interface.** Rendering an
extension's component tree is Heyo's core ability to begin with (lists,
details and forms are all drawn this way), and drawing another small
component tree in a card uses the same pipeline. An extension can put one
together from the existing components (a Markdown detail, metadata, list
rows, images, actions) plus a few new layout components (a row, a column,
text, a progress bar, a button, and a timer component that we count
ourselves). A music extension can draw a cover with playback controls, a CI
extension can draw the state of each stage, and an AI task can show
streamed output and an "Approve" button. An extension that writes no
interface gets a default card generated from the title, the body and the
buttons.

The restrictions that still have to be added, none of them for saving
power: a card has a maximum size (it floats above other applications); the
number of resident extensions has an upper limit (the background runtime
holds at most 6 now, and each takes memory; the number is to be settled
after measuring); in the native places it degrades (a KDE job entry and a
notification show only the data part). The update frequency has to be
limited in the web view (with CPU rendering, smoothness is limited).

The four reasons v0.1 listed, and how they stand on the desktop:

1. Timers are counted by the host, and an extension does not have to stay
   resident (every phone platform does this).
2. An activity can outlive the command that started it.
3. A uniform appearance, at a cost that can be controlled.
4. **Only data can be handed to different presenters**: GNOME's top bar
   item, KDE's job progress, the Dock's progress bar and notifications all
   take data. If the content were a component tree it could only be drawn
   in our own window, and "adapting to each desktop environment" would be
   out of the question.

### 4.2 The state belongs to the host

The table of activities is in the Rust host. When the runtime restarts, an
extension is updated, or the command that started it ends, the activity is
still there.
Declarative actions are carried out by the host directly; for callback
actions the extension's handler command is brought up in the background
when needed (6.5).

### 4.3 One interface, several presentations

An extension sees only one interface. The host has a presenter interface
whose input is "the list of activities, in order of priority", and each
desktop implements it in its own way.
The same activity will look different on different desktops, and this is
intended: what it buys is that the activity appears where the user would
look anyway.

---

## 5. Presentation in each desktop environment

### 5.0 The "In Progress" section in the palette (every desktop)

When the palette is called up, the top of the root search lists the
activities in progress (status items have nothing to do with it): the arrow
keys select, Enter carries out the default action, Ctrl+K shows all the
actions. This is the main outlet for keyboard users; it involves no window
techniques and no adapting to desktops, and should be done first.

### 5.1 Overview

| Session | Compact | Expanded | What the host has to do | What cannot be done, or is a risk |
|---|---|---|---|---|
| **GNOME (X11 and Wayland)** | Heyo's Shell extension draws every item (status items and activities) in the top bar: an icon plus text, a timer or progress | Menus are popped up natively by the Shell; a custom card is our window, right below the top bar item (5.2) | The extension gains a method "replace the whole list of activities" and a signal "an action was clicked" | The focus of the expanded panel under Wayland (5.2); without the extension installed it falls back to the auxiliary channels |
| **KDE Plasma (Wayland), Hyprland, Sway, niri, COSMIC and others** | A layer-shell window: top layer, against the top edge, no keyboard, an input region equal to the shape of the island | The same window | One layer-shell presenter, shared by all of them | The Rust bindings for GTK 3 are no longer maintained, so the C library has to be bound directly; on Hyprland a transparent layer-shell web window is known to grow in memory with redraws; there is no test environment on this machine |
| **Job-like activities on KDE** (downloads, background commands) | The job progress interface as well | A job entry in the notification panel, with cancel | A small D-Bus client | Only cancel and pause; custom buttons have no place |
| **Other X11 desktops** (Xfce, Cinnamon, MATE, i3 and others), and KDE's X11 session | A floating window: dock type, not kept above, does not accept focus, an input region equal to the shape of the island | The same window | What v0.1 did is used only here | Transparency needs a compositor; tiling window managers may need the user to add a floating rule |
| **Windows** | A topmost, non-activating tool window, clipped to the shape of the island with a window region | The same window | One Win32 presenter | None of it has been measured; fullscreen is judged with the system's "busy state" query |

**The auxiliary channels every session has** (they are everything when the
main presenter is not available):

- The progress bar and the count on the Dock icon (about 40 lines, one
  D-Bus signal).
- The tray title shows the compact text of the first activity (Ubuntu's
  tray extension shows it; it is harmless elsewhere).
- Notifications: only for "alerts" and "the ended state", with one
  notification reused for the same activity. **Notifications are not used
  to show progress under way**, because neither GNOME nor Plasma shows
  progress.

### 5.2 GNOME: the collapsed state in the top bar, the expanded state right below it

This was settled as you proposed.

- **The collapsed state** is drawn in the top bar by the Shell extension
  with the Shell's own interface toolkit, modeled on GNOME's screen
  recording indicator. By default it is beside the clock in the middle
  area, and the settings can move it to the right.
  That way it does not take focus by its nature, hides with the top bar in
  fullscreen, does not compete with notification banners for position, and
  stays visible under Do Not Disturb (as GNOME's own indicators do).
- **The expanded state is of two kinds.** The menu of a status item is
  popped up natively by the Shell (keyboard navigation, focus and closing
  on a click outside are all handled by the system, and the icons of menu
  items and submenus can all be shown, far better than the tray today). The
  card of an activity and the custom panel of a status item are one window
  of Heyo's own, which the extension places directly below the top bar
  item, its top edge meeting the bottom edge of the top bar, with the width
  and the horizontal position given by the extension from where the top bar
  item really is.
  The interface of this window is the same one as the expanded interface on
  KDE, X11 and Windows (a web page).
- **The join**: the collapsed state does not move, and the expanded window
  grows downward from the bottom edge of the top bar, so the animations of
  the two processes do not have to be aligned frame by frame. The expanded
  window reads the height and the background color of the top bar to fit
  it.
- **The window stays resident and only its visibility is switched**, and
  the extension turns off the system's opening and closing animations for
  it, to avoid a mapping delay on every expansion.

**The X11 session** (what you use now): the expanded window uses what was
measured in 1.4, and has no focus problem.

**The Wayland session**: an application window cannot refuse focus. For a
custom card this is now considered acceptable: the user clicked the card on
purpose and it gains focus, the same as opening any popover; hovering
without clicking does not take focus; after the click the extension gives
the focus back to the original window. Three ways are still to be tried, in
order from the most to the least certain:

1. The input region of the expanded window is set empty, so it never
   receives the mouse; the extension covers it with a transparent
   interactive area and passes hovering and clicks on to Heyo through the
   existing D-Bus interface. The focus the window gets when first shown is
   given back to the original window by the extension at once.
2. The expanded window receives clicks normally, and after a click the
   extension gives the focus back. The simplest to implement, but for the
   moment of the click the focus leaves the current application.
3. The expanded state is drawn by the extension too, with the Shell's
   popover. There are no focus or joining problems at all; the price is
   that the expanded interface is written separately for GNOME, and what it
   can express is limited by the Shell's components.

If the first two cannot be tuned in the prototype until no flaw shows, the
third is used; the code of the collapsed state is not affected.

**The extension's side** keeps the existing security model: it answers only
the Heyo process. The new top bar items have to be stated in the
extension's description, and all destroyed on `disable()`, so as to pass
review.

### 5.3 Position

"Floating centered below the top bar" of v0.1 is dropped. The principle is
now: **where there is a bar, go into the bar; where there is no bar to go
into, sit against the edge of the screen, and do not hang in the middle.**

- GNOME: in the top bar.
- layer-shell and X11 floating windows: centered against the top edge of
  the screen, the top edge coinciding with the edge of the screen (like
  those applications on macOS); the settings can change this to the bottom
  edge, or offset it left or right to keep clear of the user's own status
  bar.
- Windows: centered against the top edge of the screen. Embedding in the
  taskbar is the more native position, but how it is implemented (the way
  another project does it) has not been researched enough to conclude
  anything, and it is listed for later.

### 5.4 Interaction and guarding against accidental triggering

| Action | Effect |
|---|---|
| The pointer enters | Feedback within 100 ms (a slight brightening), to say "this can be expanded" |
| Hovering for 300 ms | Expands; collapses 600 ms after the pointer leaves |
| **While a mouse button is down, or during a drag** | Does not expand (in similar products, expanding by accident while a tab is dragged is the number one complaint) |
| A click on the collapsed state | Carries out the default action; without one, expands and stays so until the close button or somewhere else is clicked |
| The close button on the expanded state | Collapses. Always visible; it does not depend on moving the pointer away |
| The wheel | Switches between several activities |
| Right click | A menu we draw ourselves (the web engine's own context menu would make the application below lose focus): end this activity, make it an ordinary notification, no longer show this extension's activities, settings |
| The "hide now" hotkey (bound in the settings) | Hides until the next activity starts. Fullscreen detection misses in practice (some players, borderless games), and a fallback is needed |
| The "give the keyboard to the island" hotkey (bound in the settings, unbound by default) | The only case in which the island gets the keyboard |

The settings can turn off expanding on hover, so that only a click expands.

### 5.5 Fullscreen, Do Not Disturb, screen sharing

- **Fullscreen**: hidden. GNOME's top bar items and X11 windows of the dock
  type do so automatically; the other presenters judge for themselves.
  Dictation is the exception: it is using the microphone, and in fullscreen
  there still has to be a minimal indication (how, on Wayland and on X11,
  is to be settled in the prototype).
- **Do Not Disturb**: only alerting updates and automatic expansion are
  suppressed, and **the collapsed state is shown as usual**. This is what
  GNOME does with its own indicators. "Hidden under Do Not Disturb", which
  v0.1 wrote, is dropped.
- **Screen sharing and screen recording**: the content of an activity may
  contain private text (the transcript of dictation, the AI's answer). On
  Windows a switch "does not appear in screen capture" is offered (there is
  a proper interface).
  Linux has no corresponding interface, so instead: when the screen is
  detected as being shared or recorded (GNOME's top bar shows this state
  itself), the collapsed state shows only the icon and no text, and the
  expanded state does not expand automatically. Whether this detection is
  feasible is to be confirmed in the prototype.

### 5.6 Several activities

When several applications have activities, iOS shows two in the minimal
state; there is no precedent for "one compact plus one minimal", and none
for how to order across applications either (its relevance only works
inside an application). So this part is our own design, to be verified in
the prototype:

- What the order goes by, from high to low: the one using the microphone
  (dictation) → the one the user has just interacted with → the one that is
  alerting → the most recently updated.
  The `relevance` an extension gives **only orders among the several
  activities of the same extension** and takes no part in the order across
  extensions, otherwise every extension would fill in 100.
- Showing: the first is compact, and the rest are combined into one minimal
  state with a number. When there is not enough room (a crowded GNOME top
  bar), all fall back to the minimal state.

### 5.7 Rendering

- Zero redraws when idle: with no change in the activities, no timer of any
  kind is running.
- **No continuous animation.** The volume indication of dictation is
  updated at most 10 times a second, with a few discrete heights and not a
  continuous waveform; the progress ring moves in discrete steps.
- **No blurred shadows**: shadows use a pre-rendered image (the palette
  already does this).
- In the web presenter, expanding and collapsing use one transition of
  160 ms; since on this path `transform` is no cheaper than changing the
  size, the size is simply transitioned, and the nine-slice trick of v0.1
  is not needed.

---

## 6. The interface

`@heyo/api` gains a `LiveActivity` module. The permission is named
`activity`. Where it differs from v0.1 it is marked "Changed".

### 6.1 Starting, updating, ending

```ts
export namespace LiveActivity {
  /** The part that does not change. */
  interface Attributes {
    /** Unique within one extension. Calling start again with the same id is an update. */
    id: string;
    /** What this is ("Build", "Timer"). */
    name: string;
    icon: Image.ImageLike;
    tint?: Color.ColorLike;
    /** What to do when the collapsed state is clicked. */
    action?: Action;
    /** Whether the user can end it from the menu. true by default. */
    dismissible?: boolean;
    /** Changed: what to do when it cannot be shown on this desktop. "notify" by default. */
    fallback?: "notify" | "drop";
  }

  /** The part that changes. */
  interface Content {
    /** What the collapsed state shows. When left out, it is title, truncated. */
    compact?: Value;
    title: string;
    subtitle?: string;
    /** The body of the expanded state, one of three. */
    body?: { progress: Progress } | { timer: Timer } | { text: string };
    /** At most three, and they appear only in the expanded state. */
    actions?: Button[];
    /** Changed: needs the user to deal with it (confirm, type). The presenter moves it to the front and uses the accent color. */
    needsAttention?: boolean;
    /** Changed: after this the content may be inaccurate, and it is shown faded. It does not end the activity. */
    staleAt?: Date;
    /** The order among several activities of the same extension, 0–100. */
    relevance?: number;
  }

  /** A small piece of content that changes by itself. Timers and progress are counted by Heyo itself. */
  type Value = string | { timer: Timer } | { progress: Progress } | { icon: Image.ImageLike };

  /** Changed: a span of time plus a moment of pausing; countdowns, counting up and progress that advances with time all use it. */
  interface Timer {
    start: Date;
    /** Left out, it counts up and has no end. */
    end?: Date;
    /** The moment it was paused at; left out, it is running. */
    pausedAt?: Date;
    /** With an end it counts down by default. */
    countsDown?: boolean;
    style?: "clock" | "relative"; // "12:05" or "12 min"
  }

  interface Progress {
    /** 0–1. Left out, and with no interval, the progress is indeterminate. */
    value?: number;
    /** Advances by itself with time. */
    interval?: Pick<Timer, "start" | "end" | "pausedAt">;
  }

  function start(attributes: Attributes, content: Content, options?: StartOptions): Promise<Handle>;

  interface StartOptions {
    /** Changed: expands once on first appearance, for about 4 seconds. true by default; it does not expand under Do Not Disturb or in fullscreen. */
    announce?: boolean;
  }

  interface Handle {
    readonly id: string;
    update(content: Partial<Content>, options?: { alert?: Alert }): Promise<void>;
    end(final?: Partial<Content>, options?: { keep?: "none" | "brief" }): Promise<void>;
  }

  function list(): Promise<Handle[]>;
  function get(id: string): Promise<Handle | undefined>;

  /** Changed: what an activity will look like on this desktop, so that an extension can adjust its content to it. */
  function presentation(): Promise<{
    support: Support;
    /** How many characters fit in the collapsed state (the top bar is narrow). */
    compactCharacters: number;
    /** Whether the expanded state is available, and whether it can hold buttons (a KDE job entry cannot). */
    expanded: boolean;
    buttons: boolean;
    /** The version of the templates, raised when layouts are added later. */
    version: number;
  }>;
}
```

### 6.2 The order of updates (changed, new)

The runtime shim gives every `update` an increasing sequence number; the
host drops an update whose number is not greater than the one already
applied.
This is because one activity may be updated at the same time by the command
that started it, by the handler command in the background and by an action
the host carries out, and an old update that arrives late must not
overwrite a newer one.
iOS, Xiaomi and vivo all have the same protection. An extension does not
have to care about this field.

### 6.3 Alerting updates

```ts
interface Alert {
  /** Expands briefly to show this update, for about 4 seconds. It does not expand under Do Not Disturb or in fullscreen. */
  expand?: boolean;
}
```

There is no sound. A stronger alert is the business of notifications.

### 6.4 Ending

- `end()` defaults to `keep: "brief"`: the final content is shown for
  4 seconds and then disappears. `"none"` disappears at once.
- **Changed**: v0.1 allowed the ended state to be kept for up to 15
  minutes, and this is now dropped. For a result that stays for a long time
  ("The build failed, click here to see") the host sends a notification
  instead, because the desktop has no second place like the lock screen to
  put it. **A result that needs the user to deal with it** uses
  `needsAttention` to keep the activity from ending.

### 6.5 Buttons, actions and events

```ts
interface Button {
  title: string;
  icon?: Image.ImageLike;
  style?: "regular" | "destructive";
  action: Action;
}

type Action =
  | { open: string }                                   // carried out by the host directly
  | { copy: string }
  | { launch: { command: string; arguments?: Record<string, string>; context?: unknown } }
  | { event: string; data?: unknown }                  // sent back to the extension
  | { end: true };
```

This division into two levels is the same as that of iOS: declarative
actions correspond to its links, and callback actions to its intents (which
also bring the application up in the background to handle them).

How events are sent to an extension does not change: the manifest declares
an activity handler command (`"heyo": { "handles": "activity" }`), and when
there is an event the host runs it in the background runtime; a running
command can also subscribe directly with `LiveActivity.onEvent()`. The
kinds of event: `action`, `dismissed`, `expired`.

### 6.6 Limits

| Item | Value | Based on |
|---|---|---|
| The longest a single activity exists | 8 hours | iOS and Huawei both have 8 hours |
| An activity without updates | **Changed**: reduced to the minimal state after 2 hours, ended after 4 hours. `staleAt` only fades the display and does not end the activity | Huawei's two-step degradation; "ended after 1 hour" in v0.1 was stricter than any platform, and would kill activities that are quiet for a long time (long builds, meetings) by mistake |
| Size of the content | Attributes plus content, 4 KB in all | iOS |
| Text of the collapsed state | **Changed**: 10 characters or fewer are advised, and what is beyond is truncated; the exact value is given by `presentation().compactCharacters` | Android's status bar chip is about 7 characters; the 20 of v0.1 was too wide |
| Buttons | 3, only in the expanded state | Android allows 3; iOS advises fewer |
| Update frequency | Once a second per activity; what exceeds that is merged | Our own; no platform precedent to compare with |
| Activities at a time per extension | 3 | Our own |
| Alerting updates | Twice a minute per activity | Our own |

### 6.7 Permission and the conditions for starting

- **Who can start one** (changed, new): an activity must be brought about
  by an action of the user: a command the user ran, or after the user chose
  a button on an activity.
  **A command running in the background on a schedule cannot start a new
  activity out of nothing**; it can only update an activity it already has.
  This is the common rule of all the phone platforms, and it also stops
  "background extensions making themselves noticed".
- **Consent**: the manifest has to list `activity`. Nothing is asked on
  first use (no phone platform asks per activity; they all have one switch
  per application), but every extension has a switch in the settings, and
  the context menu can also turn off an extension's activities directly.
  There is a switch for all of them besides.
  (The only third-party desktop island interface that has shipped does ask,
  so this is still one of the questions of section 12.)

---

## 7. Letting what already exists benefit automatically

1. **`showHUD`**: with the palette closed it is a system notification
   today; it becomes one brief display of 1.5–2 seconds on the activity
   channel (in similar products such hints last between 1.3 and 3 seconds).
2. **A toast still in progress after the palette closes** (the `Animated`
   style): it becomes an activity, and turns into the ended state on
   success or failure. For the first time the progress of commands without
   a view has somewhere to be shown.
3. **Menu bar commands (`MenuBarExtra`)**: changed (v0.3), they become
   first-class citizens. On macOS they live in the menu bar to begin with,
   and mapping them to tray icons, as is done on Linux today, is a
   substitute that falls short (stock GNOME has no tray, and titles and the
   icons of menu items cannot be shown). Of the 100 most popular
   extensions, 26 have menu bar commands, which is exactly the ready-made
   content that keeps the status bar from being empty. They enter the
   status bar as **status items** (section 3), and are not bound by the
   rules on duration and starting that activities have; besides text, the
   title of the collapsed state can also use the timers and progress of
   activities (a meeting extension writes an end time and we count it
   ourselves, with no refresh every minute), which is an enhancement on top
   of Raycast's API and does not change its existing behavior. KDE,
   Hyprland and Windows go on using the tray.

---

## 8. Built-in activities

Reordered by the evidence of 1.2, with the states v0.1 lacked filled in.

| Activity | Collapsed | Expanded | Notes |
|---|---|---|---|
| Dictation | A microphone icon plus the last few characters recognized | The text recognized, the state of polishing; finish, cancel | Always first. GNOME already has its own microphone indication while recording, so this shows what that does not have (the transcript and the buttons), and does not repeat a microphone icon |
| Timer, Pomodoro | An icon plus a countdown | A large timer; pause, add 1 minute, end | A new built-in command; it proves the timers the host counts by itself |
| AI task | An icon plus a state | The question and the beginning of the answer; stop, open the conversation | Three states: in progress, **needs you to confirm or type**, **done** (with `needsAttention` and the ended state) |
| Brief hint | An icon plus two or three characters | None | That is, `showHUD`: copied, muted and so on |
| Engine downloads, background commands | An icon plus progress | Progress, time elapsed; cancel | Appears only beyond 2 seconds; on KDE it also goes into job progress |
| Media that is playing | — | — | **Deliberately put off.** It is the content that comes first in every similar product, but it needs connecting to the system's media interface and handling covers and several players, which is a separate piece of work; and GNOME's own date menu already has media controls. To be done after the extension interface is stable |

---

## 9. Implementation

### 9.1 Structure

```
extension ──LiveActivity.*──▶ runtime shim ──ext.activity.*──▶ host activities.rs (the table of activities, limits, order, expiry)
                                                                  │
                                       presenter interface: present(the activities, in order)
        ┌──────────────────┬──────────────────┬───────────────────┼──────────────────┬────────────────────┐
 GNOME extension      layer-shell       X11 floating       Windows window    KDE job progress  Dock/tray/notification
 (top bar item +        window             window                               (auxiliary)          (auxiliary)
expanded window)
```

- **`src-tauri/src/activities.rs` (new)**: the table of activities.
  Checking limits, sequence protection, ordering, degradation and expiry,
  and "which ones should be shown now" are all functions that do not depend
  on windows, with the time passed in as a parameter, and all unit tested.
- **`src-tauri/src/activities/presenter.rs` (new)**: the presenter
  interface, and choosing the main presenter and the auxiliary channels by
  session. The choosing logic is a testable pure function too.
- **`wire.rs`**: `ext.activity.start/update/end/list/presentation`; the
  content types are exported with ts-rs.
- **The expanded interface** (a web page): it only draws and holds no
  state.
- **`gnome-extension/`**: gains an `Activities(json)` method and an
  `ActivityAction(id, action)` signal; the protocol version goes up by one,
  and with an old extension GNOME falls back to the auxiliary channels.
- **`runtime/src/heyo/activity.ts` (new)**: the shim; where `showHUD` and
  toasts connect.

### 9.2 Phases

| Phase | What | Questions to answer |
|---|---|---|
| 0 Prototype | The table of activities; a GNOME top bar item (X11 session); the expanded window right below it; one built-in timer | Does a flaw show at the join? Is it in the way beside the clock? How do several activities look right? |
| 0b | The same thing in the GNOME Wayland container | Which of the three focus approaches of 5.2 works |
| 1 Built-in | Dictation, brief hints, AI tasks, downloads and background commands; the auxiliary channels (Dock progress, notifications) | — |
| 2 Extension interface | `LiveActivity`, the conditions for starting, the switches in the settings, handler commands, events, limits | Test the interface with a probe extension and two or three real scenarios |
| 3 Other presenters | The X11 floating window (outside GNOME); KDE job progress; layer-shell | Needs a KDE or Hyprland test environment |
| 4 | Windows; menu bar commands onto the island; media | — |

Nothing further is started until phases 0 and 0b are done.

---

## 10. Testing

| Layer | How it is tested |
|---|---|
| The table of activities | Table-driven unit tests: limits, sequence protection (an old update that arrives late is dropped), the two-step degradation and expiry (with the time injected), ordering, a background command cannot start a new activity |
| The text of timers and progress | A pure function: given the span, the moment of pausing and now, it gives the text shown |
| Choosing the presenter | A pure function: session type plus abilities → the main presenter and the auxiliary channels |
| Protocol and shim | Every `ext.activity.*` is read in; exceeding a limit returns `invalid`; the stubs outside Heyo |
| The GNOME top bar item | The check script in the Wayland container (the existing `scripts/wayland-test/`): the item appears, the text changes with updates, it is not visible in fullscreen, a click sends the action signal |
| The expanded window | The X11 sandbox: positioned right below the top bar item, does not take focus (the focused window does not change), a click on a transparent part lands on the window below, hovering does not expand while a button is down |
| Idle overhead | With one activity at rest, the CPU time of the interface process over 10 seconds is zero; while a timer is counting there is only one redraw a second |
| Smoothness | Expand and collapse for 3 seconds each, and count the frames over 20 ms |

---

## 11. The interface engine

The expanded interface is drawn with the existing web view. Flutter was
evaluated once and is shelved (`docs/flutter-evaluation.md`).

---

## 12. Open questions

1. **The position in GNOME's top bar**: beside the clock (the middle area),
   or to the left of the status area on the right? The middle is more
   conspicuous, and also more likely to feel crowded.
1a. **One item per extension, or merged into one Heyo item?** On macOS each takes a place of its own; GNOME's top bar has limited room. Leaning toward: the user chooses in the settings which ones are shown separately, and the rest go into the menu of one "Heyo" item.
1b. **The card that expands on hover**: is it demoted to a later enhancement (the reasoning of section 0), or do you think it is the soul of this feature?
1c. **Gathering the progress of other applications**: is it worth researching?
2. **The expanded state on GNOME**: by 5.2, first try "our own window right
   below the top bar item", and fall back to the Shell's native popup if
   that does not work. Is that all right?
3. **Permission**: `activity` is not asked about on first use and there is
   only a switch (what this document chooses), or is it asked once?
4. **Conditions for starting**: a scheduled background command cannot start
   a new activity (6.7). This blocks uses such as "check the build every
   5 minutes and show it when it fails", which would have to use
   notifications instead. Acceptable?
5. **Merging dictation in**: on GNOME, dictation would go from the floating
   indicator at the bottom of the screen to an item in the top bar plus an
   expanded panel, farther from where the typing is. Merge it in, or does
   dictation keep the floating indicator it has now, with only one more
   status item in the top bar?
6. **Media that is playing** is put off (section 8). Do you agree?
7. **The name**: the interface is called `LiveActivity`. "Dynamic Island"
   (Apple's product name) should not be used in the wording of the
   interface; is it called "Live Activities" in the settings?

---

## 13. Statements in v0.1 that did not match the facts, and have been corrected

| What v0.1 said | What is the case | Source |
|---|---|---|
| With several activities at once iOS shows "one compact plus one minimal" | It shows two in the minimal state; relevance only orders within one application | `mobile.md` |
| Samsung's Now Bar can switch between several activities left and right | The official description is swiping up and then choosing; no public developer interface was found | `mobile.md` |
| Huawei's templates are "progress, pickup code, timer" | Five: progress, emphasized text, left and right text, score, navigation; the timer is a placeholder | `mobile.md` |
| The existing dictation indicator is "a window of the notification type", and the island can do likewise | The code sets no window type; the Shell extension has no keep-above, positioning or fullscreen handling either | `linux.md`, `window.md` |
| On Windows, click-through is done with a hit test that returns "transparent" | It only works for windows of the same thread; a window region should be used | `window.md` |
| Animating with `transform` avoids redraws | There are no GPU layers on this rendering path, and `transform` is a redraw on the CPU all the same | `window.md` (measured) |
| On GNOME Wayland it is enough for the extension to place an application window | An application window cannot refuse focus there | `linux.md`, `window.md` (measured) |
| "Other Wayland desktops are not supported in the first version" | layer-shell covers the mainstream compositors other than GNOME's, KDE among them | `linux.md` |

Confirmed: at most 8 hours, a content limit of 4 KB (the official iOS
documentation); the content takes only data; timers are counted by the
host; hovering expands after 300 ms and collapses after 600 ms.

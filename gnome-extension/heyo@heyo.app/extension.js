// Heyo's companion inside GNOME Shell.
//
// On Wayland an application cannot see other applications' windows, type
// into them, follow the clipboard while unfocused, or read the screen. The
// Shell can. This extension does those things for Heyo, over D-Bus, at
// /dev/heyo/Shell on the Shell's own connection.
//
// These are powers no other application has, so every call is checked: the
// caller must be the process that owns Heyo's bus name.

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import St from 'gi://St';

import {Extension, InjectionManager} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Config from 'resource:///org/gnome/shell/misc/config.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const PATH = '/dev/heyo/Shell';
// The name Heyo's single-instance guard owns for as long as it runs.
const HEYO_NAME = 'dev.heyo.app.SingleInstance';
// What this extension can do, as a number Heyo compares with what it needs.
// It only ever grows: methods and fields are added, never changed or taken
// away, so a newer extension serves an older Heyo and the other way round.
// 2: Monitors says how its numbers relate to pixels and where the pointer
//    is; the Ready signal; Heyo's passing windows are kept out of the Dash,
//    the switchers and the overview.
// (Heyo reads this line from the installed file: keep its form.)
const API = 2;

const INTERFACE = `
<node>
  <interface name="dev.heyo.Shell">
    <method name="Version"><arg type="u" direction="out"/></method>
    <method name="Windows"><arg type="s" direction="out"/></method>
    <method name="Monitors"><arg type="s" direction="out"/></method>
    <method name="Activate"><arg type="u" direction="in"/></method>
    <method name="Close"><arg type="u" direction="in"/></method>
    <method name="Maximize"><arg type="u" direction="in"/></method>
    <method name="MoveResize">
      <arg type="u" direction="in"/>
      <arg type="i" direction="in"/><arg type="i" direction="in"/>
      <arg type="i" direction="in"/><arg type="i" direction="in"/>
    </method>
    <method name="OwnFocused"><arg type="b" direction="out"/></method>
    <method name="ActivateOwn"><arg type="s" direction="in"/><arg type="b" direction="out"/></method>
    <method name="WatchKey"><arg type="s" direction="in"/><arg type="b" direction="out"/></method>
    <method name="Paste"><arg type="b" direction="in"/></method>
    <method name="Selection"><arg type="s" direction="out"/></method>
    <method name="ClipboardText"><arg type="s" direction="out"/></method>
    <method name="SetClipboardText"><arg type="s" direction="in"/></method>
    <method name="ClipboardImage"><arg type="s" direction="in"/><arg type="b" direction="out"/></method>
    <method name="Screenshot">
      <arg type="i" direction="in"/><arg type="i" direction="in"/>
      <arg type="i" direction="in"/><arg type="i" direction="in"/>
      <arg type="s" direction="in"/>
    </method>
    <signal name="Ready"/>
    <signal name="KeyDown"/>
    <signal name="KeyUp"/>
    <signal name="KeyInterrupted"/>
    <signal name="ClipboardChanged">
      <arg type="s" name="source"/>
      <arg type="as" name="mimetypes"/>
    </signal>
  </interface>
</node>`;

// Until GNOME 49, maximizing took a direction; since then it takes nothing.
const TAKES_FLAGS = Number(Config.PACKAGE_VERSION.split('.')[0]) < 49;

// The keys that can be held to talk: the accelerator that reports the press,
// and the modifier whose state tells when it is let go. (Only modifiers: the
// Shell is not told when other keys are released.)
const HOLD_KEYS = {
    alt_r: ['Alt_R', Clutter.ModifierType.MOD1_MASK],
    alt_l: ['Alt_L', Clutter.ModifierType.MOD1_MASK],
    control_r: ['Control_R', Clutter.ModifierType.CONTROL_MASK],
    control_l: ['Control_L', Clutter.ModifierType.CONTROL_MASK],
    shift_r: ['Shift_R', Clutter.ModifierType.SHIFT_MASK],
};
const BUTTONS = Clutter.ModifierType.BUTTON1_MASK | Clutter.ModifierType.BUTTON2_MASK | Clutter.ModifierType.BUTTON3_MASK;

// A window's number as it goes over D-Bus. The Shell counts in 64 bits from
// a random 32-bit start, so it very nearly always fits in 32 as it is.
const idOf = window => window.get_id() % 0x100000000;

const isOwn = window => (window.get_wm_class() ?? '').toLowerCase() === 'heyo';

// Heyo's windows that come and go under the user's hands: the palette, the
// dictation pill, the screenshot overlay and pinned screenshots. They are
// not applications to switch to. (Its settings window is an ordinary one.)
const PASSING = ['Heyo', 'Heyo Dictation', 'Heyo Screenshot', 'Heyo Pinned Screenshot'];
const isPassing = window => isOwn(window) && PASSING.includes(window.get_title() ?? '');

export default class HeyoExtension extends Extension {
    enable() {
        // wrapJSObject calls `NameAsync(params, invocation)` when it exists,
        // which is where the caller can be checked. Every method goes
        // through `_guard`.
        const methods = {};
        for (const name of ['Version', 'Windows', 'Monitors', 'Activate', 'Close', 'Maximize', 'MoveResize', 'OwnFocused', 'ActivateOwn', 'WatchKey', 'Paste',
            'Selection', 'ClipboardText', 'SetClipboardText', 'ClipboardImage', 'Screenshot'])
            methods[`${name}Async`] = (params, invocation) => this._guard(name, params, invocation);
        this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE, methods);
        this._dbus.export(Gio.DBus.session, PATH);

        this._activated = global.display.connect('accelerator-activated', (_display, action) => {
            if (this._key && action === this._key.action)
                this._keyDown();
        });
        // The held key is Heyo's while Heyo runs, and nobody's after.
        this._heyoWatch = Gio.bus_watch_name(Gio.BusType.SESSION, HEYO_NAME, Gio.BusNameWatcherFlags.NONE, null, () => this._unwatchKey());

        this._selection = global.display.get_selection();
        this._ownerChanged = this._selection.connect('owner-changed', (_selection, type, _source) => {
            if (type !== Meta.SelectionType.SELECTION_CLIPBOARD)
                return;
            const focused = global.display.focus_window;
            const mimetypes = this._selection.get_mimetypes(type) ?? [];
            this._dbus.emit_signal('ClipboardChanged', new GLib.Variant('(sas)', [focused?.get_wm_class() ?? '', mimetypes]));
        });

        this._keepOutOfLists();
        // The Shell switches extensions off while the screen is locked, and
        // what Heyo had asked for (the held key) is forgotten: say when to
        // ask again.
        this._dbus.emit_signal('Ready', null);
    }

    disable() {
        this._showInLists();
        if (this._ownerChanged)
            this._selection.disconnect(this._ownerChanged);
        this._ownerChanged = null;
        this._selection = null;
        this._unwatchKey();
        if (this._activated)
            global.display.disconnect(this._activated);
        this._activated = null;
        if (this._heyoWatch)
            Gio.bus_unwatch_name(this._heyoWatch);
        this._heyoWatch = null;
        this._dbus?.unexport();
        this._dbus = null;
        this._keyboard = null;
    }

    /// Keeps Heyo's passing windows out of the Dash, the Alt+Tab switchers
    /// and the overview, as the skip-taskbar hint does on X11. On Wayland an
    /// application cannot ask for that; only the Shell can decide it.
    _keepOutOfLists() {
        // Since GNOME 49 a window can be told to stay out of window lists.
        this._native = typeof Meta.Window.prototype.hide_from_window_list === 'function';
        // A window's title and application are known once it is on screen.
        this._mapped = global.window_manager.connect('map', (_wm, actor) => this._keepOut(actor.meta_window));
        for (const actor of global.get_window_actors())
            this._keepOut(actor.meta_window);
        if (this._native)
            return;
        // Before that, the Shell's own lists read `skip_taskbar`, which is
        // made to answer yes on those windows (see `_keepOut`). The Dash asks
        // which applications run, which is counted where that cannot reach:
        // leave out the one whose windows are all of that kind.
        this._injections = new InjectionManager();
        this._injections.overrideMethod(Shell.AppSystem.prototype, 'get_running', original => function () {
            return original.call(this).filter(app => {
                const windows = app.get_windows();
                return !(windows.length > 0 && windows.every(isPassing));
            });
        });
    }

    _keepOut(window) {
        if (!window || !isPassing(window))
            return;
        if (this._native) {
            window.hide_from_window_list();
        } else if (!Object.hasOwn(window, 'skip_taskbar')) {
            Object.defineProperty(window, 'skip_taskbar', {value: true, configurable: true});
            Object.defineProperty(window, 'is_skip_taskbar', {value: () => true, configurable: true});
            // Lists made before now are made again.
            const app = Shell.WindowTracker.get_default().get_window_app(window);
            if (app)
                Shell.AppSystem.get_default().emit('app-state-changed', app);
        }
    }

    _showInLists() {
        if (this._mapped)
            global.window_manager.disconnect(this._mapped);
        this._mapped = null;
        this._injections?.clear();
        this._injections = null;
        for (const actor of global.get_window_actors()) {
            const window = actor.meta_window;
            if (!window || !isOwn(window))
                continue;
            if (this._native) {
                window.show_in_window_list();
            } else {
                delete window.skip_taskbar;
                delete window.is_skip_taskbar;
            }
        }
    }

    _pid(name) {
        const reply = Gio.DBus.session.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus',
            'GetConnectionUnixProcessID', new GLib.Variant('(s)', [name]), new GLib.VariantType('(u)'),
            Gio.DBusCallFlags.NONE, 500, null);
        return reply.deepUnpack()[0];
    }

    /// Whether the caller is Heyo: the process that owns its bus name.
    /// (Compared by process, as Heyo talks over more than one connection.)
    _isHeyo(invocation) {
        try {
            return this._pid(invocation.get_sender()) === this._pid(HEYO_NAME);
        } catch (_e) {
            // Nobody owns the name: Heyo is not running.
            return false;
        }
    }

    _guard(name, params, invocation) {
        if (!this._isHeyo(invocation)) {
            invocation.return_dbus_error('dev.heyo.Shell.Error.Denied', 'Only Heyo may use this interface.');
            return;
        }
        try {
            const result = this[`_${name}`](...params, invocation);
            // Methods that answer later return `undefined` and use the invocation.
            if (result !== undefined)
                invocation.return_value(result);
        } catch (e) {
            logError(e, `Heyo: ${name}`);
            invocation.return_dbus_error('dev.heyo.Shell.Error.Failed', `${e.message ?? e}`);
        }
    }

    _Version() {
        return new GLib.Variant('(u)', [API]);
    }

    /// Application windows, most recently used first.
    _windows() {
        return global.display.get_tab_list(Meta.TabList.NORMAL, null).filter(w => !isOwn(w));
    }

    _find(id) {
        const window = global.display.get_tab_list(Meta.TabList.NORMAL_ALL, null).find(w => idOf(w) === id);
        if (!window)
            throw new Error('the window is gone');
        return window;
    }

    _Windows() {
        const focused = global.display.focus_window;
        const workspace = global.workspace_manager.get_active_workspace();
        // Bottom to top.
        const stack = global.display.sort_windows_by_stacking(this._windows());
        const windows = this._windows().map(w => {
            const frame = w.get_frame_rect();
            return {
                id: idOf(w),
                title: w.get_title() ?? '',
                class: w.get_wm_class() ?? '',
                active: w === focused,
                monitor: w.get_monitor(),
                x: frame.x, y: frame.y, width: frame.width, height: frame.height,
                // What can be seen now, and in what order: for picking a window in a screenshot.
                shown: !w.minimized && w.located_on_workspace(workspace),
                layer: stack.indexOf(w),
            };
        });
        return new GLib.Variant('(s)', [JSON.stringify(windows)]);
    }

    /// Each monitor's rectangle and the part of it free of panels and docks.
    ///
    /// Every position this interface gives or takes is in the Shell's own
    /// coordinates. Those are pixels of the screen when monitors are laid
    /// out in pixels (the default until fractional scaling is switched on),
    /// and pixels divided by each monitor's scale otherwise. `density` says
    /// which: how many pixels one unit is on that monitor.
    _Monitors() {
        const workspace = global.workspace_manager.get_active_workspace();
        // Laid out in pixels, the stage is drawn one to one whatever the
        // monitors' scales; otherwise as finely as the finest monitor.
        const scaled = global.stage.get_resource_scale() > 1;
        const [pointerX, pointerY] = global.get_pointer();
        const monitors = [];
        for (let i = 0; i < global.display.get_n_monitors(); i++) {
            const area = global.display.get_monitor_geometry(i);
            const work = workspace.get_work_area_for_monitor(i);
            const scale = global.display.get_monitor_scale(i);
            monitors.push({
                x: area.x, y: area.y, width: area.width, height: area.height,
                work: {x: work.x, y: work.y, width: work.width, height: work.height},
                scale,
                density: scaled ? scale : 1,
                // The one the pointer is on, which an application is not told.
                pointer: pointerX >= area.x && pointerX < area.x + area.width && pointerY >= area.y && pointerY < area.y + area.height,
            });
        }
        return new GLib.Variant('(s)', [JSON.stringify(monitors)]);
    }

    _Activate(id) {
        // Switches workspace too, and counts as the user's own action.
        Main.activateWindow(this._find(id));
        return new GLib.Variant('()', []);
    }

    _Close(id) {
        this._find(id).delete(global.get_current_time());
        return new GLib.Variant('()', []);
    }

    _unmaximize(window) {
        if (window.is_fullscreen())
            window.unmake_fullscreen();
        if (TAKES_FLAGS)
            window.unmaximize(Meta.MaximizeFlags.BOTH);
        else
            window.unmaximize();
    }

    _Maximize(id) {
        const window = this._find(id);
        if (TAKES_FLAGS)
            window.maximize(Meta.MaximizeFlags.BOTH);
        else
            window.maximize();
        return new GLib.Variant('()', []);
    }

    _MoveResize(id, x, y, width, height) {
        const window = this._find(id);
        this._unmaximize(window);
        // A window that was maximized takes a moment to accept a new size.
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 50, () => {
            // It may have been closed meanwhile.
            if (window.get_compositor_private())
                window.move_resize_frame(true, x, y, width, height);
            return GLib.SOURCE_REMOVE;
        });
        return new GLib.Variant('()', []);
    }

    /// Whether one of Heyo's own windows has the keyboard focus.
    _OwnFocused() {
        const focused = global.display.focus_window;
        return new GLib.Variant('(b)', [!!focused && isOwn(focused)]);
    }

    /// Brings one of Heyo's own windows to the front. On Wayland an
    /// application cannot do that for itself without the user having just
    /// interacted with it.
    _ActivateOwn(title) {
        const window = global.display.get_tab_list(Meta.TabList.NORMAL_ALL, null).find(w => isOwn(w) && w.get_title() === title);
        if (window)
            Main.activateWindow(window);
        return new GLib.Variant('(b)', [!!window]);
    }

    _unwatchKey() {
        if (this._held)
            GLib.source_remove(this._held);
        this._held = null;
        if (this._key) {
            Main.wm.allowKeybinding(Meta.external_binding_name_for_action(this._key.action), Shell.ActionMode.NONE);
            global.display.ungrab_accelerator(this._key.action);
        }
        this._key = null;
    }

    /// Reports one key being held: KeyDown when it goes down, KeyUp when it
    /// is let go, and KeyInterrupted if another key is pressed meanwhile.
    /// An empty name stops. False if the key cannot be watched.
    ///
    /// The key still works as a modifier for the application in front.
    _WatchKey(name) {
        this._unwatchKey();
        const key = HOLD_KEYS[name];
        if (key) {
            const action = global.display.grab_accelerator(key[0], Meta.KeyBindingFlags.NONE);
            if (action !== Meta.KeyBindingAction.NONE) {
                Main.wm.allowKeybinding(Meta.external_binding_name_for_action(action), Shell.ActionMode.ALL);
                this._key = {action, mask: key[1]};
            }
        }
        return new GLib.Variant('(b)', [!!this._key]);
    }

    _keyDown() {
        if (this._held)
            return;
        const idle = global.backend.get_core_idle_monitor();
        const started = GLib.get_monotonic_time();
        const pointer = () => {
            const [x, y, modifiers] = global.get_pointer();
            return {x, y, modifiers};
        };
        let before = pointer();
        // When the last input seen so far happened, in ms after the press.
        let seen = 0;
        let interrupted = false;
        this._dbus.emit_signal('KeyDown', null);
        this._held = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 20, () => {
            const now = pointer();
            const elapsed = (GLib.get_monotonic_time() - started) / 1000;
            if (!(now.modifiers & this._key.mask) || elapsed > 120000) {
                this._dbus.emit_signal('KeyUp', null);
                this._held = null;
                return GLib.SOURCE_REMOVE;
            }
            // The Shell does not see keys meant for applications, but it
            // knows when the last input of any kind was. Input that did not
            // move the pointer or a button was a key.
            const last = elapsed - idle.get_idletime();
            if (last > seen + 10) {
                seen = last;
                const moved = now.x !== before.x || now.y !== before.y || (now.modifiers & BUTTONS) !== (before.modifiers & BUTTONS);
                if (!moved && !interrupted) {
                    interrupted = true;
                    this._dbus.emit_signal('KeyInterrupted', null);
                }
            }
            before = now;
            return GLib.SOURCE_CONTINUE;
        });
    }

    /// Presses the paste shortcut in the focused application.
    _Paste(terminal) {
        const seat = Clutter.get_default_backend().get_default_seat();
        this._keyboard ??= seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        const keys = terminal ? [Clutter.KEY_Control_L, Clutter.KEY_Shift_L, Clutter.KEY_v] : [Clutter.KEY_Control_L, Clutter.KEY_v];
        const steps = [...keys.map(k => [k, Clutter.KeyState.PRESSED]), ...[...keys].reverse().map(k => [k, Clutter.KeyState.RELEASED])];
        // One event at a time, so the application sees a real key sequence.
        const next = () => {
            const step = steps.shift();
            if (!step)
                return GLib.SOURCE_REMOVE;
            this._keyboard.notify_keyval(GLib.get_monotonic_time(), step[0], step[1]);
            return GLib.SOURCE_CONTINUE;
        };
        GLib.timeout_add(GLib.PRIORITY_DEFAULT, 12, next);
        return new GLib.Variant('()', []);
    }

    _text(type, invocation) {
        St.Clipboard.get_default().get_text(type, (_clipboard, text) => {
            invocation.return_value(new GLib.Variant('(s)', [text ?? '']));
        });
    }

    /// The text selected in the focused application (the primary selection).
    _Selection(invocation) {
        this._text(St.ClipboardType.PRIMARY, invocation);
    }

    _ClipboardText(invocation) {
        this._text(St.ClipboardType.CLIPBOARD, invocation);
    }

    _SetClipboardText(text) {
        St.Clipboard.get_default().set_text(St.ClipboardType.CLIPBOARD, text);
        return new GLib.Variant('()', []);
    }

    /// Writes the image on the clipboard to `path` as PNG; false if there is none.
    _ClipboardImage(path, invocation) {
        St.Clipboard.get_default().get_content(St.ClipboardType.CLIPBOARD, 'image/png', (_clipboard, bytes) => {
            let written = false;
            try {
                const data = bytes?.get_data?.() ?? bytes;
                if (data && data.length > 0) {
                    Gio.File.new_for_path(path).replace_contents(data, null, false, Gio.FileCreateFlags.PRIVATE, null);
                    written = true;
                }
            } catch (e) {
                logError(e, 'Heyo: ClipboardImage');
            }
            invocation.return_value(new GLib.Variant('(b)', [written]));
        });
    }

    /// Saves an area of the screen to `path` as PNG.
    _Screenshot(x, y, width, height, path, invocation) {
        const stream = Gio.File.new_for_path(path).replace(null, false, Gio.FileCreateFlags.PRIVATE, null);
        const shooter = new Shell.Screenshot();
        shooter.screenshot_area(x, y, width, height, stream, (source, result) => {
            try {
                source.screenshot_area_finish(result);
                stream.close(null);
                invocation.return_value(new GLib.Variant('()', []));
            } catch (e) {
                invocation.return_dbus_error('dev.heyo.Shell.Error.Failed', `${e.message ?? e}`);
            }
        });
    }
}

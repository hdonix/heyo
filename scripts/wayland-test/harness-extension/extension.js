// Test container only. With unsafe mode on, any client may call
// org.gnome.Shell.Eval and org.gnome.Shell.Screenshot, which is how the
// tests take screenshots and inspect windows. Heyo itself must never rely
// on this: on a real desktop those calls are refused.
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

export default class Harness extends Extension {
    enable() {
        this._was = global.context.unsafe_mode;
        global.context.unsafe_mode = true;
        // A session starts in the overview, where typing searches GNOME
        // itself. Tests want the plain desktop.
        const leave = () => Main.overview.hide();
        if (Main.layoutManager._startingUp)
            this._startup = Main.layoutManager.connect('startup-complete', leave);
        else
            leave();
    }

    disable() {
        if (this._startup)
            Main.layoutManager.disconnect(this._startup);
        this._startup = null;
        global.context.unsafe_mode = this._was;
    }
}

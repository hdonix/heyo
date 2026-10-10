// The window of a heyo-ui process, and everything about it that is GTK's or
// X11's business. Dart decides; this does: the method channel `heyo/window`
// is the whole of what Dart knows about the toolkit, so that a runner for
// another platform can offer the same channel.
//
// The window draws itself: no title bar, transparent where Flutter paints
// nothing. It starts hidden and is shown when Dart says.

#include "heyo_window.h"

#include <cmath>
#include <cstring>

#include <flutter_linux/flutter_linux.h>
#ifdef GDK_WINDOWING_X11
#include <gdk/gdkx.h>
#endif

#include "flutter/generated_plugin_registrant.h"

namespace {

GtkWindow* window = nullptr;
FlView* view = nullptr;
FlMethodChannel* channel = nullptr;
// Whether the user may resize it. A window that may not keeps the size it
// is given (GTK would otherwise shrink it to nothing: Flutter's view asks
// for no size of its own).
gboolean resizable = FALSE;
// Covering a monitor above everything, holding the keyboard by a grab.
gboolean covering = FALSE;

// --- Reading what Dart sent -------------------------------------------------

FlValue* field(FlValue* args, const char* name) {
  if (args == nullptr || fl_value_get_type(args) != FL_VALUE_TYPE_MAP) {
    return nullptr;
  }
  FlValue* value = fl_value_lookup_string(args, name);
  return value != nullptr && fl_value_get_type(value) != FL_VALUE_TYPE_NULL ? value : nullptr;
}

gboolean number(FlValue* args, const char* name, double* out) {
  FlValue* value = field(args, name);
  if (value == nullptr) {
    return FALSE;
  }
  switch (fl_value_get_type(value)) {
    case FL_VALUE_TYPE_INT:
      *out = static_cast<double>(fl_value_get_int(value));
      return TRUE;
    case FL_VALUE_TYPE_FLOAT:
      *out = fl_value_get_float(value);
      return TRUE;
    default:
      return FALSE;
  }
}

gboolean flag(FlValue* args, const char* name, gboolean* out) {
  FlValue* value = field(args, name);
  if (value == nullptr || fl_value_get_type(value) != FL_VALUE_TYPE_BOOL) {
    return FALSE;
  }
  *out = fl_value_get_bool(value);
  return TRUE;
}

const gchar* text(FlValue* args, const char* name) {
  FlValue* value = field(args, name);
  return value != nullptr && fl_value_get_type(value) == FL_VALUE_TYPE_STRING ? fl_value_get_string(value) : nullptr;
}

// --- The window ---------------------------------------------------------------

GdkWindow* gdk_window() {
  return gtk_widget_get_window(GTK_WIDGET(window));
}

gboolean on_x11() {
#ifdef GDK_WINDOWING_X11
  return GDK_IS_X11_DISPLAY(gtk_widget_get_display(GTK_WIDGET(window)));
#else
  return FALSE;
#endif
}

void size_to(int width, int height) {
  width = MAX(width, 1);
  height = MAX(height, 1);
  if (!resizable) {
    GdkGeometry hints;
    hints.min_width = hints.max_width = width;
    hints.min_height = hints.max_height = height;
    gtk_window_set_geometry_hints(window, nullptr, &hints, static_cast<GdkWindowHints>(GDK_HINT_MIN_SIZE | GDK_HINT_MAX_SIZE));
  }
  gtk_window_set_default_size(window, width, height);
  gtk_window_resize(window, width, height);
  // A window that is not on screen is given its size only when it is
  // shown: until then Flutter would lay its view out at the size the window
  // was made with, and draw its first frames for a window of another size.
  if (!gtk_widget_get_visible(GTK_WIDGET(window)) && view != nullptr) {
    GtkAllocation whole = {0, 0, width, height};
    gtk_widget_size_allocate(GTK_WIDGET(window), &whole);
  }
}

// The monitor the pointer is on, else the first.
GdkMonitor* pointed_monitor() {
  GdkDisplay* display = gtk_widget_get_display(GTK_WIDGET(window));
  GdkSeat* seat = gdk_display_get_default_seat(display);
  GdkDevice* pointer = seat != nullptr ? gdk_seat_get_pointer(seat) : nullptr;
  GdkMonitor* monitor = nullptr;
  if (pointer != nullptr) {
    gint x = 0, y = 0;
    gdk_device_get_position(pointer, nullptr, &x, &y);
    monitor = gdk_display_get_monitor_at_point(display, x, y);
  }
  if (monitor == nullptr) {
    monitor = gdk_display_get_primary_monitor(display);
  }
  return monitor != nullptr ? monitor : gdk_display_get_monitor(display, 0);
}

FlValue* rectangle(const GdkRectangle& area) {
  FlValue* map = fl_value_new_map();
  fl_value_set_string_take(map, "x", fl_value_new_int(area.x));
  fl_value_set_string_take(map, "y", fl_value_new_int(area.y));
  fl_value_set_string_take(map, "width", fl_value_new_int(area.width));
  fl_value_set_string_take(map, "height", fl_value_new_int(area.height));
  return map;
}

// `configure`: what kind of window this is. Only what is named changes.
void configure(FlValue* args) {
  const gchar* title = text(args, "title");
  if (title != nullptr) {
    gtk_window_set_title(window, title);
  }
  gboolean on = FALSE;
  if (flag(args, "resizable", &on)) {
    resizable = on;
    if (resizable) {
      gtk_window_set_geometry_hints(window, nullptr, nullptr, static_cast<GdkWindowHints>(0));
    }
  }
  double width = 0, height = 0;
  if (number(args, "width", &width) && number(args, "height", &height)) {
    size_to(static_cast<int>(lround(width)), static_cast<int>(lround(height)));
  }
  // A window that never takes the keyboard from the application in use.
  if (flag(args, "focus", &on)) {
    gtk_window_set_accept_focus(window, on);
    gtk_window_set_focus_on_map(window, on);
  }
  if (flag(args, "above", &on)) {
    gtk_window_set_keep_above(window, on);
  }
  if (flag(args, "taskbar", &on)) {
    gtk_window_set_skip_taskbar_hint(window, !on);
    gtk_window_set_skip_pager_hint(window, !on);
  }
  // What the window manager should take it for. Set before it is shown.
  const gchar* type = text(args, "type");
  if (type != nullptr) {
    GdkWindowTypeHint hint = GDK_WINDOW_TYPE_HINT_NORMAL;
    if (strcmp(type, "utility") == 0) hint = GDK_WINDOW_TYPE_HINT_UTILITY;
    if (strcmp(type, "dialog") == 0) hint = GDK_WINDOW_TYPE_HINT_DIALOG;
    if (strcmp(type, "notification") == 0) hint = GDK_WINDOW_TYPE_HINT_NOTIFICATION;
    if (strcmp(type, "dock") == 0) hint = GDK_WINDOW_TYPE_HINT_DOCK;
    gtk_window_set_type_hint(window, hint);
  }
}

// `focus`: gives the window the keyboard. Heyo is usually shown from a
// hotkey, not from input to its own window, so a window manager's focus
// stealing prevention rejects a plain request; presenting with the server's
// current time makes the request newer than the user's last interaction.
// (src-tauri/src/platform/focus.rs does the same for the web view.)
void focus() {
  guint32 time = GDK_CURRENT_TIME;
#ifdef GDK_WINDOWING_X11
  GdkWindow* target = gdk_window();
  if (target != nullptr && GDK_IS_X11_WINDOW(target)) {
    time = gdk_x11_get_server_time(target);
  }
#endif
  gtk_window_present_with_time(window, time);
  gtk_widget_grab_focus(GTK_WIDGET(view));
}

// `center`: the middle of the monitor the pointer is on.
void center() {
  GdkMonitor* monitor = pointed_monitor();
  if (monitor == nullptr) {
    return;
  }
  GdkRectangle area;
  gdk_monitor_get_geometry(monitor, &area);
  gint width = 0, height = 0;
  gtk_window_get_size(window, &width, &height);
  gtk_window_move(window, area.x + (area.width - width) / 2, area.y + (area.height - height) / 2);
}

// `setFrame`: where the window is and how large, in logical pixels or (as
// Heyo's host counts) physical ones. Whatever is left out stays.
void set_frame(FlValue* args) {
  gboolean logical = TRUE;
  flag(args, "logical", &logical);
  const double scale = logical ? 1.0 : gtk_widget_get_scale_factor(GTK_WIDGET(window));
  double x = 0, y = 0, width = 0, height = 0;
  if (number(args, "width", &width) && number(args, "height", &height)) {
    size_to(static_cast<int>(lround(width / scale)), static_cast<int>(lround(height / scale)));
  }
  if (number(args, "x", &x) && number(args, "y", &y)) {
    gtk_window_move(window, static_cast<int>(floor(x / scale)), static_cast<int>(floor(y / scale)));
  }
}

// `geometry`: the window, the monitor the pointer is on and the scale.
// Rectangles are in logical pixels; times `scale` for physical ones.
FlValue* geometry() {
  FlValue* map = fl_value_new_map();
  GdkRectangle frame = {0, 0, 0, 0};
  gtk_window_get_position(window, &frame.x, &frame.y);
  gtk_window_get_size(window, &frame.width, &frame.height);
  fl_value_set_string_take(map, "window", rectangle(frame));
  fl_value_set_string_take(map, "scale", fl_value_new_float(gtk_widget_get_scale_factor(GTK_WIDGET(window))));
  fl_value_set_string_take(map, "visible", fl_value_new_bool(gtk_widget_get_visible(GTK_WIDGET(window))));
  fl_value_set_string_take(map, "focused", fl_value_new_bool(gtk_window_is_active(window)));
  GdkMonitor* monitor = pointed_monitor();
  if (monitor != nullptr) {
    GdkRectangle area;
    gdk_monitor_get_geometry(monitor, &area);
    fl_value_set_string_take(map, "monitor", rectangle(area));
    gdk_monitor_get_workarea(monitor, &area);
    fl_value_set_string_take(map, "workarea", rectangle(area));
  }
  return map;
}

// `setInputRegion`: the parts of the window that take the pointer, as
// rectangles `[x, y, width, height]` in logical pixels; a click anywhere
// else goes to whatever is underneath. Without rectangles, all of it does.
void set_input_region(FlValue* args) {
  // Through the widget, which keeps the region and puts it back whenever
  // the window is made anew; set on the GdkWindow alone it does not hold.
  FlValue* rects = field(args, "rects");
  if (rects == nullptr || fl_value_get_type(rects) != FL_VALUE_TYPE_LIST) {
    gtk_widget_input_shape_combine_region(GTK_WIDGET(window), nullptr);
    return;
  }
  cairo_region_t* region = cairo_region_create();
  for (size_t i = 0; i < fl_value_get_length(rects); i++) {
    FlValue* rect = fl_value_get_list_value(rects, i);
    if (fl_value_get_type(rect) != FL_VALUE_TYPE_LIST || fl_value_get_length(rect) != 4) {
      continue;
    }
    double sides[4];
    for (size_t side = 0; side < 4; side++) {
      FlValue* value = fl_value_get_list_value(rect, side);
      sides[side] = fl_value_get_type(value) == FL_VALUE_TYPE_INT ? static_cast<double>(fl_value_get_int(value)) : fl_value_get_float(value);
    }
    cairo_rectangle_int_t area = {static_cast<int>(floor(sides[0])), static_cast<int>(floor(sides[1])), static_cast<int>(ceil(sides[2])),
                                  static_cast<int>(ceil(sides[3]))};
    cairo_region_union_rectangle(region, &area);
  }
  gtk_widget_input_shape_combine_region(GTK_WIDGET(window), region);
  cairo_region_destroy(region);
}

// --- Covering the screen: the screenshot overlay -----------------------------

// No window manager hands a window that bypasses it the keyboard, so once
// it is on screen it grabs it: every key goes to it until it hides. The
// hotkey that started the capture is usually still held and the desktop's
// own grab on it still active, so the first attempts can fail: retry.
// (src-tauri/src/platform/x11/overlay.rs, which this follows, says more.)
gboolean grab_keyboard(gpointer attempts_left) {
  const guint attempts = GPOINTER_TO_UINT(attempts_left);
  GdkWindow* target = gdk_window();
  if (!covering || target == nullptr || !gtk_widget_get_visible(GTK_WIDGET(window))) {
    return G_SOURCE_REMOVE;
  }
  GdkSeat* seat = gdk_display_get_default_seat(gdk_window_get_display(target));
  if (seat == nullptr) {
    return G_SOURCE_REMOVE;
  }
  const GdkGrabStatus status = gdk_seat_grab(seat, target, GDK_SEAT_CAPABILITY_KEYBOARD, TRUE, nullptr, nullptr, nullptr, nullptr);
  if (status != GDK_GRAB_SUCCESS && attempts > 0) {
    g_timeout_add(50, grab_keyboard, GUINT_TO_POINTER(attempts - 1));
  }
  return G_SOURCE_REMOVE;
}

gboolean on_map(GtkWidget*, GdkEvent*, gpointer) {
  if (covering && on_x11()) {
    grab_keyboard(GUINT_TO_POINTER(60));
  }
  return FALSE;
}

void release_keyboard() {
  if (!covering) {
    return;
  }
  covering = FALSE;
  GdkSeat* seat = gdk_display_get_default_seat(gtk_widget_get_display(GTK_WIDGET(window)));
  if (seat != nullptr) {
    gdk_seat_ungrab(seat);
  }
}

// `cover`: puts the window exactly over a rectangle (logical pixels), above
// everything, the desktop's own panels included, and gives it the keyboard.
// On X11 it bypasses the window manager (like a menu): nothing animates it
// and nothing may move it. Elsewhere it is the best an ordinary window can
// do: fullscreen on that monitor.
void cover(FlValue* args) {
  double x = 0, y = 0, width = 0, height = 0;
  if (!number(args, "x", &x) || !number(args, "y", &y) || !number(args, "width", &width) || !number(args, "height", &height)) {
    return;
  }
  const int left = static_cast<int>(x), top = static_cast<int>(y);
  const int wide = static_cast<int>(width), high = static_cast<int>(height);
  covering = TRUE;
  if (on_x11()) {
    gtk_widget_realize(GTK_WIDGET(window));
    gdk_window_set_override_redirect(gdk_window(), TRUE);
    gtk_window_move(window, left, top);
    size_to(wide, high);
    gtk_widget_show(GTK_WIDGET(window));
    gdk_window_move_resize(gdk_window(), left, top, wide, high);
    gdk_window_raise(gdk_window());
    // Mapped already (shown twice running): the map event will not come.
    grab_keyboard(GUINT_TO_POINTER(60));
  } else {
    GdkDisplay* display = gtk_widget_get_display(GTK_WIDGET(window));
    gtk_window_move(window, left, top);
    int found = -1;
    for (int n = 0; n < gdk_display_get_n_monitors(display); n++) {
      GdkRectangle area;
      gdk_monitor_get_geometry(gdk_display_get_monitor(display, n), &area);
      if (area.x == left && area.y == top) {
        found = n;
      }
    }
    if (found >= 0) {
      gtk_window_fullscreen_on_monitor(window, gtk_window_get_screen(window), found);
    } else {
      gtk_window_fullscreen(window);
    }
    gtk_widget_show(GTK_WIDGET(window));
    gtk_window_present(window);
  }
  gtk_widget_grab_focus(GTK_WIDGET(view));
}

void hide() {
  release_keyboard();
  gtk_widget_hide(GTK_WIDGET(window));
}

// --- The channel --------------------------------------------------------------

void on_call(FlMethodChannel*, FlMethodCall* call, gpointer) {
  const gchar* method = fl_method_call_get_name(call);
  FlValue* args = fl_method_call_get_args(call);
  g_autoptr(FlValue) result = nullptr;
  if (strcmp(method, "configure") == 0) {
    configure(args);
  } else if (strcmp(method, "show") == 0) {
    gtk_widget_show(GTK_WIDGET(window));
  } else if (strcmp(method, "hide") == 0) {
    hide();
  } else if (strcmp(method, "focus") == 0) {
    focus();
  } else if (strcmp(method, "center") == 0) {
    center();
  } else if (strcmp(method, "setFrame") == 0) {
    set_frame(args);
  } else if (strcmp(method, "geometry") == 0) {
    result = geometry();
  } else if (strcmp(method, "setInputRegion") == 0) {
    set_input_region(args);
  } else if (strcmp(method, "cover") == 0) {
    cover(args);
  } else if (strcmp(method, "startMove") == 0) {
    // The pointer is down on something that drags the window (a title bar
    // Heyo draws itself).
    GdkSeat* seat = gdk_display_get_default_seat(gtk_widget_get_display(GTK_WIDGET(window)));
    gint x = 0, y = 0;
    if (seat != nullptr && gdk_seat_get_pointer(seat) != nullptr) {
      gdk_device_get_position(gdk_seat_get_pointer(seat), nullptr, &x, &y);
      gtk_window_begin_move_drag(window, 1, x, y, GDK_CURRENT_TIME);
    }
  } else if (strcmp(method, "close") == 0) {
    release_keyboard();
    gtk_main_quit();
  } else {
    fl_method_call_respond(call, FL_METHOD_RESPONSE(fl_method_not_implemented_response_new()), nullptr);
    return;
  }
  fl_method_call_respond(call, FL_METHOD_RESPONSE(fl_method_success_response_new(result)), nullptr);
}

void tell(const gchar* method, FlValue* argument) {
  fl_method_channel_invoke_method(channel, method, argument, nullptr, nullptr, nullptr);
}

// The window gained or lost the keyboard.
void on_active(GObject*, GParamSpec*, gpointer) {
  g_autoptr(FlValue) active = fl_value_new_bool(gtk_window_is_active(window));
  tell("focusChanged", active);
}

// Closing (Alt+F4) is asked of Dart, which asks Heyo: most of its windows
// only hide.
gboolean on_delete(GtkWidget*, GdkEvent*, gpointer) {
  tell("closeRequested", nullptr);
  return TRUE;
}

}  // namespace

void heyo_window_create(char** arguments) {
  window = GTK_WINDOW(gtk_window_new(GTK_WINDOW_TOPLEVEL));
  gtk_window_set_title(window, "Heyo");
  gtk_window_set_decorated(window, FALSE);
  // Transparent wherever Flutter paints nothing: needs a visual with alpha,
  // chosen before the window exists on the display.
  gtk_widget_set_app_paintable(GTK_WIDGET(window), TRUE);
  GdkVisual* visual = gdk_screen_get_rgba_visual(gtk_window_get_screen(window));
  if (visual != nullptr) {
    gtk_widget_set_visual(GTK_WIDGET(window), visual);
  }
  size_to(800, 600);

  g_autoptr(FlDartProject) project = fl_dart_project_new();
  fl_dart_project_set_dart_entrypoint_arguments(project, arguments);
  view = fl_view_new(project);
  GdkRGBA transparent = {0, 0, 0, 0};
  fl_view_set_background_color(view, &transparent);
  gtk_widget_show(GTK_WIDGET(view));
  gtk_container_add(GTK_CONTAINER(window), GTK_WIDGET(view));
  // Flutter starts drawing once the view is realized; the window stays
  // hidden until Dart shows it.
  gtk_widget_realize(GTK_WIDGET(view));
  fl_register_plugins(FL_PLUGIN_REGISTRY(view));

  g_autoptr(FlStandardMethodCodec) codec = fl_standard_method_codec_new();
  channel = fl_method_channel_new(fl_engine_get_binary_messenger(fl_view_get_engine(view)), "heyo/window", FL_METHOD_CODEC(codec));
  fl_method_channel_set_method_call_handler(channel, on_call, nullptr, nullptr);

  g_signal_connect(window, "notify::is-active", G_CALLBACK(on_active), nullptr);
  g_signal_connect(window, "delete-event", G_CALLBACK(on_delete), nullptr);
  g_signal_connect(window, "map-event", G_CALLBACK(on_map), nullptr);
  g_signal_connect(window, "destroy", G_CALLBACK(gtk_main_quit), nullptr);
  gtk_widget_grab_focus(GTK_WIDGET(view));
}

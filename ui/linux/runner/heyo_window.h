#ifndef HEYO_WINDOW_H_
#define HEYO_WINDOW_H_

#include <gtk/gtk.h>

// Makes this process's one window, hidden, with Flutter drawing in it, and
// the channel Dart tells it what to do over (`heyo/window`; the Dart side is
// lib/host/window.dart, which documents the methods). `arguments` go to
// Dart's `main` as they are.
void heyo_window_create(char** arguments);

#endif  // HEYO_WINDOW_H_

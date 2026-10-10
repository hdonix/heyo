// heyo-ui: one of Heyo's windows, drawn by Flutter. Heyo starts one process
// for each window and speaks to it over its stdin and stdout
// (src-tauri/src/shell/flutter.rs); which window this is, it is told in its
// arguments, which go to Dart as they are (lib/main.dart).

#include <cstdio>
#include <cstring>
#include <pthread.h>
#include <poll.h>
#include <unistd.h>

#include <gtk/gtk.h>

#include "heyo_window.h"

// Ends the process when whoever started it closes its stdin: when Heyo
// exits, or is done with this window. Dart reads the same stdin and would
// notice too, but only while Dart is running: a process that hung holding
// the keyboard (the screenshot overlay grabs it) must still end with Heyo.
static void* end_with_host(void*) {
  struct pollfd in = {STDIN_FILENO, 0, 0};
  for (;;) {
    if (poll(&in, 1, -1) > 0 && (in.revents & (POLLHUP | POLLERR | POLLNVAL))) {
      _exit(0);
    }
  }
  return nullptr;
}

int main(int argc, char** argv) {
  gboolean standalone = FALSE;
  for (int i = 1; i < argc; i++) {
    if (strcmp(argv[i], "--version") == 0) {
      printf("heyo-ui %s\n", HEYO_VERSION);
      return 0;
    }
    if (strcmp(argv[i], "--standalone") == 0) {
      standalone = TRUE;
    }
  }

  // Heyo's windows are known as Heyo's by their class (X11) or application
  // id (Wayland), which the toolkit takes from these: the same as the
  // windows of the `heyo` program itself have.
  g_set_prgname("heyo");
  gdk_set_program_class("Heyo");
  gtk_init(&argc, &argv);

  // Without a host (a view being worked on, started by hand) there is no
  // one whose leaving should end it.
  if (!standalone) {
    pthread_t thread;
    if (pthread_create(&thread, nullptr, end_with_host, nullptr) == 0) {
      pthread_detach(thread);
    }
  }

  heyo_window_create(argv + 1);
  gtk_main();
  return 0;
}

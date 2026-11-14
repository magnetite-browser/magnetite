// A virtual pointer that stays for the whole session (wlrctl's goes away
// after each call, before the client has bound wl_pointer). Commands on
// stdin: "move X Y" (output pixels), "down N" / "up N" (272 left, 273
// right), "scroll DY".
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <wayland-client.h>
#include "vp.h"

static struct zwlr_virtual_pointer_manager_v1 *manager;

static void global(void *data, struct wl_registry *reg, uint32_t name, const char *iface, uint32_t version) {
  if (!strcmp(iface, zwlr_virtual_pointer_manager_v1_interface.name))
    manager = wl_registry_bind(reg, name, &zwlr_virtual_pointer_manager_v1_interface, 1);
}
static void global_remove(void *data, struct wl_registry *reg, uint32_t name) {}
static const struct wl_registry_listener listener = {global, global_remove};

static uint32_t now(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

int main(int argc, char **argv) {
  int w = argc > 2 ? atoi(argv[1]) : 1400, h = argc > 2 ? atoi(argv[2]) : 900;
  struct wl_display *display = wl_display_connect(NULL);
  if (!display) return 1;
  struct wl_registry *reg = wl_display_get_registry(display);
  wl_registry_add_listener(reg, &listener, NULL);
  wl_display_roundtrip(display);
  if (!manager) return 2;
  struct zwlr_virtual_pointer_v1 *p = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(manager, NULL);
  wl_display_roundtrip(display);
  char line[128];
  int a, b;
  while (fgets(line, sizeof line, stdin)) {
    if (sscanf(line, "move %d %d", &a, &b) == 2)
      zwlr_virtual_pointer_v1_motion_absolute(p, now(), a, b, w, h);
    else if (sscanf(line, "down %d", &a) == 1)
      zwlr_virtual_pointer_v1_button(p, now(), a, WL_POINTER_BUTTON_STATE_PRESSED);
    else if (sscanf(line, "up %d", &a) == 1)
      zwlr_virtual_pointer_v1_button(p, now(), a, WL_POINTER_BUTTON_STATE_RELEASED);
    else if (sscanf(line, "scroll %d", &a) == 1)
      zwlr_virtual_pointer_v1_axis(p, now(), WL_POINTER_AXIS_VERTICAL_SCROLL, wl_fixed_from_int(a));
    zwlr_virtual_pointer_v1_frame(p);
    wl_display_roundtrip(display);
  }
  return 0;
}

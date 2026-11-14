#ifndef REMYA_LAUNCHER_WINDOW_H_
#define REMYA_LAUNCHER_WINDOW_H_

#include <flutter_linux/flutter_linux.h>
#include <gtk/gtk.h>

bool configure_launcher_overlay(GtkWindow *window);
void configure_settings_window(GtkWindow *window);
void register_launcher_window(FlView *view, GtkWindow *window);

#endif

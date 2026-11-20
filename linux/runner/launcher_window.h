#ifndef REMYA_LAUNCHER_WINDOW_H_
#define REMYA_LAUNCHER_WINDOW_H_

#include <flutter_linux/flutter_linux.h>
#include <gtk/gtk.h>

bool configure_launcher_overlay(GtkWindow *window, bool resident);
void show_launcher_window(GtkWindow *window);
bool launcher_window_is_open(GtkWindow *window);
void open_launcher_window(GtkWindow *window, bool clipboard);
bool hide_resident_launcher(GtkWindow *window);
void configure_settings_window(GtkWindow *window);
void register_launcher_window(FlView *view, GtkWindow *window);
void show_launcher_clipboard(GtkWindow *window);

#endif

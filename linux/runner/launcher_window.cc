#include "launcher_window.h"

#include <gtk-layer-shell.h>
#include <initializer_list>

namespace {

const char kHiding[] = "remya-launcher-hiding";
const char kPress[] = "remya-launcher-press";
const char kChannel[] = "remya-launcher-window-channel";
const char kReady[] = "remya-launcher-dart-ready";
const char kShowClipboard[] = "remya-launcher-show-clipboard";
const char kDismissWindows[] = "remya-launcher-dismiss-windows";

// The last primary press, which a later beginMove turns into a move drag.
struct Press {
  guint32 time = GDK_CURRENT_TIME;
  int x = 0;
  int y = 0;
};

void remember_press(GtkGestureMultiPress *, int, double, double,
                    gpointer data) {
  auto *press = static_cast<Press *>(data);
  g_autoptr(GdkEvent) event = gtk_get_current_event();
  double x, y;
  if (event == nullptr || !gdk_event_get_root_coords(event, &x, &y)) return;
  press->time = gdk_event_get_time(event);
  press->x = static_cast<int>(x);
  press->y = static_cast<int>(y);
}

void begin_move(GtkWindow *window) {
  auto *press = static_cast<Press *>(g_object_get_data(G_OBJECT(window), kPress));
  if (press == nullptr || press->time == GDK_CURRENT_TIME) return;
  gtk_window_begin_move_drag(window, 1, press->x, press->y, press->time);
  press->time = GDK_CURRENT_TIME;
}

gboolean close_window(gpointer data) {
  gtk_widget_destroy(GTK_WIDGET(data));
  return G_SOURCE_REMOVE;
}

void schedule_close(GtkWidget *window) {
  g_idle_add_full(G_PRIORITY_DEFAULT_IDLE, close_window, g_object_ref(window),
                  g_object_unref);
}

void configure_overlay(GtkWindow *window, const char *name,
                       GtkLayerShellKeyboardMode keyboard_mode) {
  auto *widget = GTK_WIDGET(window);
  auto *visual = gdk_screen_get_rgba_visual(gtk_widget_get_screen(widget));
  if (visual != nullptr)
    gtk_widget_set_visual(widget, visual);
  gtk_widget_set_app_paintable(widget, TRUE);
  gtk_window_set_decorated(window, FALSE);
  gtk_layer_init_for_window(window);
  gtk_layer_set_namespace(window, name);
  gtk_layer_set_layer(window, GTK_LAYER_SHELL_LAYER_OVERLAY);
  gtk_layer_set_exclusive_zone(window, -1);
  for (auto edge : {GTK_LAYER_SHELL_EDGE_LEFT, GTK_LAYER_SHELL_EDGE_RIGHT,
                    GTK_LAYER_SHELL_EDGE_TOP, GTK_LAYER_SHELL_EDGE_BOTTOM}) {
    gtk_layer_set_anchor(window, edge, TRUE);
  }
  gtk_layer_set_keyboard_mode(window, keyboard_mode);
}

GHashTable *dismiss_windows(GtkWidget *launcher) {
  return static_cast<GHashTable *>(
      g_object_get_data(G_OBJECT(launcher), kDismissWindows));
}

void destroy_dismiss_window(gpointer data) {
  gtk_widget_destroy(GTK_WIDGET(data));
  g_object_unref(data);
}

void add_dismiss_window(GtkWidget *launcher, GdkMonitor *monitor) {
  auto *windows = dismiss_windows(launcher);
  if (g_hash_table_contains(windows, monitor)) return;

  auto *dismiss = gtk_window_new(GTK_WINDOW_TOPLEVEL);
  g_hash_table_insert(windows, g_object_ref(monitor), g_object_ref(dismiss));
  configure_overlay(GTK_WINDOW(dismiss), "remya-launcher-dismiss",
                    GTK_LAYER_SHELL_KEYBOARD_MODE_NONE);
  gtk_layer_set_monitor(GTK_WINDOW(dismiss), monitor);
  gtk_window_set_accept_focus(GTK_WINDOW(dismiss), FALSE);
  g_signal_connect(dismiss, "draw",
                   G_CALLBACK(+[](GtkWidget *, cairo_t *cr, gpointer) {
                     cairo_set_operator(cr, CAIRO_OPERATOR_CLEAR);
                     cairo_paint(cr);
                     return TRUE;
                   }), nullptr);
  gtk_widget_add_events(dismiss, GDK_BUTTON_PRESS_MASK | GDK_BUTTON_RELEASE_MASK);
  g_signal_connect(dismiss, "button-release-event",
                   G_CALLBACK(+[](GtkWidget *, GdkEventButton *, gpointer data) {
                     schedule_close(GTK_WIDGET(data));
                     return TRUE;
                   }), launcher);
  gtk_widget_show(dismiss);
}

void dismiss_monitor_added(GdkDisplay *, GdkMonitor *monitor,
                           GtkWidget *launcher) {
  if (gtk_widget_get_mapped(launcher)) add_dismiss_window(launcher, monitor);
}

void dismiss_monitor_removed(GdkDisplay *, GdkMonitor *monitor,
                             GtkWidget *launcher) {
  g_hash_table_remove(dismiss_windows(launcher), monitor);
}

gboolean on_focus_out(GtkWidget *window, GdkEvent *, gpointer) {
  if (gtk_widget_get_visible(window) &&
      !g_object_get_data(G_OBJECT(window), kHiding)) {
    schedule_close(window);
  }
  return GDK_EVENT_PROPAGATE;
}

void hide_launcher(GtkWindow *window) {
  auto *widget = GTK_WIDGET(window);
  g_object_set_data(G_OBJECT(widget), kHiding, GINT_TO_POINTER(TRUE));
  gtk_widget_hide(widget);
  gdk_display_sync(gtk_widget_get_display(widget));
}

gint run_dialog(GtkWindow *window, GtkWidget *dialog) {
  // Unmap the overlay so the chooser receives both keyboard and pointer input.
  hide_launcher(window);
  gtk_window_present(GTK_WINDOW(dialog));
  return gtk_dialog_run(GTK_DIALOG(dialog));
}

void save_file(GtkWindow *window, FlMethodCall *call, const gchar *filename) {
  GtkWidget *dialog = gtk_file_chooser_dialog_new(
      "Save clipboard image", nullptr, GTK_FILE_CHOOSER_ACTION_SAVE, "_Cancel",
      GTK_RESPONSE_CANCEL, "_Save", GTK_RESPONSE_ACCEPT, nullptr);
  auto *chooser = GTK_FILE_CHOOSER(dialog);
  gtk_file_chooser_set_current_name(chooser, filename);
  gtk_file_chooser_set_do_overwrite_confirmation(chooser, TRUE);

  gint response = run_dialog(window, dialog);
  g_autofree gchar *path = response == GTK_RESPONSE_ACCEPT
                               ? gtk_file_chooser_get_filename(chooser)
                               : nullptr;
  gtk_widget_destroy(dialog);
  show_launcher_window(window);
  g_autoptr(FlValue) value =
      path == nullptr ? nullptr : fl_value_new_string(path);
  fl_method_call_respond_success(call, value, nullptr);
}

void open_with(GtkWindow *window, FlMethodCall *call, const gchar *path) {
  g_autoptr(GFile) file = g_file_new_for_path(path);
  GtkWidget *dialog =
      gtk_app_chooser_dialog_new(nullptr, GTK_DIALOG_MODAL, file);
  gtk_window_set_title(GTK_WINDOW(dialog), "Open clipboard item with");

  gint response = run_dialog(window, dialog);
  g_autoptr(GAppInfo) app =
      response == GTK_RESPONSE_OK
          ? gtk_app_chooser_get_app_info(GTK_APP_CHOOSER(dialog))
          : nullptr;
  gtk_widget_destroy(dialog);
  show_launcher_window(window);
  if (app == nullptr) {
    g_autoptr(FlValue) value = fl_value_new_bool(FALSE);
    fl_method_call_respond_success(call, value, nullptr);
    return;
  }

  g_autoptr(GError) error = nullptr;
  GList files = {file, nullptr, nullptr};
  if (!g_app_info_launch(app, &files, nullptr, &error)) {
    fl_method_call_respond_error(call, "open-failed", error->message, nullptr,
                                 nullptr);
    return;
  }
  g_autoptr(FlValue) value = fl_value_new_bool(TRUE);
  fl_method_call_respond_success(call, value, nullptr);
}

void resolve_app_icon(GtkWidget *widget, FlMethodCall *call, FlValue *args) {
  FlValue *name = nullptr;
  FlValue *size = nullptr;
  if (args != nullptr && fl_value_get_type(args) == FL_VALUE_TYPE_MAP) {
    name = fl_value_lookup_string(args, "name");
    size = fl_value_lookup_string(args, "size");
  }
  if (name == nullptr || fl_value_get_type(name) != FL_VALUE_TYPE_STRING ||
      size == nullptr || fl_value_get_type(size) != FL_VALUE_TYPE_INT ||
      fl_value_get_int(size) < 1 || fl_value_get_int(size) > 256) {
    fl_method_call_respond_error(call, "invalid-arguments",
                                 "Expected an icon name and size from 1 to 256",
                                 nullptr, nullptr);
    return;
  }

  const gchar *icon = fl_value_get_string(name);
  const int pixels = fl_value_get_int(size);
  g_autoptr(GdkPixbuf) pixbuf = nullptr;
  if (g_path_is_absolute(icon)) {
    if (g_file_test(icon, G_FILE_TEST_IS_REGULAR))
      pixbuf = gdk_pixbuf_new_from_file_at_scale(icon, pixels, pixels, TRUE,
                                                 nullptr);
  } else if (*icon != '\0' && g_strstr_len(icon, -1, "/") == nullptr) {
    auto *theme =
        gtk_icon_theme_get_for_screen(gtk_widget_get_screen(widget));
    pixbuf = gtk_icon_theme_load_icon(theme, icon, pixels,
                                     GTK_ICON_LOOKUP_FORCE_SIZE, nullptr);
  }

  g_autofree gchar *buffer = nullptr;
  gsize length = 0;
  g_autoptr(FlValue) value = nullptr;
  if (pixbuf != nullptr &&
      gdk_pixbuf_save_to_bufferv(pixbuf, &buffer, &length, "png", nullptr,
                                nullptr, nullptr)) {
    value = fl_value_new_uint8_list(
        reinterpret_cast<const uint8_t *>(buffer), length);
  }
  fl_method_call_respond_success(call, value, nullptr);
}

void window_method(FlMethodChannel *, FlMethodCall *call, gpointer data) {
  auto *window = GTK_WINDOW(data);
  auto *widget = GTK_WIDGET(window);
  const gchar *method = fl_method_call_get_name(call);
  FlValue *args = fl_method_call_get_args(call);

  if (g_str_equal(method, "hide")) {
    hide_launcher(window);
  } else if (g_str_equal(method, "show")) {
    show_launcher_window(window);
  } else if (g_str_equal(method, "close")) {
    schedule_close(widget);
  } else if (g_str_equal(method, "beginMove")) {
    begin_move(window);
  } else if (g_str_equal(method, "readyForRequests")) {
    g_object_set_data(G_OBJECT(window), kReady, GINT_TO_POINTER(TRUE));
    g_autoptr(FlValue) pending = fl_value_new_bool(
        g_object_steal_data(G_OBJECT(window), kShowClipboard) != nullptr);
    fl_method_call_respond_success(call, pending, nullptr);
    return;
  } else if (g_str_equal(method, "resolveAppIcon")) {
    resolve_app_icon(widget, call, args);
    return;
  } else if (g_str_equal(method, "saveFile") ||
             g_str_equal(method, "openWith")) {
    if (fl_value_get_type(args) != FL_VALUE_TYPE_STRING) {
      fl_method_call_respond_error(call, "invalid-arguments",
                                   "Expected a file name", nullptr, nullptr);
    } else if (g_str_equal(method, "saveFile")) {
      save_file(window, call, fl_value_get_string(args));
    } else {
      open_with(window, call, fl_value_get_string(args));
    }
    return;
  } else {
    fl_method_call_respond_not_implemented(call, nullptr);
    return;
  }
  fl_method_call_respond_success(call, nullptr, nullptr);
}

} // namespace

bool configure_launcher_overlay(GtkWindow *window) {
  if (!gtk_layer_is_supported())
    return false;

  auto *widget = GTK_WIDGET(window);
  configure_overlay(window, "remya-launcher",
                    GTK_LAYER_SHELL_KEYBOARD_MODE_EXCLUSIVE);
  auto *windows = g_hash_table_new_full(g_direct_hash, g_direct_equal,
                                       g_object_unref, destroy_dismiss_window);
  g_object_set_data_full(G_OBJECT(window), kDismissWindows, windows,
                         [](gpointer data) {
                           g_hash_table_unref(static_cast<GHashTable *>(data));
                         });
  g_signal_connect(window, "unmap",
                   G_CALLBACK(+[](GtkWidget *, gpointer data) {
                     g_hash_table_remove_all(static_cast<GHashTable *>(data));
                   }), windows);
  auto *display = gtk_widget_get_display(widget);
  g_signal_connect_object(display, "monitor-added",
                          G_CALLBACK(dismiss_monitor_added), window,
                          G_CONNECT_DEFAULT);
  g_signal_connect_object(display, "monitor-removed",
                          G_CALLBACK(dismiss_monitor_removed), window,
                          G_CONNECT_DEFAULT);
  g_signal_connect(window, "focus-out-event", G_CALLBACK(on_focus_out),
                   nullptr);
  return true;
}

void show_launcher_window(GtkWindow *window) {
  auto *widget = GTK_WIDGET(window);
  g_object_set_data(G_OBJECT(window), kHiding, nullptr);
  if (!gtk_widget_get_mapped(widget) && dismiss_windows(widget) != nullptr) {
    // Hyprland stacks layer surfaces in creation order. Map the click
    // catchers first so the launcher receives input on its own monitor.
    auto *display = gtk_widget_get_display(widget);
    for (int i = 0; i < gdk_display_get_n_monitors(display); ++i)
      add_dismiss_window(widget, gdk_display_get_monitor(display, i));
  }
  gtk_window_present(window);
}

void configure_settings_window(GtkWindow *window) {
  auto *widget = GTK_WIDGET(window);
  gtk_window_set_decorated(window, FALSE);
  auto *press = new Press();
  g_object_set_data_full(G_OBJECT(window), kPress, press,
                         [](gpointer data) { delete static_cast<Press *>(data); });
  auto *gesture = gtk_gesture_multi_press_new(widget);
  g_object_set_data_full(G_OBJECT(window), "remya-launcher-move-gesture",
                         gesture, g_object_unref);
  gtk_event_controller_set_propagation_phase(GTK_EVENT_CONTROLLER(gesture),
                                             GTK_PHASE_CAPTURE);
  gtk_gesture_single_set_button(GTK_GESTURE_SINGLE(gesture), 1);
  g_signal_connect(gesture, "pressed", G_CALLBACK(remember_press), press);
}

void register_launcher_window(FlView *view, GtkWindow *window) {
  g_autoptr(FlStandardMethodCodec) codec = fl_standard_method_codec_new();
  g_autoptr(FlMethodChannel) channel = fl_method_channel_new(
      fl_engine_get_binary_messenger(fl_view_get_engine(view)),
      "top.remya.launcher/window", FL_METHOD_CODEC(codec));
  fl_method_channel_set_method_call_handler(channel, window_method, window,
                                            nullptr);
  g_object_set_data_full(G_OBJECT(window), kChannel, g_steal_pointer(&channel),
                         g_object_unref);
}

void show_launcher_clipboard(GtkWindow *window) {
  // A second invocation can arrive before Flutter has attached its handler.
  if (g_object_get_data(G_OBJECT(window), kReady) == nullptr) {
    g_object_set_data(G_OBJECT(window), kShowClipboard, GINT_TO_POINTER(TRUE));
    return;
  }
  auto *channel = FL_METHOD_CHANNEL(g_object_get_data(G_OBJECT(window), kChannel));
  fl_method_channel_invoke_method(channel, "showClipboard", nullptr, nullptr,
                                  nullptr, nullptr);
}

#include "my_application.h"

#include <flutter_linux/flutter_linux.h>
#ifdef GDK_WINDOWING_X11
#include <gdk/gdkx.h>
#endif

#include "flutter/generated_plugin_registrant.h"
#include "launcher_window.h"

struct _MyApplication {
  GtkApplication parent_instance;
  char** dart_entrypoint_arguments;
};

G_DEFINE_TYPE(MyApplication, my_application, GTK_TYPE_APPLICATION)

// Called when first Flutter frame received.
static void first_frame_cb(FlView* view, gpointer) {
  gtk_widget_show(gtk_widget_get_toplevel(GTK_WIDGET(view)));
}

// Implements GApplication::activate.
static void my_application_activate(GApplication* application) {
  MyApplication* self = MY_APPLICATION(application);
  auto* gtk_application = GTK_APPLICATION(application);
  if (GList* windows = gtk_application_get_windows(gtk_application)) {
    gtk_window_present(GTK_WINDOW(windows->data));
    return;
  }
  GtkWindow* window = GTK_WINDOW(gtk_application_window_new(gtk_application));
  gboolean preview = FALSE;
  gboolean settings = FALSE;
  for (char** argument = self->dart_entrypoint_arguments;
       argument && *argument; ++argument) {
    if (g_strcmp0(*argument, "--settings") == 0) settings = TRUE;
    if (g_str_has_prefix(*argument, "--preview=")) preview = TRUE;
  }
  const bool overlay = !preview && !settings && configure_launcher_overlay(window);

  // Use a header bar when running in GNOME as this is the common style used
  // by applications and is the setup most users will be using (e.g. Ubuntu
  // desktop).
  // If running on X and not using GNOME then just use a traditional title bar
  // in case the window manager does more exotic layout, e.g. tiling.
  // If running on Wayland assume the header bar will work (may need changing
  // if future cases occur).
  // Settings is an ordinary xdg toplevel without the overlay's focus-out close.
  const char* title = settings ? "Remy'a Launcher Settings" : "Remy'a Launcher";
  gboolean use_header_bar = TRUE;
#ifdef GDK_WINDOWING_X11
  GdkScreen* screen = gtk_window_get_screen(window);
  if (GDK_IS_X11_SCREEN(screen)) {
    const gchar* wm_name = gdk_x11_screen_get_window_manager_name(screen);
    if (g_strcmp0(wm_name, "GNOME Shell") != 0) {
      use_header_bar = FALSE;
    }
  }
#endif
  // The header bar title is only drawn; the compositor reads the window title.
  gtk_window_set_title(window, title);
  // Settings draws its own title bar, so it stays undecorated.
  if (settings) {
    configure_settings_window(window);
  } else if (!overlay && use_header_bar) {
    GtkHeaderBar* header_bar = GTK_HEADER_BAR(gtk_header_bar_new());
    gtk_widget_show(GTK_WIDGET(header_bar));
    gtk_header_bar_set_title(header_bar, title);
    gtk_header_bar_set_show_close_button(header_bar, TRUE);
    gtk_window_set_titlebar(window, GTK_WIDGET(header_bar));
  }

  if (settings) {
    gtk_window_set_default_size(window, 760, 640);
  } else if (!overlay) {
    gtk_window_set_default_size(window, 1000, 820);
  }

  g_autoptr(FlDartProject) project = fl_dart_project_new();
  fl_dart_project_set_dart_entrypoint_arguments(
      project, self->dart_entrypoint_arguments);

  FlView* view = fl_view_new(project);
  GdkRGBA background_color;
  // Background defaults to black, override it here if necessary, e.g. #00000000
  // for transparent.
  gdk_rgba_parse(&background_color, overlay ? "#00000000" : "#141218");
  fl_view_set_background_color(view, &background_color);
  gtk_widget_show(GTK_WIDGET(view));
  gtk_container_add(GTK_CONTAINER(window), GTK_WIDGET(view));

  // Show overlay windows immediately so gtk-layer-shell receives the compositor
  // configure size before Flutter creates its initial render surface.
  if (overlay) {
    gtk_widget_show_all(GTK_WIDGET(window));
  } else {
    g_signal_connect(view, "first-frame", G_CALLBACK(first_frame_cb), nullptr);
    gtk_widget_realize(GTK_WIDGET(view));
  }

  fl_register_plugins(FL_PLUGIN_REGISTRY(view));
  if (!preview) register_launcher_window(view, window);

  gtk_widget_grab_focus(GTK_WIDGET(view));
}

// Routes subsequent Launcher invocations to the existing window over the session bus.
static int my_application_command_line(GApplication* application,
                                       GApplicationCommandLine* command_line) {
  auto* self = MY_APPLICATION(application);
  g_auto(GStrv) args =
      g_application_command_line_get_arguments(command_line, nullptr);
  const gchar* const* options = args + 1;
  GList* windows = gtk_application_get_windows(GTK_APPLICATION(application));
  if (windows != nullptr) {
    auto* window = GTK_WINDOW(windows->data);
    if (g_strv_contains(options, "--toggle")) {
      gtk_window_close(window);
    } else {
      if (g_strv_contains(options, "--clipboard")) {
        show_launcher_clipboard(window);
      }
      gtk_window_present(window);
    }
    return 0;
  }

  g_clear_pointer(&self->dart_entrypoint_arguments, g_strfreev);
  self->dart_entrypoint_arguments = g_strdupv(args + 1);
  g_application_activate(application);
  return 0;
}

// Implements GObject::dispose.
static void my_application_dispose(GObject* object) {
  MyApplication* self = MY_APPLICATION(object);
  g_clear_pointer(&self->dart_entrypoint_arguments, g_strfreev);
  G_OBJECT_CLASS(my_application_parent_class)->dispose(object);
}

static void my_application_class_init(MyApplicationClass* klass) {
  G_APPLICATION_CLASS(klass)->activate = my_application_activate;
  G_APPLICATION_CLASS(klass)->command_line = my_application_command_line;
  G_OBJECT_CLASS(klass)->dispose = my_application_dispose;
}

static void my_application_init(MyApplication* self) {}

MyApplication* my_application_new(char** arguments) {
  // Set the program name to the application ID, which helps various systems
  // like GTK and desktop environments map this running application to its
  // corresponding .desktop file. This ensures better integration by allowing
  // the application to be recognized beyond its binary name.
  g_set_prgname(APPLICATION_ID);

  auto flags = G_APPLICATION_HANDLES_COMMAND_LINE;
  for (char** arg = arguments; *arg != nullptr; ++arg) {
    if (g_str_equal(*arg, "--settings") ||
        g_str_has_prefix(*arg, "--preview=")) {
      flags = static_cast<GApplicationFlags>(flags | G_APPLICATION_NON_UNIQUE);
      break;
    }
  }
  return MY_APPLICATION(g_object_new(my_application_get_type(), "application-id",
                                    APPLICATION_ID, "flags", flags, nullptr));
}

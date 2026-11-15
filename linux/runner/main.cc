#include "my_application.h"

int main(int argc, char** argv) {
  // Desktop overlays should stay on the integrated Mesa GPU instead of waking a
  // discrete GPU via PRIME offload, which also avoids Flutter Linux's NVIDIA
  // shader compositor offset bug (#195885).
  if (g_strcmp0(g_getenv("__NV_PRIME_RENDER_OFFLOAD"), "1") == 0 &&
      g_file_test("/usr/share/glvnd/egl_vendor.d/50_mesa.json",
                  G_FILE_TEST_EXISTS)) {
    g_setenv("__NV_PRIME_RENDER_OFFLOAD", "0", TRUE);
    g_setenv("__GLX_VENDOR_LIBRARY_NAME", "mesa", TRUE);
    g_setenv("__EGL_VENDOR_LIBRARY_FILENAMES",
             "/usr/share/glvnd/egl_vendor.d/50_mesa.json", TRUE);
  }

  g_autoptr(MyApplication) app = my_application_new(argv + 1);
  return g_application_run(G_APPLICATION(app), argc, argv);
}

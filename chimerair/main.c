#include <stdint.h>

extern int32_t chimera_beam_runtime_entry(int32_t argc, const char *const *argv);

int c_main(int argc, char **argv) {
    return chimera_beam_runtime_entry((int32_t)argc, (const char *const *)argv);
}

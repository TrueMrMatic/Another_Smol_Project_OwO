#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Run SWF "director" loop for a selected file path.
// Returns 0 on normal exit back to browser.
int swf_director_run(const char* swf_path);

#ifdef __cplusplus
}
#endif
#include <3ds.h>
#include <stdio.h>

// Declare Rust functions
void* bridge_player_create();
void bridge_tick(void* ctx);

int main(int argc, char* argv[]) {
    gfxInitDefault();
    consoleInit(GFX_TOP, NULL); // Print to top screen

    printf("Initializing Ruffle...\n");
    void* player = bridge_player_create();
    printf("Ruffle initialized!\n");

    // Main loop
    while (aptMainLoop()) {
        hidScanInput();
        u32 kDown = hidKeysDown();
        if (kDown & KEY_START) break;

        // Tick Ruffle every frame
        bridge_tick(player);

        gfxFlushBuffers();
        gfxSwapBuffers();
        gspWaitForVBlank();
    }

    gfxExit();
    return 0;
}

#include <sys/types.h>
#include <stdlib.h>
#include <time.h>

// Rust requires this for HashMap initialization.
// On a real app, use the 3DS secure RNG (sslc), but this is enough to link.
ssize_t getrandom(void *buf, size_t buflen, unsigned int flags) {
    char *p = (char *)buf;
    for (size_t i = 0; i < buflen; i++) {
        p[i] = rand() % 256;
    }
    return buflen;
}
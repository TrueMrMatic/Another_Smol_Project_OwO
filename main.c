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
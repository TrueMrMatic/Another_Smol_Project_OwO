#include <3ds.h>
#include <stdio.h>
#include <stdlib.h>
#include <3ds/svc.h>

#include "sd_browser.h"
#include "swf_director.h"
#include "bridge.h"

u32 __stacksize__ = 256 * 1024; // 256 KiB

#define BASE_DIR   "sdmc:/flash"
#define MAX_FILES  256

PrintConsole conTop;
PrintConsole conBot;

static inline void global_exit_check(u32 down, u32 held) {
    if ((held & KEY_L) && (held & KEY_R) && (down & KEY_START)) {
        svcExitProcess();
    }
}

static void draw_menu(char names[][SD_NAME_MAX], int count, int sel, int top) {
    consoleSelect(&conTop);
    consoleClear();

    printf("Flash folder: %s\n", BASE_DIR);
    printf("A: run | START: exit | L+R+START: quit\n\n");

    const int LINES = 18;
    for (int i = 0; i < LINES; i++) {
        int idx = top + i;
        if (idx >= count) break;
        printf("%c %s\n", (idx == sel) ? '>' : ' ', names[idx]);
    }
    printf("\n(%d/%d)\n", sel + 1, count);
}

int main(int argc, char* argv[]) {
    gfxInitDefault();
    consoleInit(GFX_TOP, &conTop);
    consoleInit(GFX_BOTTOM, &conBot);

    consoleSelect(&conTop);
    printf("bridge: %s | 2+3=%ld\n\n", bridge_version(), (long)bridge_add(2, 3));

    static char names[MAX_FILES][SD_NAME_MAX];
    int count = sd_list_swfs(BASE_DIR, names, MAX_FILES);

    if (count <= 0) {
        printf("No .swf found in %s\n", BASE_DIR);
        printf("Create folder and put .swf files inside.\n");
        printf("Press START to exit.\n");
        while (aptMainLoop()) {
            hidScanInput();
            u32 down = hidKeysDown();
            u32 held = hidKeysHeld();
            global_exit_check(down, held);
            if (down & KEY_START) break;
            gspWaitForVBlank();
        }
        gfxExit();
        return 0;
    }

    int sel = 0;
    int top = 0;
    draw_menu(names, count, sel, top);

    char current_path[512];

    while (aptMainLoop()) {
        hidScanInput();
        u32 down = hidKeysDown();
        u32 held = hidKeysHeld();
        global_exit_check(down, held);

        if (down & KEY_START) break;

        if (down & KEY_DOWN) { if (sel < count - 1) sel++; }
        if (down & KEY_UP)   { if (sel > 0) sel--; }

        const int LINES = 18;
        if (sel < top) top = sel;
        if (sel >= top + LINES) top = sel - LINES + 1;

        if (down & KEY_A) {
            snprintf(current_path, sizeof(current_path), "%s/%s", BASE_DIR, names[sel]);
            swf_director_run(current_path);
            draw_menu(names, count, sel, top);
        }

        if (down) draw_menu(names, count, sel, top);

        gfxFlushBuffers();
        gfxSwapBuffers();
        gspWaitForVBlank();
    }

    gfxExit();
    return 0;
}
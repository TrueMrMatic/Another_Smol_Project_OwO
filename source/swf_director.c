#include <3ds.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <3ds/svc.h>

#include "bridge.h"
#include "swf_director.h"

// Consoles are owned by main.c
extern PrintConsole conTop;
extern PrintConsole conBot;

// Hard-exit combo: hold L+R then press START
static inline void global_exit_check(u32 down, u32 held) {
    if ((held & KEY_L) && (held & KEY_R) && (down & KEY_START)) {
        svcExitProcess();
    }
}

static void wait_keys_released(void) {
    while (aptMainLoop()) {
        hidScanInput();
        if (hidKeysHeld() == 0) break;
        gspWaitForVBlank();
    }
}

// 3DS top framebuffer is "sideways": treat (x,y) as physical 400x240 coords.
static inline void putpx_rgb565_phys(u8* fb, int x, int y, u16 c) {
    if ((unsigned)x >= 400u || (unsigned)y >= 240u) return;
    ((u16*)fb)[(x * 240) + (239 - y)] = c;
}

static inline void drawDotRGB565_phys(u8* fb, int x, int y, u16 c) {
    putpx_rgb565_phys(fb, x, y, c);
}

// Hash character_id to a bright RGB565 color
static inline u16 id_to_rgb565(uint16_t id) {
    uint32_t x = (uint32_t)id * 2654435761u;
    uint8_t r = (x >> 16) & 0xFF;
    uint8_t g = (x >>  8) & 0xFF;
    uint8_t b = (x >>  0) & 0xFF;
    r |= 0x40; g |= 0x40; b |= 0x40;
    return (u16)(((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3));
}

// Fit stage pixels into 400x240, preserving aspect ratio, centered.
static inline void map_stage_to_top_fit(
    int32_t x, int32_t y,
    int32_t stage_w, int32_t stage_h,
    int* sx, int* sy
) {
    if (stage_w <= 0 || stage_h <= 0) {
        int xx = x, yy = y;
        if (xx < 0) xx = 0; 
		if (xx > 399) xx = 399;
        if (yy < 0) yy = 0; 
		if (yy > 239) yy = 239;
        *sx = xx; *sy = yy;
        return;
    }

    // scale = min(400/stage_w, 240/stage_h) in 16.16 fixed
    int64_t s1 = (400LL * 65536LL) / stage_w;
    int64_t s2 = (240LL * 65536LL) / stage_h;
    int64_t s = (s1 < s2) ? s1 : s2;

    int64_t scaled_w = (stage_w * s) >> 16;
    int64_t scaled_h = (stage_h * s) >> 16;

    int64_t ox = (400 - scaled_w) / 2;
    int64_t oy = (240 - scaled_h) / 2;

    int64_t xx = ox + (((int64_t)x * s) >> 16);
    int64_t yy = oy + (((int64_t)y * s) >> 16);

    if (xx < 0) xx = 0; 
	if (xx > 399) xx = 399;
    if (yy < 0) yy = 0; 
	if (yy > 239) yy = 239;

    *sx = (int)xx;
    *sy = (int)yy;
}

static u8* load_file(const char* path, size_t* out_len) {
    *out_len = 0;
    FILE* f = fopen(path, "rb");
    if (!f) return NULL;
    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    fseek(f, 0, SEEK_SET);
    if (sz <= 0) { fclose(f); return NULL; }

    u8* buf = (u8*)malloc((size_t)sz);
    if (!buf) { fclose(f); return NULL; }

    size_t rd = fread(buf, 1, (size_t)sz, f);
    fclose(f);

    if (rd != (size_t)sz) {
        free(buf);
        return NULL;
    }
    *out_len = (size_t)sz;
    return buf;
}

int swf_director_run(const char* swf_path) {
    consoleSelect(&conBot);
    consoleClear();
    printf("Loading: %s\n", swf_path);

    size_t swf_len = 0;
    u8* swf_buf = load_file(swf_path, &swf_len);
    if (!swf_buf) {
        printf("ERROR: failed to read SWF.\nB: back\n");
        while (aptMainLoop()) {
            hidScanInput();
            u32 d = hidKeysDown();
            u32 h = hidKeysHeld();
            global_exit_check(d, h);
            if (d & KEY_B) break;
            gspWaitForVBlank();
        }
        wait_keys_released();
        consoleSelect(&conTop);
        consoleClear();
        return 0;
    }

    int32_t stage_w = 0, stage_h = 0;
    uint16_t total_frames = 0;
    uint32_t total_instances = 0;

    BridgePlayer* player = bridge_player_create(
        swf_buf, swf_len,
        &stage_w, &stage_h,
        &total_frames,
        &total_instances
    );

    free(swf_buf); // Rust copies internally

    if (!player) {
        consoleSelect(&conBot);
        printf("ERROR: bridge_player_create failed.\nB: back\n");
        while (aptMainLoop()) {
            hidScanInput();
            u32 d = hidKeysDown();
            u32 h = hidKeysHeld();
            global_exit_check(d, h);
            if (d & KEY_B) break;
            gspWaitForVBlank();
        }
        wait_keys_released();
        consoleSelect(&conTop);
        consoleClear();
        return 0;
    }

    consoleSelect(&conBot);
    printf("stage=%dx%d frames=%u total_inst=%lu\n",
           (int)stage_w, (int)stage_h, (unsigned)total_frames, (unsigned long)total_instances);
    printf("START: pause | LEFT/RIGHT (paused): step | Y: log | B: back | L+R+START: quit\n");

    enum { MAX_INST_PER_FRAME = 50000 };
    BridgeDlInstance* inst = (BridgeDlInstance*)malloc(sizeof(BridgeDlInstance) * MAX_INST_PER_FRAME);
    if (!inst) {
        printf("OOM: inst buffer\n");
        bridge_player_destroy(player);
        wait_keys_released();
        consoleSelect(&conTop);
        consoleClear();
        return 0;
    }

#if defined(KEY_DLEFT) && defined(KEY_DRIGHT)
#   define STEP_LEFT  KEY_DLEFT
#   define STEP_RIGHT KEY_DRIGHT
#else
#   define STEP_LEFT  KEY_LEFT
#   define STEP_RIGHT KEY_RIGHT
#endif

    int frame = 0;
    bool paused = false;

    while (aptMainLoop()) {
        hidScanInput();
        u32 down = hidKeysDown();
        u32 held = hidKeysHeld();
        global_exit_check(down, held);

        if (down & KEY_B) break;

        if (down & KEY_START) {
            paused = !paused;
            consoleSelect(&conBot);
            printf("paused=%d\n", paused ? 1 : 0);
        }

        if (paused && total_frames > 0) {
            if (down & STEP_RIGHT) { if (frame < (int)total_frames - 1) frame++; }
            if (down & STEP_LEFT)  { if (frame > 0) frame--; }
        }

        uint32_t cnt_u32 = 0;
        int32_t rc = bridge_player_get_frame_instances(
            player, frame, inst, MAX_INST_PER_FRAME, &cnt_u32
        );

        if (rc < 0) {
            consoleSelect(&conBot);
            printf("bridge_player_get_frame_instances rc=%ld\nB: back\n", (long)rc);
            while (aptMainLoop()) {
                hidScanInput();
                u32 d = hidKeysDown();
                u32 h = hidKeysHeld();
                global_exit_check(d, h);
                if (d & KEY_B) break;
                gspWaitForVBlank();
            }
            break;
        }

        if ((down & KEY_Y) && total_frames > 0) {
            consoleSelect(&conBot);
            printf("frame=%d/%u count=%lu\n", frame, (unsigned)total_frames, (unsigned long)cnt_u32);
            uint32_t sample = (cnt_u32 > 12) ? 12 : cnt_u32;
            for (uint32_t i = 0; i < sample; i++) {
                BridgeDlInstance* p = &inst[i];
                int sx, sy;
                map_stage_to_top_fit(p->x_px, p->y_px, stage_w, stage_h, &sx, &sy);
                printf("  d%u id=%u raw=(%ld,%ld) mapped=(%d,%d)\n",
                    (unsigned)p->depth, (unsigned)p->character_id,
                    (long)p->x_px, (long)p->y_px, sx, sy);
            }
            fflush(stdout);
        }

        u16 w, h;
        u8* fb = gfxGetFramebuffer(GFX_TOP, GFX_LEFT, &w, &h);
        memset(fb, 0, (size_t)w * (size_t)h * 2);

        for (uint32_t i = 0; i < cnt_u32; i++) {
            BridgeDlInstance* p = &inst[i];
            int sx, sy;
            map_stage_to_top_fit(p->x_px, p->y_px, stage_w, stage_h, &sx, &sy);
            u16 col = id_to_rgb565(p->character_id);
            drawDotRGB565_phys(fb, sx, sy, col);
        }

        if (!paused && total_frames > 0) {
            frame++;
            if (frame >= (int)total_frames) frame = 0;
        }

        gfxFlushBuffers();
        gfxSwapBuffers();
        gspWaitForVBlank();
    }

#undef STEP_LEFT
#undef STEP_RIGHT

    free(inst);
    bridge_player_destroy(player);

    wait_keys_released();
    consoleSelect(&conTop);
    consoleClear();
    return 0;
}
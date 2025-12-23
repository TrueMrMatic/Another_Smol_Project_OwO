#pragma once
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint16_t depth;
    uint16_t character_id;
    int32_t  x_px;
    int32_t  y_px;
} BridgeDlInstance;

typedef struct BridgePlayer BridgePlayer;

const char* bridge_version(void);
int64_t bridge_add(int64_t a, int64_t b);

BridgePlayer* bridge_player_create(
    const uint8_t* swf_ptr,
    size_t swf_len,
    int32_t* out_stage_w,
    int32_t* out_stage_h,
    uint16_t* out_total_frames,
    uint32_t* out_total_instances
);

void bridge_player_destroy(BridgePlayer* p);

int32_t bridge_player_get_frame_instances(
    BridgePlayer* p,
    int32_t frame_index,
    BridgeDlInstance* out_instances,
    size_t out_instances_cap,
    uint32_t* out_count
);

#ifdef __cplusplus
}
#endif
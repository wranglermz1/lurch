/* Ring layout shared by lurchcap3.dll (writer, in the guest) and drain (reader, on the host).
 * Each slot is one C_GameMovement::ProcessMovement call: raw snapshots of everything the
 * call reads or writes, and every movement trace it made with the engine's answer. */
#pragma once
#include <stdint.h>

#define LURCH_MAGIC "lurchcap-ring-3"
#define LURCH_CAP 512
#define LURCH_MAX_TRACES 48

#define SNAP_PLAYER 0x4800
#define SNAP_MV 0x200
#define SNAP_SETTINGS 0x1000
#define SNAP_GLOBALS 0x60
#define TRACE_RAY 0x68
#define TRACE_RESULT 0x180

/* One engine trace: which IEngineTrace slot, its mask argument, the Ray_t (for slot 0x40,
 * which takes no ray, its start and end vectors instead) and the trace_t it filled. */
typedef struct {
    uint32_t slot, mask;
    uint8_t ray[TRACE_RAY];
    uint8_t result[TRACE_RESULT];
} lurch_trace;

typedef struct {
    uint32_t seq, ntraces, traces_lost, pad;
    uint8_t globals[SNAP_GLOBALS];
    uint8_t settings[SNAP_SETTINGS];
    uint8_t mv_in[SNAP_MV], mv_out[SNAP_MV];
    uint8_t player_in[SNAP_PLAYER], player_out[SNAP_PLAYER];
    lurch_trace traces[LURCH_MAX_TRACES];
} lurch_rec;

/* write counts records ever written; read is advanced by drain through a guest write.
 * A record is dropped, not overwritten, while write - read == LURCH_CAP. */
typedef struct {
    char magic[16];
    uint32_t rec_size, cap;
    volatile uint32_t write, read, dropped, server_calls, trace_hooked, pad;
    lurch_rec recs[LURCH_CAP];
} lurch_ring;

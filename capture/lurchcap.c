/* lurchcap3.dll: records every C_GameMovement::ProcessMovement call of the oracle build into a
 * ring in its own .bss, for drain to read from the host. Addresses are in docs/oracle.md.
 *
 * ProcessMovement is taken over through vtable slot 2, chaining to whatever the slot held, so
 * this loads beside an earlier lurchcap. Traces are caught one level down, at the IEngineTrace
 * object's vtable, so every movement trace is seen whichever wrapper made it; only calls on
 * the movement thread while a hooked ProcessMovement is running are kept.
 *
 * The zipline the player rides is entity data, not player state: on entry the ridden zipline,
 * the entity its rope runs to, that one's next and the zipline's previous are snapshotted into
 * the call's trace list as entries with slot ENTITY_SLOT | index << 4 | chunk, ahead of the
 * real traces, so drain and the trace format need no change. */
#include <windows.h>
#include <string.h>
#include "ring.h"

#define BUILD_STAMP 0x5dba2a2fu

#define RVA_CLIENT_VTABLE 0x144a2d8
#define RVA_SERVER_VTABLE 0x14db2f0
#define RVA_GLOBALS 0x171f448
#define RVA_ENGINE_TRACE 0xd413e80

#define P_SETTINGS 0x2240
#define P_CMD 0x2f50
#define P_ZIPLINE 0x2998

#define RVA_ENTITY_LIST 0x1f33f58
#define RVA_ZIPLINE_VTABLE 0x14ab310
#define RVA_ZIPLINE_END_VTABLE 0x14a99c8
#define Z_PREV 0xd4c
#define Z_NEXT 0xd50
#define ENTITY_AT 0xa00
#define ENTITY_ZIPLINE 0x600
#define ENTITY_END 0x10
#define ENTITY_SLOT 0x1000
#define ENTITY_CHUNK (TRACE_RAY + TRACE_RESULT)
#define ENTITY_HEADER 32

typedef void(__fastcall *process_movement_fn)(void *, char *, char *);
typedef void(__fastcall *trace5_fn)(void *, void *, uint32_t, void *, void *);
typedef void(__fastcall *trace4_fn)(void *, void *, uint32_t, void *);
typedef void(__fastcall *trace6_fn)(void *, void *, void *, uint32_t, void *, void *);
typedef void(__fastcall *trace7_fn)(void *, void *, float *, float *, uint64_t, void *, void *);

__declspec(dllexport) lurch_ring ring;

static char *base;
static process_movement_fn next_client, next_server;
static void *engine_next[0x50 / 8];
static lurch_rec *current;
static DWORD movement_thread;

#define AT(p, off, T) (*(T *)((char *)(p) + (off)))

static void copy(void *dst, const void *src, size_t n) {
    if (src)
        memcpy(dst, src, n);
}

typedef float *(__fastcall *vec_fn)(void *);

static char *entity(uint32_t h) {
    if (h == 0xffffffffu)
        return NULL;
    char *slot = base + RVA_ENTITY_LIST + (size_t)(h & 0xffff) * 0x20;
    return AT(slot, 8, uint32_t) == h >> 16 ? AT(slot, 0, char *) : NULL;
}

/* 1 a zipline, 2 a zipline_end, 0 anything else. */
static uint32_t rope_kind(char *e) {
    if (!e)
        return 0;
    size_t vt = (size_t)(AT(e, 0, char *) - base);
    return vt == RVA_ZIPLINE_VTABLE ? 1 : vt == RVA_ZIPLINE_END_VTABLE ? 2 : 0;
}

/* Handle, kind, abs origin and abs angles (the entity's own getters, vtable 0x40 and 0x48),
 * then its fields from ENTITY_AT on, split over chunks of one trace entry each. */
static void snap_entity(lurch_rec *r, uint32_t h, uint32_t index) {
    char *e = entity(h);
    uint32_t kind = rope_kind(e);
    if (!kind)
        return;
    static uint8_t buf[ENTITY_HEADER + ENTITY_ZIPLINE];
    size_t n = kind == 1 ? ENTITY_ZIPLINE : ENTITY_END;
    void **vt = AT(e, 0, void **);
    memset(buf, 0, sizeof buf);
    memcpy(buf, &h, 4);
    memcpy(buf + 4, &kind, 4);
    memcpy(buf + 8, ((vec_fn)vt[0x40 / 8])(e), 12);
    memcpy(buf + 20, ((vec_fn)vt[0x48 / 8])(e), 12);
    memcpy(buf + ENTITY_HEADER, e + ENTITY_AT, n);
    size_t total = ENTITY_HEADER + n;
    for (uint32_t c = 0; c * ENTITY_CHUNK < total && r->ntraces < LURCH_MAX_TRACES; c++) {
        lurch_trace *t = &r->traces[r->ntraces++];
        t->slot = ENTITY_SLOT | index << 4 | c;
        t->mask = 0;
        memcpy(t->ray, buf + c * ENTITY_CHUNK, TRACE_RAY);
        memcpy(t->result, buf + c * ENTITY_CHUNK + TRACE_RAY, TRACE_RESULT);
    }
}

static void snap_ropes(lurch_rec *r, char *player) {
    uint32_t z = AT(player, P_ZIPLINE, uint32_t);
    char *ze = entity(z);
    if (rope_kind(ze) != 1)
        return;
    uint32_t next = AT(ze, Z_NEXT, uint32_t);
    char *ne = entity(next);
    snap_entity(r, z, 0);
    snap_entity(r, next, 1);
    if (rope_kind(ne) == 1)
        snap_entity(r, AT(ne, Z_NEXT, uint32_t), 2);
    snap_entity(r, AT(ze, Z_PREV, uint32_t), 3);
}

static lurch_trace *slot_for(uint32_t slot, uint32_t mask) {
    lurch_rec *r = current;
    if (!r || GetCurrentThreadId() != movement_thread)
        return NULL;
    if (r->ntraces == LURCH_MAX_TRACES) {
        r->traces_lost++;
        return NULL;
    }
    lurch_trace *t = &r->traces[r->ntraces++];
    t->slot = slot;
    t->mask = mask;
    return t;
}

static void __fastcall trace18(void *self, void *ray, uint32_t mask, void *filter, void *tr) {
    ((trace5_fn)engine_next[0x18 / 8])(self, ray, mask, filter, tr);
    lurch_trace *t = slot_for(0x18, mask);
    if (t) {
        memcpy(t->ray, ray, TRACE_RAY);
        memcpy(t->result, tr, TRACE_RESULT);
    }
}

static void __fastcall trace20(void *self, void *ray, uint32_t mask, void *tr) {
    ((trace4_fn)engine_next[0x20 / 8])(self, ray, mask, tr);
    lurch_trace *t = slot_for(0x20, mask);
    if (t) {
        memcpy(t->ray, ray, TRACE_RAY);
        memcpy(t->result, tr, TRACE_RESULT);
    }
}

static void __fastcall trace38(void *self, void *ray, void *list, uint32_t mask, void *filter, void *tr) {
    ((trace6_fn)engine_next[0x38 / 8])(self, ray, list, mask, filter, tr);
    lurch_trace *t = slot_for(0x38, mask);
    if (t) {
        memcpy(t->ray, ray, TRACE_RAY);
        memcpy(t->result, tr, TRACE_RESULT);
    }
}

static void __fastcall trace40(void *self, void *ent, float *start, float *end, uint64_t mask, void *filter, void *tr) {
    ((trace7_fn)engine_next[0x40 / 8])(self, ent, start, end, mask, filter, tr);
    lurch_trace *t = slot_for(0x40, (uint32_t)mask);
    if (t) {
        memcpy(t->ray, start, 12);
        memcpy(t->ray + 12, end, 12);
        memcpy(t->result, tr, TRACE_RESULT);
    }
}

static int hook_engine_trace(void) {
    void **obj = AT(base, RVA_ENGINE_TRACE, void **);
    if (!obj)
        return 0;
    void **vt = *(void ***)obj;
    static void *const hooks[][2] = {
        {(void *)0x18, (void *)trace18}, {(void *)0x20, (void *)trace20},
        {(void *)0x38, (void *)trace38}, {(void *)0x40, (void *)trace40}};
    DWORD old;
    if (!VirtualProtect(vt, 0x50, PAGE_READWRITE, &old))
        return 0;
    for (size_t i = 0; i < sizeof hooks / sizeof *hooks; i++) {
        size_t s = (size_t)hooks[i][0] / 8;
        engine_next[s] = vt[s];
        InterlockedExchangePointer(&vt[s], hooks[i][1]);
    }
    VirtualProtect(vt, 0x50, old, &old);
    return 1;
}

static void __fastcall hook_client(void *self, char *player, char *mv) {
    if (!movement_thread) {
        movement_thread = GetCurrentThreadId();
        ring.trace_hooked = hook_engine_trace();
    }
    char *cmd = AT(player, P_CMD, char *);
    uint32_t w = ring.write;
    lurch_rec *r = cmd && w - ring.read < LURCH_CAP ? &ring.recs[w % LURCH_CAP] : NULL;
    if (r) {
        r->seq = AT(cmd, 0, uint32_t);
        r->ntraces = r->traces_lost = 0;
        copy(r->globals, AT(base, RVA_GLOBALS, char *), SNAP_GLOBALS);
        copy(r->settings, AT(player, P_SETTINGS, char *), SNAP_SETTINGS);
        memcpy(r->mv_in, mv, SNAP_MV);
        memcpy(r->player_in, player, SNAP_PLAYER);
        snap_ropes(r, player);
        current = r;
    } else if (cmd) {
        ring.dropped++;
    }
    next_client(self, player, mv);
    current = NULL;
    if (!r)
        return;
    memcpy(r->mv_out, mv, SNAP_MV);
    memcpy(r->player_out, player, SNAP_PLAYER);
    MemoryBarrier();
    ring.write = w + 1;
}

static void __fastcall hook_server(void *self, char *player, char *mv) {
    ring.server_calls++;
    next_server(self, player, mv);
}

static int swap_slot(uint32_t vtable_rva, void *hook, process_movement_fn *next) {
    void **slot = (void **)(base + vtable_rva) + 2;
    DWORD old;
    if (!VirtualProtect(slot, sizeof *slot, PAGE_READWRITE, &old))
        return 0;
    *next = (process_movement_fn)*slot;
    InterlockedExchangePointer(slot, hook);
    VirtualProtect(slot, sizeof *slot, old, &old);
    return 1;
}

BOOL WINAPI DllMain(HINSTANCE dll, DWORD reason, LPVOID reserved) {
    if (reason != DLL_PROCESS_ATTACH)
        return TRUE;
    base = (char *)GetModuleHandleA(NULL);
    IMAGE_NT_HEADERS *nt = (IMAGE_NT_HEADERS *)(base + ((IMAGE_DOS_HEADER *)base)->e_lfanew);
    if (nt->FileHeader.TimeDateStamp != BUILD_STAMP)
        return TRUE;
    ring.rec_size = sizeof(lurch_rec);
    ring.cap = LURCH_CAP;
    if (!swap_slot(RVA_CLIENT_VTABLE, hook_client, &next_client))
        return TRUE;
    swap_slot(RVA_SERVER_VTABLE, hook_server, &next_server);
    memcpy(ring.magic, LURCH_MAGIC, sizeof LURCH_MAGIC);
    return TRUE;
}

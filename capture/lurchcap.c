/* lurchcap3.dll: records every C_GameMovement::ProcessMovement call of the oracle build into a
 * ring in its own .bss, for drain to read from the host. Addresses are in docs/oracle.md.
 *
 * ProcessMovement is taken over through vtable slot 2, chaining to whatever the slot held, so
 * this loads beside an earlier lurchcap. Traces are caught one level down, at the IEngineTrace
 * object's vtable, so every movement trace is seen whichever wrapper made it; only calls on
 * the movement thread while a hooked ProcessMovement is running are kept. */
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

/* lurchcap.dll: records every C_GameMovement::ProcessMovement call of the oracle build into a
 * ring in its own .bss, for drain to read from the host. Addresses are in docs/oracle.md.
 *
 * The hook replaces vtable slot 2 rather than patching code: RunCommand reaches
 * ProcessMovement only through that slot, so no trampoline is needed. */
#include <windows.h>
#include "ring.h"

#define BUILD_STAMP 0x5dba2a2fu

#define RVA_CLIENT_VTABLE 0x144a2d8
#define RVA_SERVER_VTABLE 0x14db2f0
#define RVA_PROCESS_MOVEMENT 0x805ee0
#define RVA_SERVER_PROCESS_MOVEMENT 0xb8abf0
#define RVA_WEAPON_SPEED_SCALE 0x8049e0
#define RVA_SV_GRAVITY 0x231ed78
#define RVA_GLOBALS 0x171f448
#define RVA_EXTRA_AIR_ACCEL 0x2316758
#define RVA_STANCE_BLOCK 0x2387f6c
#define RVA_STANCE_STRIDE 0x2387f74

#define P_FLAGS98 0x98
#define P_GRAVITY 0x3c8
#define P_STANCE_GATE 0x3e0
#define P_GROUND 0x3dc
#define P_MOVETYPE 0x452
#define P_ACCEL_SCALE 0x1a84
#define P_SETTINGS 0x2240
#define P_SPRINT 0x2604
#define P_RAMP_END 0x261c
#define P_NORMAL 0x266c
#define P_FRICTION_SCALE 0x26ac
#define P_SLIDE 0x2815
#define P_SPEED_SCALE 0x2cb8
#define P_CMD 0x2f50
#define P_AIR_SPEED 0x3108
#define P_AIR_ACCEL 0x310c
#define P_LAGGED 0x358c
#define P_TIME_SCALE 0x3a60

#define MV_ANGLES 0x0c
#define MV_BUTTONS 0x24
#define MV_OLDBUTTONS 0x28
#define MV_MOVE 0x30
#define MV_MAXSPEED 0xd4
#define MV_ORIGIN 0x118
#define MV_VELOCITY 0x124

#define CMD_SEQ 0x000
#define CMD_FRAMETIME 0x270

typedef void(__fastcall *process_movement_fn)(void *, char *, char *);
typedef float(__fastcall *player_fn)(char *);

__declspec(dllexport) lurch_ring ring;

static char *base;
static process_movement_fn orig_client, orig_server;

#define AT(p, off, T) (*(T *)((char *)(p) + (off)))

static float setting(char *settings, uint32_t global_rva, int is_byte) {
    char *f = settings + AT(base, global_rva, uint32_t);
    return is_byte ? (float)*(uint8_t *)f : *(float *)f;
}

static int stance_of(char *player) {
    return AT(player, P_STANCE_GATE, int) < 1 ? 2 : (AT(player, P_FLAGS98, uint8_t) >> 1) & 1;
}

static char *stance_block(char *player) {
    char *s = AT(player, P_SETTINGS, char *);
    uint32_t off = AT(base, RVA_STANCE_BLOCK, uint32_t);
    if ((int32_t)off < 0)
        off = AT(s, (off & 0x7fffffff) + 4, uint32_t);
    return s + off + AT(base, RVA_STANCE_STRIDE, uint32_t) * stance_of(player);
}

static void sample_state(lurch_state *st, char *player, char *mv) {
    for (int i = 0; i < 3; i++) {
        st->origin[i] = AT(mv, MV_ORIGIN + 4 * i, float);
        st->velocity[i] = AT(mv, MV_VELOCITY + 4 * i, float);
    }
    st->flags = (AT(player, P_SPRINT, uint8_t) ? FLAG_SPRINT : 0) |
                ((AT(player, P_FLAGS98, uint8_t) & 2) ? FLAG_DUCKED : 0) |
                (AT(player, P_SLIDE, uint8_t) ? FLAG_SLIDE : 0);
    st->onground = AT(player, P_GROUND, uint32_t) != 0xffffffffu;
}

static void sample_aux(float *a, char *player) {
    char *s = AT(player, P_SETTINGS, char *);
    char *sb = stance_block(player);
    float g = AT(player, P_GRAVITY, float);
    a[AUX_gravity] = g;
    a[AUX_speedScale] = AT(player, P_SPEED_SCALE, float);
    a[AUX_accelScale] = AT(player, P_ACCEL_SCALE, float);
    a[AUX_timeScale] = AT(player, P_TIME_SCALE, float);
    a[AUX_frictionScale] = AT(player, P_FRICTION_SCALE, float);
    a[AUX_lagged] = AT(player, P_LAGGED, float);
    a[AUX_airSpeed] = AT(player, P_AIR_SPEED, float);
    a[AUX_airAcceleration] = AT(player, P_AIR_ACCEL, float);
    a[AUX_normalX] = AT(player, P_NORMAL, float);
    a[AUX_normalY] = AT(player, P_NORMAL + 4, float);
    a[AUX_normalZ] = AT(player, P_NORMAL + 8, float);
    a[AUX_frictionRampEnd] = AT(player, P_RAMP_END, float);
    a[AUX_weaponSpeedScale] = ((player_fn)(base + RVA_WEAPON_SPEED_SCALE))(player);
    a[AUX_movetype] = AT(player, P_MOVETYPE, uint8_t);
    a[AUX_stance] = (float)stance_of(player);
    a[AUX_lowSpeed] = AT(sb, 0x44, float);
    a[AUX_speed] = AT(sb, 0x48, float);
    a[AUX_sprintspeed] = AT(sb, 0x4c, float);
    a[AUX_acceleration] = AT(sb, 0x50, float);
    a[AUX_deceleration] = AT(sb, 0x54, float);
    a[AUX_lowAcceleration] = AT(sb, 0x58, float);
    a[AUX_sprintAcceleration] = AT(sb, 0x5c, float);
    a[AUX_sprintDeceleration] = AT(sb, 0x60, float);
    a[AUX_speedScaleSide] = setting(s, 0x2387320, 0);
    a[AUX_speedScaleBack] = setting(s, 0x23879d8, 0);
    a[AUX_gravityScale] = setting(s, 0x2385f98, 0);
    a[AUX_airDrag] = setting(s, 0x23878e8, 0);
    a[AUX_airDragConstant] = setting(s, 0x2387098, 0);
    a[AUX_airStrafeEnabled] = setting(s, 0x2387a38, 1);
    a[AUX_sv_gravity] = AT(AT(base, RVA_SV_GRAVITY, char *), 0x68, float);
    a[AUX_player_extraairaccelleration] = AT(AT(base, RVA_EXTRA_AIR_ACCEL, char *), 0x68, float);
}

static void __fastcall hook_client(void *self, char *player, char *mv) {
    char *cmd = AT(player, P_CMD, char *);
    uint32_t w = ring.write;
    lurch_rec *r = w - ring.read < LURCH_CAP ? &ring.recs[w % LURCH_CAP] : NULL;
    if (r && cmd) {
        r->seq = AT(cmd, CMD_SEQ, uint32_t);
        r->frametime = AT(cmd, CMD_FRAMETIME, float);
        r->curtime = AT(AT(base, RVA_GLOBALS, char *), 0x10, float);
        for (int i = 0; i < 3; i++) {
            r->angles[i] = AT(mv, MV_ANGLES + 4 * i, float);
            r->move[i] = AT(mv, MV_MOVE + 4 * i, float);
        }
        r->buttons = AT(mv, MV_BUTTONS, uint32_t);
        r->oldbuttons = AT(mv, MV_OLDBUTTONS, uint32_t);
        sample_state(&r->in, player, mv);
        sample_aux(r->aux, player);
    }
    orig_client(self, player, mv);
    if (!cmd)
        return;
    if (!r) {
        ring.dropped++;
        return;
    }
    sample_state(&r->out, player, mv);
    r->maxspeed = AT(mv, MV_MAXSPEED, float);
    MemoryBarrier();
    ring.write = w + 1;
}

static void __fastcall hook_server(void *self, char *player, char *mv) {
    ring.server_calls++;
    orig_server(self, player, mv);
}

static int swap_slot(uint32_t vtable_rva, uint32_t expect_rva, void *hook, process_movement_fn *orig) {
    void **slot = (void **)(base + vtable_rva) + 2;
    DWORD old;
    if (*slot != base + expect_rva)
        return 0;
    *orig = (process_movement_fn)*slot;
    if (!VirtualProtect(slot, sizeof *slot, PAGE_READWRITE, &old))
        return 0;
    InterlockedExchangePointer(slot, hook);
    VirtualProtect(slot, sizeof *slot, old, &old);
    return 1;
}

static void say(const char *msg) {
    OutputDebugStringA(msg);
}

BOOL WINAPI DllMain(HINSTANCE dll, DWORD reason, LPVOID reserved) {
    if (reason != DLL_PROCESS_ATTACH)
        return TRUE;
    base = (char *)GetModuleHandleA(NULL);
    IMAGE_NT_HEADERS *nt = (IMAGE_NT_HEADERS *)(base + ((IMAGE_DOS_HEADER *)base)->e_lfanew);
    if (nt->FileHeader.TimeDateStamp != BUILD_STAMP) {
        say("lurchcap: not build 5dba2a2f, not hooking\n");
        return TRUE;
    }
    ring.rec_size = sizeof(lurch_rec);
    ring.cap = LURCH_CAP;
    if (!swap_slot(RVA_CLIENT_VTABLE, RVA_PROCESS_MOVEMENT, hook_client, &orig_client)) {
        say("lurchcap: client ProcessMovement slot is not the expected function, not hooking\n");
        return TRUE;
    }
    if (!swap_slot(RVA_SERVER_VTABLE, RVA_SERVER_PROCESS_MOVEMENT, hook_server, &orig_server))
        say("lurchcap: server ProcessMovement slot unexpected, server calls not counted\n");
    memcpy(ring.magic, LURCH_MAGIC, sizeof LURCH_MAGIC);
    say("lurchcap: hooked\n");
    return TRUE;
}

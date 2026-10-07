/* Ring layout shared by lurchcap.dll (writer, in the guest) and drain (reader, on the host).
 * Private to the capture side: the text trace is the only contract with the Rust code. */
#pragma once
#include <stdint.h>

#define LURCH_MAGIC "lurchcap-ring-1"
#define LURCH_CAP 8192

/* Values sampled per call that drain turns into the trace's settings line. Each must hold
 * still for a whole recording; drain fails the recording if one moves. */
#define LURCH_AUX(X)                  \
    X(gravity)                        \
    X(speedScale)                     \
    X(accelScale)                     \
    X(timeScale)                      \
    X(frictionScale)                  \
    X(lagged)                         \
    X(airSpeed)                       \
    X(airAcceleration)                \
    X(normalX)                        \
    X(normalY)                        \
    X(normalZ)                        \
    X(frictionRampEnd)                \
    X(weaponSpeedScale)               \
    X(movetype)                       \
    X(stance)                         \
    X(lowSpeed)                       \
    X(speed)                          \
    X(sprintspeed)                    \
    X(acceleration)                   \
    X(deceleration)                   \
    X(lowAcceleration)                \
    X(sprintAcceleration)             \
    X(sprintDeceleration)             \
    X(speedScaleSide)                 \
    X(speedScaleBack)                 \
    X(gravityScale)                   \
    X(airDrag)                        \
    X(airDragConstant)                \
    X(airStrafeEnabled)               \
    X(sv_gravity)                     \
    X(player_extraairaccelleration)

#define LURCH_AUX_ENUM(n) AUX_##n,
enum { LURCH_AUX(LURCH_AUX_ENUM) AUX_COUNT };

#define FLAG_SPRINT 1u
#define FLAG_DUCKED 2u
#define FLAG_SLIDE 4u

typedef struct {
    float origin[3], velocity[3];
    uint32_t flags, onground;
} lurch_state;

typedef struct {
    uint32_t seq;
    float frametime, curtime;
    float angles[3], move[3];
    uint32_t buttons, oldbuttons;
    lurch_state in, out;
    float maxspeed;
    float aux[AUX_COUNT];
} lurch_rec;

/* write counts records ever written; read is advanced by drain through a guest write.
 * A record is dropped, not overwritten, while write - read == LURCH_CAP. */
typedef struct {
    char magic[16];
    uint32_t rec_size, cap;
    volatile uint32_t write, read, dropped, server_calls;
    lurch_rec recs[LURCH_CAP];
} lurch_ring;

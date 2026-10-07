// drain OUT.ltr [--all] [--map NAME] [--seconds N]: follow lurchcap3's ring in the guest and
// write a binary trace when stopped (Ctrl-C or --seconds). Built against HAVM's reader
// (capture/Makefile). PROC overrides the process name.
//
// File: header, then one record per ProcessMovement call. Snapshots are stored as runs of
// bytes that differ from a reference, so a record costs what actually changed:
//   header  "lurchtr4" u32 player mv settings globals trace_ray trace_result, char map[32]
//   record  u32 seq ntraces traces_lost
//           globals    vs previous record's globals
//           settings   vs previous record's settings
//           mv_in      vs previous record's mv_out (one call starts where the last ended)
//           mv_out     vs this record's mv_in
//           player_in  vs previous record's player_out
//           player_out vs this record's player_in
//           ntraces x { u32 slot, u32 mask, ray runs, result runs } each vs the previous trace
//   runs    { u16 offset, u16 length, bytes } ... terminated by length 0; differing bytes
//           closer than 7 apart share a run (src/ltr.rs reads this, and writes it too)
//
// By default only the first execution of each seq is kept: prediction re-runs a command it
// has already run, from corrected state.
#include "vm/vmm.hpp"
extern "C" {
#include "ring.h"
}

#include <atomic>
#include <chrono>
#include <csignal>
#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <set>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

namespace {

std::atomic<bool> stop{false};

uint64_t find_ring(const Process& game) {
    const uint64_t dll = game.module_base("lurchcap3.dll");
    if (!dll) throw std::runtime_error("lurchcap3.dll is not loaded");
    const uint32_t lfanew = game.read<uint32_t>(dll + 0x3c);
    const uint32_t size = game.read<uint32_t>(dll + lfanew + 24 + 56);
    std::vector<char> img(size);
    game.read_partial(dll, img.data(), size);
    // The magic also sits in .rdata as the literal the DLL copies from; the ring is the hit
    // whose header carries this drain's record size.
    for (size_t at = 0; at + sizeof(lurch_ring::magic) + 8 <= size; at++) {
        const char* hit = (const char*)memmem(img.data() + at, size - at, LURCH_MAGIC, sizeof LURCH_MAGIC);
        if (!hit) break;
        at = hit - img.data();
        uint32_t rec_size;
        memcpy(&rec_size, hit + offsetof(lurch_ring, rec_size), 4);
        if (rec_size == sizeof(lurch_rec)) return dll + at;
    }
    throw std::runtime_error("lurchcap3.dll is loaded but has no ring with this drain's layout");
}

void put32(FILE* f, uint32_t v) { fwrite(&v, 4, 1, f); }

void runs(FILE* f, const uint8_t* now, const uint8_t* ref, uint32_t n) {
    const uint32_t merge_gap = 6;
    static const uint8_t zeros[SNAP_PLAYER] = {};
    if (!ref) ref = zeros;
    for (uint32_t i = 0; i < n;) {
        if (now[i] == ref[i]) {
            i++;
            continue;
        }
        uint32_t j = i + 1, same = 0;
        while (j < n && j - i < 0xffff) {
            same = now[j] == ref[j] ? same + 1 : 0;
            j++;
            if (same > merge_gap) break;
        }
        const uint32_t end = j - same;
        const uint16_t hdr[2] = {(uint16_t)i, (uint16_t)(end - i)};
        fwrite(hdr, 2, 2, f);
        fwrite(now + i, 1, end - i, f);
        i = end;
    }
    const uint16_t done[2] = {0, 0};
    fwrite(done, 2, 2, f);
}

}  // namespace

int main(int argc, char** argv) {
    const char* out = nullptr;
    const char* map = "unknown";
    bool all = false;
    double seconds = 0;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--all")) all = true;
        else if (!strcmp(argv[i], "--map") && i + 1 < argc) map = argv[++i];
        else if (!strcmp(argv[i], "--seconds") && i + 1 < argc) seconds = atof(argv[++i]);
        else out = argv[i];
    }
    if (!out) {
        fprintf(stderr, "usage: drain OUT.ltr [--all] [--map NAME] [--seconds N]\n");
        return 2;
    }
    signal(SIGINT, [](int) { stop = true; });
    try {
        const char* proc = getenv("PROC") ? getenv("PROC") : "r5apex.exe";
        Vmm vm = Vmm::open_qemu();
        Process game = vm.process(proc);
        const uint64_t ring = find_ring(game);
        if (!game.read<uint32_t>(ring + offsetof(lurch_ring, trace_hooked)))
            fprintf(stderr, "warning: the trace detour is not in place; records carry no traces\n");
        uint32_t read = game.read<uint32_t>(ring + offsetof(lurch_ring, write));
        game.write_bytes(ring + offsetof(lurch_ring, read), &read, 4);
        const uint32_t dropped0 = game.read<uint32_t>(ring + offsetof(lurch_ring, dropped));
        const uint32_t server0 = game.read<uint32_t>(ring + offsetof(lurch_ring, server_calls));
        fprintf(stderr, "draining %s ring at %#llx\n", proc, (unsigned long long)ring);

        FILE* f = fopen(out, "wb");
        if (!f) throw std::runtime_error(std::string("cannot write ") + out);
        fwrite("lurchtr4", 1, 8, f);
        for (uint32_t v : {SNAP_PLAYER, SNAP_MV, SNAP_SETTINGS, SNAP_GLOBALS, TRACE_RAY, TRACE_RESULT}) put32(f, v);
        char mapname[32] = {};
        strncpy(mapname, map, sizeof mapname - 1);
        fwrite(mapname, 1, sizeof mapname, f);

        auto r = std::make_unique<lurch_rec>();
        auto prev = std::make_unique<lurch_rec>();
        bool have_prev = false;
        lurch_trace last_trace{};
        bool have_trace = false;
        std::set<uint32_t> seen;
        size_t calls = 0, kept = 0, gaps = 0, lost = 0;
        uint32_t last_seq = 0;
        const auto t0 = std::chrono::steady_clock::now();
        while (!stop && (seconds <= 0 || std::chrono::steady_clock::now() - t0 < std::chrono::duration<double>(seconds))) {
            const uint32_t write = game.read<uint32_t>(ring + offsetof(lurch_ring, write));
            for (; read != write; read++) {
                const uint64_t va = ring + offsetof(lurch_ring, recs) + (uint64_t)(read % LURCH_CAP) * sizeof(lurch_rec);
                // Read the fixed part, then only the traces the call made: the unused trace
                // slots are pages the DLL never touched, which do not exist to be read. A record
                // can also reach pages only just faulted in, which MemProcFS's page-table cache
                // still holds as unmapped until it is refreshed.
                const size_t fixed = offsetof(lurch_rec, traces);
                auto fetch = [&](uint64_t at, void* dst, size_t n) {
                    for (int tries = 0; game.read_partial(at, dst, n) != n; tries++) {
                        if (tries == 20) throw std::runtime_error("ring read failed");
                        VMMDLL_ConfigSet(vm.handle(), VMMDLL_OPT_REFRESH_FREQ_TLB, 1);
                        std::this_thread::sleep_for(std::chrono::milliseconds(5));
                    }
                };
                fetch(va, r.get(), fixed);
                if (r->ntraces > LURCH_MAX_TRACES) throw std::runtime_error("corrupt record: ntraces");
                if (r->ntraces) fetch(va + fixed, r->traces, r->ntraces * sizeof(lurch_trace));
                calls++;
                if (!all && !seen.insert(r->seq).second) continue;
                if (have_prev && !all && r->seq != last_seq + 1) gaps++;
                last_seq = r->seq;
                lost += r->traces_lost;
                const lurch_rec* p = have_prev ? prev.get() : nullptr;
                put32(f, r->seq);
                put32(f, r->ntraces);
                put32(f, r->traces_lost);
                runs(f, r->globals, p ? p->globals : nullptr, SNAP_GLOBALS);
                runs(f, r->settings, p ? p->settings : nullptr, SNAP_SETTINGS);
                runs(f, r->mv_in, p ? p->mv_out : nullptr, SNAP_MV);
                runs(f, r->mv_out, r->mv_in, SNAP_MV);
                runs(f, r->player_in, p ? p->player_out : nullptr, SNAP_PLAYER);
                runs(f, r->player_out, r->player_in, SNAP_PLAYER);
                for (uint32_t k = 0; k < r->ntraces; k++) {
                    const lurch_trace& t = r->traces[k];
                    fwrite(&t.slot, 4, 1, f);
                    fwrite(&t.mask, 4, 1, f);
                    runs(f, t.ray, have_trace ? last_trace.ray : nullptr, TRACE_RAY);
                    runs(f, t.result, have_trace ? last_trace.result : nullptr, TRACE_RESULT);
                    last_trace = t;
                    have_trace = true;
                }
                std::swap(r, prev);
                have_prev = true;
                kept++;
            }
            game.write_bytes(ring + offsetof(lurch_ring, read), &read, 4);
            std::this_thread::sleep_for(std::chrono::milliseconds(2));
        }
        fclose(f);
        const uint32_t dropped = game.read<uint32_t>(ring + offsetof(lurch_ring, dropped)) - dropped0;
        const uint32_t server = game.read<uint32_t>(ring + offsetof(lurch_ring, server_calls)) - server0;
        fprintf(stderr, "%zu calls, %zu written, %zu seq gaps, %u dropped in the guest, %zu traces lost, %u server calls\n",
                calls, kept, gaps, dropped, lost, server);
    } catch (const std::exception& e) {
        fprintf(stderr, "drain: %s\n", e.what());
        return 1;
    }
    return 0;
}

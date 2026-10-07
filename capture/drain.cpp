// drain OUT.trace [--all] [--map NAME]: follow lurchcap's ring in the guest and write a text
// trace on Ctrl-C. Built against HAVM's reader (capture/Makefile). PROC overrides the process.
//
// By default only the first execution of each seq is kept: prediction re-runs a command it
// has already run, and the re-run starts from corrected state, not from the previous out.
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
#include <set>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

namespace {

std::atomic<bool> stop{false};

#define LURCH_AUX_NAME(n) #n,
const char* const AUX_NAMES[] = {LURCH_AUX(LURCH_AUX_NAME)};

uint32_t bits(float f) {
    uint32_t u;
    memcpy(&u, &f, 4);
    return u;
}

uint64_t find_ring(const Process& game) {
    const uint64_t dll = game.module_base("lurchcap.dll");
    if (!dll) throw std::runtime_error("lurchcap.dll is not loaded");
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
    throw std::runtime_error("lurchcap.dll is loaded but has no ring with this drain's layout: it did not hook, or ring.h differs");
}

void state(FILE* f, const char* tag, const lurch_state& s) {
    fprintf(f, "%s", tag);
    for (float v : s.origin) fprintf(f, " %08x", bits(v));
    for (float v : s.velocity) fprintf(f, " %08x", bits(v));
    fprintf(f, " %x %x", s.flags, s.onground);
}

}  // namespace

int main(int argc, char** argv) {
    const char* out = nullptr;
    const char* map = "unknown";
    bool all = false;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--all")) all = true;
        else if (!strcmp(argv[i], "--map") && i + 1 < argc) map = argv[++i];
        else out = argv[i];
    }
    if (!out) {
        fprintf(stderr, "usage: drain OUT.trace [--all] [--map NAME]\n");
        return 2;
    }
    signal(SIGINT, [](int) { stop = true; });
    try {
        const char* proc = getenv("PROC") ? getenv("PROC") : "r5apex.exe";
        Vmm vm = Vmm::open_qemu();
        Process game = vm.process(proc);
        const uint64_t ring = find_ring(game);
        const uint32_t rec_size = game.read<uint32_t>(ring + offsetof(lurch_ring, rec_size));
        const uint32_t cap = game.read<uint32_t>(ring + offsetof(lurch_ring, cap));
        if (rec_size != sizeof(lurch_rec) || cap != LURCH_CAP)
            throw std::runtime_error("ring layout differs from this drain's ring.h: rebuild both");
        uint32_t read = game.read<uint32_t>(ring + offsetof(lurch_ring, write));
        game.write_bytes(ring + offsetof(lurch_ring, read), &read, 4);
        const uint32_t dropped0 = game.read<uint32_t>(ring + offsetof(lurch_ring, dropped));
        const uint32_t server0 = game.read<uint32_t>(ring + offsetof(lurch_ring, server_calls));
        fprintf(stderr, "draining %s ring at %#llx; Ctrl-C to stop\n", proc, (unsigned long long)ring);

        std::vector<lurch_rec> recs;
        while (!stop) {
            const uint32_t write = game.read<uint32_t>(ring + offsetof(lurch_ring, write));
            for (; read != write; read++) {
                lurch_rec r;
                const uint64_t va = ring + offsetof(lurch_ring, recs) + (read % LURCH_CAP) * sizeof r;
                size_t got = game.read_partial(va, &r, sizeof r);
                if (got != sizeof r) {
                    // The record reached a page the DLL only just faulted in, which MemProcFS's
                    // page-table cache still holds as unmapped.
                    VMMDLL_ConfigSet(vm.handle(), VMMDLL_OPT_REFRESH_FREQ_TLB, 1);
                    got = game.read_partial(va, &r, sizeof r);
                }
                if (got != sizeof r) {
                    char m[160];
                    snprintf(m, sizeof m, "ring read failed: record %u at %#llx, %zu of %zu bytes (write %u)",
                             read, (unsigned long long)va, got, sizeof r, write);
                    throw std::runtime_error(m);
                }
                recs.push_back(r);
            }
            game.write_bytes(ring + offsetof(lurch_ring, read), &read, 4);
            std::this_thread::sleep_for(std::chrono::milliseconds(2));
        }
        const uint32_t dropped = game.read<uint32_t>(ring + offsetof(lurch_ring, dropped)) - dropped0;
        const uint32_t server = game.read<uint32_t>(ring + offsetof(lurch_ring, server_calls)) - server0;

        std::vector<lurch_rec> kept;
        std::set<uint32_t> seen;
        for (const auto& r : recs)
            if (all || seen.insert(r.seq).second) kept.push_back(r);
        if (kept.empty()) throw std::runtime_error("no records");

        for (const auto& r : kept)
            for (int k = 0; k < AUX_COUNT; k++)
                if (r.aux[k] != kept[0].aux[k]) {
                    fprintf(stderr, "%s moved from %g to %g at seq %x: not a single-settings recording, nothing written\n",
                            AUX_NAMES[k], kept[0].aux[k], r.aux[k], r.seq);
                    return 1;
                }

        FILE* f = fopen(out, "w");
        if (!f) throw std::runtime_error(std::string("cannot write ") + out);
        fprintf(f, "# lurch-trace 1 build=5dba2a2f-02ae8471 map=%s\nsettings", map);
        for (int k = 0; k < AUX_COUNT; k++) fprintf(f, " %s=%08x", AUX_NAMES[k], bits(kept[0].aux[k]));
        fprintf(f, "\n");
        uint32_t gaps = 0;
        for (size_t i = 0; i < kept.size(); i++) {
            const auto& r = kept[i];
            if (i && !all && r.seq != kept[i - 1].seq + 1) gaps++;
            fprintf(f, "cmd %x %08x %08x", r.seq, bits(r.frametime), bits(r.curtime));
            for (float v : r.angles) fprintf(f, " %08x", bits(v));
            for (float v : r.move) fprintf(f, " %08x", bits(v));
            fprintf(f, " %x %x\n", r.buttons, r.oldbuttons);
            state(f, "in ", r.in);
            fprintf(f, "\n");
            state(f, "out", r.out);
            fprintf(f, " %08x\n", bits(r.maxspeed));
        }
        fclose(f);
        fprintf(stderr, "%zu calls, %zu written, %u seq gaps, %u dropped in the guest, %u server ProcessMovement calls\n",
                recs.size(), kept.size(), gaps, dropped, server);
    } catch (const std::exception& e) {
        fprintf(stderr, "drain: %s\n", e.what());
        return 1;
    }
    return 0;
}

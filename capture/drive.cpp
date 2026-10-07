// drive STEP...: hold movement keys in the guest on a timed script, by stamping the oracle
// build's kbutton_t state from the host (HAVM's route, see HAVM src/input/kbutton.hpp).
//
//   STEP = KEYS:SECONDS, KEYS = comma-separated names or "-" for none
//   drive forward:2 -:1 forward,moveright:2 jump:0.05 -:1
//
// The kbuttons were found through each +command's ConCommand callback (docs/oracle.md).
// Every key is released on exit, Ctrl-C included.
#include "vm/vmm.hpp"

#include <chrono>
#include <csignal>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <map>
#include <set>
#include <stdexcept>
#include <string>
#include <thread>

namespace {

const std::map<std::string, uint64_t> KBUTTONS = {
    {"forward", 0xd4151a8}, {"backward", 0xd4151d0}, {"moveleft", 0xd415198}, {"moveright", 0xd4151c0},
    {"jump", 0xd415120},    {"duck", 0x281a0f20},    {"speed", 0xd415048},    {"walk", 0x281a0fd0},
    {"left", 0x281a1258},   {"right", 0x281a1220},
};
// HELD alone: KeyState reads a set LETGO bit as a key on its way up and returns 0, so the
// 0x5 HAVM stamps for attack would set the button bit with no forwardmove behind it.
constexpr uint32_t REST = 0x4, DOWN = 0x1;

volatile sig_atomic_t stop = 0;

}  // namespace

int main(int argc, char** argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: drive KEYS:SECONDS ...  keys: forward backward moveleft moveright jump duck speed walk left right, - for none\n");
        return 2;
    }
    signal(SIGINT, [](int) { stop = 1; });
    try {
        const char* proc = getenv("PROC") ? getenv("PROC") : "r5apex.exe";
        Vmm vm = Vmm::open_qemu();
        Process game = vm.process(proc);
        const uint64_t base = game.base();
        auto stamp = [&](const std::string& k, uint32_t v) {
            int32_t kb[3];
            if (!game.read_into(base + KBUTTONS.at(k), kb, sizeof kb) || (uint32_t)kb[0] > 0xffff || (uint32_t)kb[1] > 0xffff)
                throw std::runtime_error("kbutton " + k + " does not look like one; not writing");
            game.write_bytes(base + KBUTTONS.at(k) + 8, &v, 4);
        };
        std::set<std::string> held;
        for (int i = 1; i < argc && !stop; i++) {
            std::string step = argv[i];
            const size_t colon = step.rfind(':');
            if (colon == std::string::npos) throw std::runtime_error("step without :SECONDS: " + step);
            std::set<std::string> want;
            for (size_t a = 0, b; a < colon; a = b + 1) {
                b = step.find(',', a);
                if (b == std::string::npos || b > colon) b = colon;
                const std::string k = step.substr(a, b - a);
                if (k == "-") continue;
                if (!KBUTTONS.count(k)) throw std::runtime_error("unknown key " + k);
                want.insert(k);
            }
            for (const auto& k : held)
                if (!want.count(k)) stamp(k, REST);
            for (const auto& k : want) stamp(k, DOWN);
            held = want;
            const auto until = std::chrono::steady_clock::now() + std::chrono::duration<double>(atof(step.c_str() + colon + 1));
            while (!stop && std::chrono::steady_clock::now() < until) {
                for (const auto& k : held) stamp(k, DOWN);
                std::this_thread::sleep_for(std::chrono::milliseconds(2));
            }
        }
        for (const auto& [k, va] : KBUTTONS) stamp(k, REST);
    } catch (const std::exception& e) {
        fprintf(stderr, "drive: %s\n", e.what());
        return 1;
    }
    return 0;
}

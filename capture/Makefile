HAVM ?= /home/fz/HAVM/HAVM
WINCC := x86_64-w64-mingw32-gcc
WFLAGS := -O2 -Wall -Wextra -Wno-unused-parameter
HAVM_SRC := $(addprefix $(HAVM)/src/,vm/vmm.cpp vm/process.cpp vm/scatter.cpp acquire/qemu.cpp \
            acquire/memmap.cpp sig/pattern.cpp sig/scan.cpp)
HAVM_LIB := $(HAVM)/MemProcFS/files

all: lurchcap.dll inject.exe drain

lurchcap.dll: lurchcap.c ring.h
	$(WINCC) $(WFLAGS) -shared -o $@ $<

inject.exe: inject.c
	$(WINCC) $(WFLAGS) -o $@ $<

drain: drain.cpp ring.h
	g++ -O2 -std=c++17 -DLINUX -D_GNU_SOURCE -I. -I$(HAVM)/src -I$(HAVM) -I$(HAVM)/MemProcFS/includes \
	    $< $(HAVM_SRC) -o $@ -pthread -L$(HAVM_LIB) -l:vmm.so -Wl,-rpath,$(HAVM_LIB)

clean:
	rm -f lurchcap.dll inject.exe drain

.PHONY: all clean

/* inject.exe DLL [PROCESS]: load DLL into the running PROCESS (default r5apex.exe). */
#include <windows.h>
#include <tlhelp32.h>
#include <stdio.h>

static DWORD find_pid(const char *name) {
    PROCESSENTRY32 e = {.dwSize = sizeof e};
    HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
    DWORD pid = 0;
    for (BOOL ok = Process32First(snap, &e); ok && !pid; ok = Process32Next(snap, &e))
        if (!_stricmp(e.szExeFile, name))
            pid = e.th32ProcessID;
    CloseHandle(snap);
    return pid;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: inject DLL [PROCESS]\n");
        return 2;
    }
    char path[MAX_PATH];
    if (!GetFullPathNameA(argv[1], sizeof path, path, NULL) || GetFileAttributesA(path) == INVALID_FILE_ATTRIBUTES) {
        fprintf(stderr, "no such file: %s\n", argv[1]);
        return 1;
    }
    const char *target = argc > 2 ? argv[2] : "r5apex.exe";
    DWORD pid = find_pid(target);
    if (!pid) {
        fprintf(stderr, "%s is not running\n", target);
        return 1;
    }
    HANDLE proc = OpenProcess(PROCESS_ALL_ACCESS, FALSE, pid);
    if (!proc) {
        fprintf(stderr, "OpenProcess %lu failed: %lu\n", pid, GetLastError());
        return 1;
    }
    size_t n = strlen(path) + 1;
    void *remote = VirtualAllocEx(proc, NULL, n, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if (!remote || !WriteProcessMemory(proc, remote, path, n, NULL)) {
        fprintf(stderr, "writing the path failed: %lu\n", GetLastError());
        return 1;
    }
    LPTHREAD_START_ROUTINE load = (LPTHREAD_START_ROUTINE)(void *)GetProcAddress(GetModuleHandleA("kernel32.dll"), "LoadLibraryA");
    HANDLE t = CreateRemoteThread(proc, NULL, 0, load, remote, 0, NULL);
    if (!t) {
        fprintf(stderr, "CreateRemoteThread failed: %lu\n", GetLastError());
        return 1;
    }
    WaitForSingleObject(t, INFINITE);
    DWORD module = 0;
    GetExitCodeThread(t, &module);
    VirtualFreeEx(proc, remote, 0, MEM_RELEASE);
    if (!module) {
        fprintf(stderr, "LoadLibraryA returned NULL in %s\n", target);
        return 1;
    }
    printf("loaded %s into %s (%lu)\n", path, target, pid);
    return 0;
}

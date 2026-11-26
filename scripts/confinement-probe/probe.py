#!/usr/bin/env python3
"""Sonde de confinement (ADR 0034, phase 0) : Landlock et seccomp sont-ils utilisables dans ce conteneur ?

Chaque essai s'exécute dans un processus enfant (la restriction est irréversible) et rend un résultat
JSON. Rien n'est installé ; seuls des dossiers jetables sous /tmp sont créés.
"""

import ctypes
import json
import os
import platform
import socket
import struct
import sys
import tempfile

SYS_LANDLOCK_CREATE_RULESET = 444
SYS_LANDLOCK_ADD_RULE = 445
SYS_LANDLOCK_RESTRICT_SELF = 446
LANDLOCK_CREATE_RULESET_VERSION = 1
LANDLOCK_RULE_PATH_BENEATH = 1
LANDLOCK_RULE_NET_PORT = 2

FS_EXECUTE = 1 << 0
FS_WRITE_FILE = 1 << 1
FS_READ_FILE = 1 << 2
FS_READ_DIR = 1 << 3
FS_MAKE_REG = 1 << 8
FS_READ = FS_READ_FILE | FS_READ_DIR
FS_WRITE = FS_WRITE_FILE | FS_MAKE_REG
NET_CONNECT_TCP = 1 << 1
ABI_FIRST_WITH_NET = 4

PR_SET_NO_NEW_PRIVS = 38
PR_SET_SECCOMP = 22
SECCOMP_MODE_FILTER = 2
SECCOMP_RET_ALLOW = 0x7FFF0000
SECCOMP_RET_ERRNO = 0x00050000
BPF_LD_W_ABS = 0x20
BPF_JEQ_K = 0x15
BPF_RET_K = 0x06
SECCOMP_DATA_NR_OFFSET = 0
SECCOMP_DATA_ARCH_OFFSET = 4
EPERM = 1

MACHINES = {
    # machine : (AUDIT_ARCH, numéro de getppid, numéro de ptrace)
    "x86_64": (0xC000003E, 110, 101),
    "aarch64": (0xC00000B7, 173, 117),
}

libc = ctypes.CDLL(None, use_errno=True)


def landlock_abi() -> int:
    version = libc.syscall(SYS_LANDLOCK_CREATE_RULESET, None, 0, LANDLOCK_CREATE_RULESET_VERSION)
    return version if version >= 0 else -ctypes.get_errno()


def fs_mask(abi: int) -> int:
    mask = (1 << 13) - 1
    if abi >= 2:
        mask |= 1 << 13
    if abi >= 3:
        mask |= 1 << 14
    if abi >= 5:
        mask |= 1 << 15
    return mask


def restrict_filesystem_and_network(abi: int, readable: str, writable: str) -> None:
    handled_net = NET_CONNECT_TCP if abi >= ABI_FIRST_WITH_NET else 0
    attr = struct.pack("QQ", fs_mask(abi), handled_net) if handled_net else struct.pack("Q", fs_mask(abi))
    buffer = ctypes.create_string_buffer(attr, len(attr))
    ruleset = libc.syscall(SYS_LANDLOCK_CREATE_RULESET, buffer, len(attr), 0)
    if ruleset < 0:
        raise OSError(ctypes.get_errno(), "landlock_create_ruleset")
    for path, access in ((readable, FS_READ), (writable, FS_READ | FS_WRITE)):
        fd = os.open(path, os.O_PATH | os.O_CLOEXEC)
        rule = struct.pack("=Qi", access, fd)
        rule_buffer = ctypes.create_string_buffer(rule, len(rule))
        if libc.syscall(SYS_LANDLOCK_ADD_RULE, ruleset, LANDLOCK_RULE_PATH_BENEATH, rule_buffer, 0) < 0:
            raise OSError(ctypes.get_errno(), "landlock_add_rule")
        os.close(fd)
    if libc.prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0:
        raise OSError(ctypes.get_errno(), "no_new_privs")
    if libc.syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset, 0) < 0:
        raise OSError(ctypes.get_errno(), "landlock_restrict_self")


def outcome(action) -> str:
    try:
        action()
        return "autorisé"
    except PermissionError:
        return "refusé"
    except OSError as err:
        return f"erreur {err.errno}"


def read_file(path: str):
    return lambda: open(path, "rb").close()


def write_file(path: str):
    return lambda: open(path, "wb").close()


def connect_to(port: int):
    def action():
        with socket.create_connection(("127.0.0.1", port), timeout=2):
            pass
    return action


def landlock_trial() -> dict:
    abi = landlock_abi()
    if abi < 1:
        return {"abi": abi, "disponible": False}
    root = tempfile.mkdtemp(prefix="sonde-")
    readable, writable = os.path.join(root, "lecture"), os.path.join(root, "ecriture")
    os.mkdir(readable)
    os.mkdir(writable)
    open(os.path.join(readable, "fichier"), "w").close()
    server = socket.socket()
    server.bind(("127.0.0.1", 0))
    server.listen(4)
    port = server.getsockname()[1]
    reader, writer = os.pipe()
    pid = os.fork()
    if pid == 0:
        os.close(reader)
        report = {"abi": abi, "disponible": True}
        try:
            report["reseau_avant"] = outcome(connect_to(port))
            restrict_filesystem_and_network(abi, readable, writable)
            report["lecture_dossier_autorise"] = outcome(read_file(os.path.join(readable, "fichier")))
            report["ecriture_dossier_autorise"] = outcome(write_file(os.path.join(writable, "sortie")))
            report["lecture_hors_dossier"] = outcome(read_file("/etc/hostname"))
            report["ecriture_dans_dossier_lecture"] = outcome(write_file(os.path.join(readable, "intrus")))
            report["reseau_apres"] = outcome(connect_to(port)) if abi >= ABI_FIRST_WITH_NET else "non géré (ABI < 4)"
        except OSError as err:
            report["erreur"] = f"{err.errno} {err.strerror}"
        os.write(writer, json.dumps(report).encode())
        os._exit(0)
    os.close(writer)
    data = b""
    while chunk := os.read(reader, 4096):
        data += chunk
    os.waitpid(pid, 0)
    server.close()
    return json.loads(data) if data else {"abi": abi, "erreur": "l'enfant n'a rien rendu"}


def seccomp_trial() -> dict:
    machine = platform.machine()
    if machine not in MACHINES:
        return {"erreur": f"architecture non prévue : {machine}"}
    audit_arch, getppid_nr, _ = MACHINES[machine]
    reader, writer = os.pipe()
    pid = os.fork()
    if pid == 0:
        os.close(reader)
        report = {}
        try:
            report["getppid_avant"] = libc.syscall(getppid_nr)
            instructions = [
                (BPF_LD_W_ABS, 0, 0, SECCOMP_DATA_ARCH_OFFSET),
                (BPF_JEQ_K, 1, 0, audit_arch),
                (BPF_RET_K, 0, 0, SECCOMP_RET_ERRNO | EPERM),
                (BPF_LD_W_ABS, 0, 0, SECCOMP_DATA_NR_OFFSET),
                (BPF_JEQ_K, 0, 1, getppid_nr),
                (BPF_RET_K, 0, 0, SECCOMP_RET_ERRNO | EPERM),
                (BPF_RET_K, 0, 0, SECCOMP_RET_ALLOW),
            ]
            raw = b"".join(struct.pack("=HBBI", *instruction) for instruction in instructions)
            filters = ctypes.create_string_buffer(raw, len(raw))
            program = struct.pack("=HxxxxxxQ", len(instructions), ctypes.addressof(filters))
            program_buffer = ctypes.create_string_buffer(program, len(program))
            if libc.prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0:
                raise OSError(ctypes.get_errno(), "no_new_privs")
            if libc.prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, program_buffer, 0, 0) != 0:
                raise OSError(ctypes.get_errno(), "seccomp_filter")
            result = libc.syscall(getppid_nr)
            report["getppid_apres"] = result
            report["getppid_errno"] = ctypes.get_errno() if result < 0 else 0
            report["filtre_applique"] = result == -1
        except OSError as err:
            report["erreur"] = f"{err.errno} {err.strerror}"
        os.write(writer, json.dumps(report).encode())
        os._exit(0)
    os.close(writer)
    data = b""
    while chunk := os.read(reader, 4096):
        data += chunk
    os.waitpid(pid, 0)
    return json.loads(data) if data else {"erreur": "l'enfant n'a rien rendu"}


def main() -> int:
    report = {
        "noyau": platform.release(),
        "architecture": platform.machine(),
        "uid": os.getuid(),
        "landlock": landlock_trial(),
        "seccomp": seccomp_trial(),
    }
    print(json.dumps(report, indent=2, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Manage private macOS QEMU disk templates and copy-on-write test instances."""
import argparse
import contextlib
import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import time
import uuid
from decimal import Decimal

VM_SLICE = "c3r-vms.slice"
VM_MEMORY_MAX = 10_000_000_000
VM_OVERHEAD = 1024 * 1024 * 1024
DEFAULT_RAM_MIB = 6144
AUDIO_PROFILES = ("none", "usb-null")


class Error(Exception):
    pass


def stamp():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")


def named(value):
    if not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,47}", value):
        raise Error("Names must contain 1 to 48 lowercase letters, numbers, underscores or hyphens")
    return value


def run(args, **kwargs):
    return subprocess.run([str(x) for x in args], check=True, **kwargs)


def read(path):
    return json.loads(Path(path).read_text())


def sync_dir(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def write(path, value):
    path = Path(path)
    tmp = path.with_name(path.name + ".new-" + uuid.uuid4().hex)
    try:
        with tmp.open("x", encoding="utf-8") as handle:
            json.dump(value, handle, indent=2)
            handle.write("\n")
            handle.flush()
            os.fsync(handle.fileno())
        tmp.chmod(0o600)
        os.replace(tmp, path)
        sync_dir(path.parent)
    finally:
        tmp.unlink(missing_ok=True)


def copy_private(source, target):
    shutil.copyfile(source, target)
    Path(target).chmod(0o600)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def executable(name):
    value = shutil.which(name)
    if not value:
        raise Error(f"{name} is required; use nix shell nixpkgs#qemu nixpkgs#python3")
    return str(Path(value).resolve())


def qemu_img(*args, capture=False):
    result = run([executable("qemu-img"), *args], capture_output=capture, text=True)
    return result.stdout if capture else None


def image_info(path):
    return json.loads(qemu_img("info", "--output=json", path, capture=True))


def assert_not_open(path):
    """Refuse even legacy QEMU processes which predate this manager."""
    wanted = Path(path).stat()
    for proc in Path("/proc").iterdir():
        if not proc.name.isdigit():
            continue
        try:
            if proc.stat().st_uid != os.getuid():
                continue
            for fd in (proc / "fd").iterdir():
                try:
                    current = fd.stat()
                    if (current.st_dev, current.st_ino) == (wanted.st_dev, wanted.st_ino):
                        raise Error(f"{path} is open by PID {proc.name}; shut the guest down first")
                except (FileNotFoundError, PermissionError, ProcessLookupError):
                    pass
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            continue


def listening_available(port):
    try:
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", port))
        return True
    except OSError:
        return False


def audio_profile(manifest):
    profile = manifest.get("audio_profile", "none")
    if profile not in AUDIO_PROFILES:
        raise Error(f"Unsupported VM audio profile: {profile}")
    return profile


def configured_ram(command):
    """Read the effective final -m argument, including externally started QEMU."""
    value = "128"
    for index, argument in enumerate(command):
        if argument in ("-m", "--memory") and index + 1 < len(command):
            value = command[index + 1]
        elif argument.startswith("-m="):
            value = argument[3:]
    value = value.split(",", 1)[0].removeprefix("size=")
    match = re.fullmatch(r"([0-9]+(?:\.[0-9]+)?)([bBkKmMgGtT]?)", value)
    if not match:
        raise Error(f"Cannot account QEMU RAM argument {value!r}; refusing another VM")
    unit = match[2].upper()
    multiplier = {"": 1024 ** 2, "B": 1, "K": 1024, "M": 1024 ** 2, "G": 1024 ** 3, "T": 1024 ** 4}[unit]
    return int(Decimal(match[1]) * multiplier)


def clean_file_cache(stats):
    return max(0, stats.get("file", 0) - stats.get("shmem", 0) - stats.get("file_dirty", 0) - stats.get("file_writeback", 0))


def memory_admission(existing, requested, observed_usage=0, idle_clean_cache=0):
    reserved = sum(ram + VM_OVERHEAD for ram in existing)
    # An empty slice can retain clean disk cache after its VM exits. The kernel
    # can reclaim those pages under the unchanged hard limit. Never discount
    # anonymous/shared memory or cache while any VM remains active.
    effective_usage = max(0, observed_usage - (idle_clean_cache if not existing else 0))
    total = max(reserved, effective_usage) + requested + VM_OVERHEAD
    if total > VM_MEMORY_MAX:
        raise Error(f"Combined VM RAM plus overhead would reserve {total} bytes, above the shared {VM_MEMORY_MAX}-byte limit; stop an owned VM or lower its configured RAM")
    return total


def planned_clean_reclaim(existing, requested, observed_usage, clean_cache):
    memory_admission(existing, requested)  # Never reclaim to hide configured overcommit.
    needed = observed_usage + requested + VM_OVERHEAD - VM_MEMORY_MAX
    if needed <= 0 or clean_cache < needed:
        return 0
    return min(clean_cache, needed + 256 * 1024**2, 4 * 1024**3)


class Store:
    def __init__(self, root):
        self.root = Path(root).expanduser().absolute()
        if self.root.is_symlink():
            raise Error("The VM store root must not be a symlink")
        self.root.mkdir(parents=True, exist_ok=True, mode=0o700)
        self.root.chmod(0o700)
        self.templates = self.root / "templates"
        self.instances = self.root / "instances"
        self.templates.mkdir(exist_ok=True)
        self.instances.mkdir(exist_ok=True)

    @contextlib.contextmanager
    def lock(self):
        with (self.root / ".lock").open("a") as handle:
            fcntl.flock(handle, fcntl.LOCK_EX)
            yield

    def item(self, kind, name):
        path = self.root / kind / named(name)
        if path.is_symlink() or not path.is_dir():
            raise Error(f"Unknown {kind[:-1]}: {name}")
        return path

    def template(self, name):
        path = self.item("templates", name)
        manifest = read(path / "template.json")
        if manifest.get("schema") != 1 or manifest.get("name") != name:
            raise Error(f"Invalid template manifest: {path}")
        return path, manifest

    def instance(self, name):
        path = self.item("instances", name)
        if (path / "snapshot-pending.json").exists() and (path / "reset-pending.json").exists():
            raise Error(f"Conflicting snapshot and reset journals: {path}")
        if (path / "snapshot-pending.json").exists():
            self.recover_snapshot(path)
        if (path / "reset-pending.json").exists():
            self.recover_reset(path)
        manifest = read(path / "instance.json")
        if manifest.get("schema") != 1 or manifest.get("name") != name:
            raise Error(f"Invalid instance manifest: {path}")
        return path, manifest

    def active(self, path, manifest):
        pidfile = path / "qemu.pid"
        if not pidfile.exists():
            return None
        try:
            pid = int(pidfile.read_text().strip())
            command = (Path("/proc") / str(pid) / "cmdline").read_bytes().split(b"\0")
        except (ValueError, FileNotFoundError, ProcessLookupError):
            return None
        disk = str(path / "disk.qcow2").encode()
        if not any(disk in arg for arg in command) or not any(manifest["uuid"].encode() in arg for arg in command):
            raise Error(f"Stale/reused PID {pid}; refusing to control an unrelated process")
        return pid

    def stopped(self, path, manifest, clean=False):
        if self.active(path, manifest):
            raise Error(f"{manifest['name']} is running; stop it before changing its disk")
        assert_not_open(path / "disk.qcow2")
        if clean and not (path / "clean-shutdown.json").is_file():
            raise Error("No clean shutdown recorded; boot and use 'stop' before snapshotting")
        # This also acquires QEMU image locks and catches damaged qcow2 metadata.
        qemu_img("check", path / "disk.qcow2")

    def overlay(self, target, base):
        if Path(target).exists():
            raise Error(f"Refusing to overwrite disk {target}")
        qemu_img("create", "-f", "qcow2", "-F", "qcow2", "-b", str(Path(base).resolve()), target)
        Path(target).chmod(0o600)

    def new_ports(self, ssh=None, vnc=None):
        used = set()
        for file in self.instances.glob("*/instance.json"):
            item = read(file)
            used.update([item["ssh_port"], item["vnc_port"]])
        if ssh is None:
            ssh = next((p for p in range(22400, 23000) if p not in used and listening_available(p)), None)
        if vnc is None:
            vnc = next((p for p in range(5920, 6500) if p not in used and listening_available(p)), None)
        if ssh is None or vnc is None or ssh == vnc or vnc < 5900:
            raise Error("No unused local ports available")
        for port in (ssh, vnc):
            if not 1024 <= port <= 65535 or port in used or not listening_available(port):
                raise Error(f"Local port {port} is in use or reserved by another instance")
        return ssh, vnc

    def clone(self, name, template, ssh=None, vnc=None):
        target = self.instances / named(name)
        if target.exists():
            raise Error(f"Instance already exists: {name}")
        source, base = self.template(template)
        ssh, vnc = self.new_ports(ssh, vnc)
        used_macs = {read(file)["mac"] for file in self.instances.glob("*/instance.json")}
        while True:
            ident = uuid.uuid4()
            mac = "52:54:00:" + ":".join(f"{b:02x}" for b in ident.bytes[-3:])
            if mac not in used_macs:
                break
        manifest = {"schema": 1, "name": name, "template": template, "created": stamp(),
                    "uuid": str(ident), "mac": mac,
                    "ssh_port": ssh, "vnc_port": vnc, "user": base["user"],
                    "ram_mib": DEFAULT_RAM_MIB, "vcpus": 8, "cores": 4, "threads": 2,
                    "audio_profile": audio_profile(base)}
        target.mkdir(mode=0o700)
        try:
            self.overlay(target / "disk.qcow2", source / "disk.qcow2")
            copy_private(source / "OVMF_VARS.fd", target / "OVMF_VARS.fd")
            write(target / "instance.json", manifest)
            write(target / "clean-shutdown.json", {"at": stamp(), "reason": "cloned clean template", "template": template})
            (target / "evidence").mkdir()
        except BaseException:
            shutil.rmtree(target)
            raise
        print(json.dumps(manifest, indent=2))

    def import_legacy(self, source, name, clean):
        if not clean:
            raise Error("Import requires --clean-shutdown after the guest has shut down normally")
        source = Path(source).resolve()
        target = self.templates / named(name)
        if target.exists():
            raise Error(f"Template already exists: {name}")
        disk = source / "mac_hdd_ng.img"
        nvram = source / "OVMF_VARS-1920x1080.fd"
        assert_not_open(disk)
        assert_not_open(nvram)
        qemu_img("check", disk)
        if image_info(disk)["format"] != "qcow2":
            raise Error("Legacy macOS disk is not qcow2")
        if disk.stat().st_dev != self.root.stat().st_dev:
            raise Error("Import requires the same filesystem so the large disk can be moved without copying")
        credentials = read(source / "credentials.json")
        user = credentials.get("user") or credentials.get("username") or "c3r"
        if not re.fullmatch(r"[a-zA-Z_][a-zA-Z0-9_-]*", user):
            raise Error("Invalid guest username")
        support = self.root / "support"
        if support.exists():
            raise Error("Store support already exists; use snapshot for further templates")
        support.mkdir(mode=0o700)
        target.mkdir(mode=0o700)
        moved = False
        try:
            for src, dest in [(source / "OVMF_CODE_4M.fd", "OVMF_CODE.fd"),
                              (source / "id_ed25519", "id_ed25519"),
                              (source / "id_ed25519.pub", "id_ed25519.pub"),
                              (source / "credentials.json", "credentials.json")]:
                copy_private(src, support / dest)
            # Flatten the small boot image so no disposable OSX-KVM backing path survives.
            qemu_img("convert", "-O", "qcow2", source / "helpers/OpenCore-debug.qcow2", support / "OpenCore.qcow2")
            if image_info(support / "OpenCore.qcow2").get("backing-filename"):
                raise Error("Imported OpenCore must have no external backing image")
            for file in [support / "OVMF_CODE.fd", support / "OpenCore.qcow2"]:
                file.chmod(0o444)
            copy_private(nvram, target / "OVMF_VARS.fd")
            os.rename(disk, target / "disk.qcow2")
            moved = True
            (target / "disk.qcow2").chmod(0o444)
            (target / "OVMF_VARS.fd").chmod(0o444)
            write(target / "template.json", {"schema": 1, "name": name, "created": stamp(), "user": user,
                                            "source": str(source), "clean_shutdown": True,
                                            "disk": image_info(target / "disk.qcow2"),
                                            "nvram_sha256": digest(target / "OVMF_VARS.fd"),
                                            "opencore": "OpenCore.qcow2", "opencore_sha256": digest(support / "OpenCore.qcow2")})
            write(self.root / "store.json", {"schema": 1, "created": stamp(), "guest": "macOS Sonoma x86_64",
                                             "qemu": executable("qemu-system-x86_64"),
                                             "qemu_version": run([executable("qemu-system-x86_64"), "--version"], capture_output=True, text=True).stdout.splitlines()[0],
                                             "firmware_sha256": digest(support / "OVMF_CODE.fd"),
                                             "opencore_sha256": digest(support / "OpenCore.qcow2"),
                                             "source": str(source)})
        except BaseException:
            if moved:
                (target / "disk.qcow2").chmod(0o600)
                os.rename(target / "disk.qcow2", disk)
            shutil.rmtree(target)
            shutil.rmtree(support)
            raise
        print(f"Imported clean template {name}: {target}")

    def qemu_command(self, path, item):
        store = read(self.root / "store.json")
        support = self.root / "support"
        _, template = self.template(item["template"])
        boot, _ = self.boot_image(template.get("opencore", "OpenCore.qcow2"))
        expected = template.get("opencore_sha256", store["opencore_sha256"])
        if digest(boot) != expected:
            raise Error(f"Template boot image changed: {boot}")
        # QEMU's key/value grammar treats commas as option delimiters.
        if "," in str(self.root):
            raise Error("QEMU VM paths must not contain commas")
        cpus = f"{item['vcpus']},cores={item['cores']},threads={item['threads']},sockets=1"
        # Optional per-instance settings, absent from older manifests (see `configure`).
        # shared_ram: guest RAM is a memfd with share=on, so a host process of the same
        # user can map guest pages through /proc/<qemu-pid>/fd (no file, no root).
        machine = "q35"
        ram = []
        if item.get("shared_ram"):
            machine = "q35,memory-backend=macvm-ram"
            ram = ["-object", f"memory-backend-memfd,id=macvm-ram,size={item['ram_mib']}M,share=on"]
        audio = []
        if audio_profile(item) == "usb-null":
            audio = ["-audiodev", "none,id=macvm-audio", "-device",
                     "usb-audio,id=macvm-usb-audio,bus=xhci.0,audiodev=macvm-audio"]
        extra = [str(arg) for arg in item.get("qemu_extra", [])]
        return [executable("qemu-system-x86_64"), "-name", f"c3r-macos-{item['name']}", "-uuid", item["uuid"],
                "-enable-kvm", *ram, "-machine", machine, "-m", str(item["ram_mib"]), "-smp", cpus,
                "-cpu", "Skylake-Client,-hle,-rtm,kvm=on,vendor=GenuineIntel,+invtsc,vmware-cpuid-freq=on,-pcid,+ssse3,+sse4.2,+popcnt,+avx,+aes,+xsave,+xsaveopt,check",
                "-device", "qemu-xhci,id=xhci", "-device", "usb-kbd,bus=xhci.0", "-device", "usb-tablet,bus=xhci.0",
                "-global", "ICH9-LPC.acpi-pci-hotplug-with-bridge-support=off",
                "-device", "isa-applesmc,osk=ourhardworkbythesewordsguardedpleasedontsteal(c)AppleComputerInc",
                "-drive", f"if=pflash,format=raw,readonly=on,file={support / 'OVMF_CODE.fd'}",
                "-drive", f"if=pflash,format=raw,file={path / 'OVMF_VARS.fd'}", "-smbios", "type=2",
                "-device", "ich9-ahci,id=sata",
                "-drive", f"id=OpenCoreBoot,if=none,snapshot=on,format=qcow2,file={boot}",
                "-device", "ide-hd,bus=sata.2,drive=OpenCoreBoot",
                "-drive", f"id=MacHDD,if=none,file={path / 'disk.qcow2'},format=qcow2",
                "-device", "ide-hd,bus=sata.4,drive=MacHDD",
                "-netdev", f"user,id=net0,hostfwd=tcp:127.0.0.1:{item['ssh_port']}-:22",
                "-device", f"virtio-net-pci,netdev=net0,id=net0,mac={item['mac']}",
                "-qmp", f"unix:{path / 'qmp.sock'},server=on,wait=off", "-monitor", "none",
                "-device", "vmware-svga", "-display", "none", "-vnc", f"127.0.0.1:{item['vnc_port'] - 5900}", "-k", "en-us",
                "-serial", f"file:{path / 'evidence/serial.log'}", "-pidfile", str(path / "qemu.pid"), *audio, *extra]

    def configure(self, name, ram_mib=None, shared_ram=None, qemu_extra=None, audio=None):
        """Change a stopped instance's optional QEMU settings. Older manifests lack them all."""
        path, item = self.instance(name)
        if self.active(path, item):
            raise Error(f"{name} is running; stop it before changing its QEMU settings")
        if audio is not None:
            item["audio_profile"] = audio_profile({"audio_profile": audio})
        if ram_mib is not None:
            if not 2048 <= ram_mib <= 65536:
                raise Error("RAM must be between 2048 and 65536 MiB")
            item["ram_mib"] = ram_mib
        if shared_ram is not None:
            item["shared_ram"] = shared_ram
        if qemu_extra is not None:
            value = json.loads(qemu_extra)
            if not isinstance(value, list) or not all(isinstance(x, str) for x in value):
                raise Error("--qemu-extra must be a JSON list of strings")
            if any(x in ("-daemonize", "-pidfile", "-uuid", "-name") for x in value):
                raise Error("--qemu-extra must not override the manager's own process options")
            item["qemu_extra"] = value
        write(path / "instance.json", item)
        print(json.dumps(item, indent=2))

    def slice_info(self):
        def read_config():
            result = run(["systemctl", "--user", "show", VM_SLICE, "--property=LoadState,MemoryCurrent,MemoryMax,MemorySwapMax,ControlGroup,ActiveState"], capture_output=True, text=True)
            return dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
        def validate_config(info):
            if info.get("LoadState") != "loaded" or info.get("MemoryMax") != str(VM_MEMORY_MAX) or info.get("MemorySwapMax") != "0":
                raise Error(f"{VM_SLICE} must be configured with MemoryMax={VM_MEMORY_MAX} and MemorySwapMax=0 before starting any VM")
        info = read_config()
        # A static slice is normally inactive after a fresh user-manager boot.
        # Activate only an already-loaded unit whose hard limits are exact.
        validate_config(info)
        if info.get("ActiveState") != "active":
            run(["systemctl", "--user", "start", VM_SLICE], capture_output=True, text=True)
            info = read_config()
        validate_config(info)
        if info.get("ActiveState") != "active" or not info.get("ControlGroup"):
            raise Error(f"{VM_SLICE} did not become active; no VM was started")
        return info

    def admit(self, command):
        info = self.slice_info()
        existing = []
        for proc in Path("/proc").iterdir():
            if not proc.name.isdigit():
                continue
            try:
                if proc.stat().st_uid != os.getuid():
                    continue
                args = (proc / "cmdline").read_bytes().decode(errors="replace").split("\0")
                if not args or not Path(args[0]).name.startswith("qemu-system-"):
                    continue
                groups = (proc / "cgroup").read_text().splitlines()
                inside = any(line.split(":", 2)[-1] == info["ControlGroup"] or line.split(":", 2)[-1].startswith(info["ControlGroup"] + "/") for line in groups)
                name = args[args.index("-name") + 1] if "-name" in args else ""
                registered = read(self.root / "store.json").get("scoped_qemu_names", [])
                ours = name.startswith("c3r-") or name in registered or any(str(self.instances) + "/" in argument for argument in args)
                if not inside and not ours:
                    continue  # Unrelated fleet VMs belong to another host task.
                if not inside:
                    raise Error(f"Renderer QEMU PID {proc.name} is outside the shared VM slice; its owner must move or stop it before another renderer VM starts")
                existing.append(configured_ram(args))
            except (FileNotFoundError, PermissionError, ProcessLookupError):
                continue
        requested = configured_ram(command)
        memory_admission(existing, requested)  # Check configured reservations first.
        group = Path("/sys/fs/cgroup") / info["ControlGroup"].lstrip("/")
        try:
            stats = {key: int(value) for key, value in (line.split() for line in (group / "memory.stat").read_text().splitlines())}
            cache = clean_file_cache(stats)
        except (OSError, ValueError):
            cache = 0
        usage = int(info["MemoryCurrent"])
        try:
            return memory_admission(existing, requested, usage, cache if not existing else 0)
        except Error:
            if not existing:
                raise
            amount = planned_clean_reclaim(existing, requested, usage, cache)
            if not amount:
                raise
            reclaim_error = None
            try:
                # File-LRU reclaim only. No anonymous reclaim and no swap allowance.
                (group / "memory.reclaim").write_text(f"{amount} swappiness=0")
            except OSError as error:
                reclaim_error = str(error)  # EAGAIN may still have reclaimed useful cache.
            after = int((group / "memory.current").read_text().strip())
            write(self.root / "last-admission-reclaim.json", {"at": stamp(), "requested_clean_file_bytes": amount,
                  "swappiness": 0, "before_bytes": usage, "after_bytes": after, "error": reclaim_error,
                  "hard_memory_max_bytes": VM_MEMORY_MAX, "memory_swap_max": 0})
            return memory_admission(existing, requested, after)

    def in_own_service(self, unit):
        info = self.slice_info()
        expected = info["ControlGroup"] + "/" + unit
        return any(line.split(":", 2)[-1] == expected for line in Path("/proc/self/cgroup").read_text().splitlines())

    def start(self, name, foreground=False):
        path, item = self.instance(name)
        if self.active(path, item):
            raise Error(f"{name} is already running")
        self.stopped(path, item)
        for port in (item["ssh_port"], item["vnc_port"]):
            if not listening_available(port):
                raise Error(f"Local port {port} is occupied")
        self.verify_one(path, item, check=False)
        command = self.qemu_command(path, item)
        # QEMU is always a foreground process owned by one capped systemd unit.
        # Extensions may add devices, but may not detach or replace manager identity.
        forbidden = {"daemonize", "pidfile", "uuid", "name", "readconfig"}
        for argument in item.get("qemu_extra", []):
            if argument.startswith("-") and argument.lstrip("-").split("=", 1)[0] in forbidden:
                raise Error(f"Extra QEMU argument cannot override managed process ownership: {argument}")
        reserved = self.admit(command)
        for file in (path / "qemu.pid", path / "qmp.sock", path / "clean-shutdown.json"):
            file.unlink(missing_ok=True)
        unit = f"c3r-macos-{item['name']}.service"
        write(path / "last-start.json", {"at": stamp(), "command": command, "systemd_unit": unit,
                                        "slice": VM_SLICE, "shared_memory_max_bytes": VM_MEMORY_MAX,
                                        "reserved_aggregate_bytes": reserved})
        if self.in_own_service(unit):
            kind = run(["systemctl", "--user", "show", unit, "--property=Type", "--value"], capture_output=True, text=True).stdout.strip()
            if kind not in ("exec", "simple"):
                raise Error(f"Direct VM service must use Type=exec or simple, got {kind}; no recursive service or detached QEMU started")
            return os.execv(command[0], command)
        run(["systemd-run", "--user", f"--unit={unit}", f"--slice={VM_SLICE}", "--collect", "--service-type=exec",
             "--property=MemoryAccounting=yes", "--property=MemorySwapMax=0", "--property=KillMode=control-group",
             f"--property=StandardOutput=append:{path / 'evidence/qemu.log'}",
             f"--property=StandardError=append:{path / 'evidence/qemu.log'}", "--", *command], capture_output=True, text=True)
        deadline = time.monotonic() + 15
        while not self.active(path, item):
            if time.monotonic() >= deadline:
                raise Error(f"Managed QEMU did not start; inspect {unit} and {path / 'evidence/qemu.log'}")
            time.sleep(0.1)
        print(f"Started {name} in {VM_SLICE}: SSH 127.0.0.1:{item['ssh_port']}, VNC 127.0.0.1:{item['vnc_port']}")

    def ssh_command(self, path, item):
        return ["ssh", "-i", str(self.root / "support/id_ed25519"), "-p", str(item["ssh_port"]),
                "-o", f"UserKnownHostsFile={path / 'known_hosts'}", "-o", "StrictHostKeyChecking=accept-new",
                "-o", "IdentitiesOnly=yes", "-o", "IdentityAgent=none", "-o", "BatchMode=yes",
                "-o", "ConnectTimeout=15", f"{item['user']}@127.0.0.1"]

    def stop(self, name, timeout):
        path, item = self.instance(name)
        if not self.active(path, item):
            if (path / "clean-shutdown.json").exists():
                print(f"{name} is already cleanly stopped")
                return
            raise Error(f"{name} is stopped without a clean shutdown record; boot before snapshotting")
        result = subprocess.run([*self.ssh_command(path, item), "sudo -n /sbin/shutdown -h now"], capture_output=True)
        # SSH commonly returns 255 as the guest closes the connection during shutdown.
        if result.returncode not in (0, 255):
            raise Error(f"Guest shutdown failed: {result.stderr.decode(errors='replace').strip()}")
        if result.returncode == 255 and b"shutdown" not in result.stdout + result.stderr and b"closed" not in result.stderr.lower():
            raise Error(f"SSH shutdown did not connect: {result.stderr.decode(errors='replace').strip()}")
        end = time.monotonic() + timeout
        while self.active(path, item):
            if time.monotonic() >= end:
                raise Error("Guest did not shut down within the timeout; no snapshot or forced kill was performed")
            time.sleep(1)
        assert_not_open(path / "disk.qcow2")
        qemu_img("check", path / "disk.qcow2")
        write(path / "clean-shutdown.json", {"at": stamp(), "reason": "guest SSH shutdown"})
        print(f"Cleanly stopped {name}")

    def boot_image(self, name):
        if Path(name).name != name or not name.endswith(".qcow2"):
            raise Error("Boot image must name a qcow2 file in the store's support directory")
        path = self.root / "support" / name
        if path.is_symlink() or not path.is_file():
            raise Error(f"Unknown boot image: {name}")
        info = image_info(path)
        if info["format"] != "qcow2" or info.get("backing-filename"):
            raise Error("Boot image must be self-contained qcow2")
        return path, digest(path)

    def recover_snapshot(self, path):
        journal = read(path / "snapshot-pending.json")
        item = journal["instance"]
        template = journal["template"]
        if journal.get("schema") != 1 or item["name"] != path.name or template["source_instance"] != path.name:
            raise Error(f"Invalid snapshot journal: {path}")
        target = self.templates / named(template["name"])
        if target.is_symlink() or not target.is_dir():
            raise Error(f"Snapshot target is missing or unsafe: {target}")
        if self.active(path, item):
            raise Error("Cannot recover a snapshot while the instance is running")
        disk, frozen = path / "disk.qcow2", target / "disk.qcow2"
        for file in (disk, frozen, path / "OVMF_VARS.fd", target / "OVMF_VARS.fd"):
            if file.exists():
                assert_not_open(file)
        if digest(path / "OVMF_VARS.fd") != template["nvram_sha256"] or digest(target / "OVMF_VARS.fd") != template["nvram_sha256"]:
            raise Error("Snapshot firmware changed while the transaction was pending")
        identity = journal["source_disk_identity"]
        original = frozen if frozen.exists() else disk
        stat = original.stat()
        if [stat.st_dev, stat.st_ino] != identity:
            raise Error("Snapshot recovery cannot identify the original disk; no disk was overwritten")
        if not frozen.exists():
            os.rename(disk, frozen)
            sync_dir(path)
            sync_dir(target)
        frozen.chmod(0o444)
        (target / "OVMF_VARS.fd").chmod(0o444)
        if disk.exists():
            try:
                info = image_info(disk)
                correct = Path(info.get("full-backing-filename", info.get("backing-filename", ""))).resolve() == frozen.resolve()
            except (subprocess.CalledProcessError, OSError, ValueError):
                correct = False
            if not correct:
                # An interrupted qemu-img child can leave a partial new overlay.
                # Preserve its bytes rather than mistaking it for the source disk.
                recovery = path / "snapshot-recovery"
                recovery.mkdir(exist_ok=True, mode=0o700)
                os.rename(disk, recovery / (stamp() + ".qcow2"))
                sync_dir(recovery)
                sync_dir(path)
        if not disk.exists():
            self.overlay(disk, frozen)
            sync_dir(path)
        template["disk"] = image_info(frozen)
        write(target / "template.json", template)
        # Recovery always finishes forward. It never removes a committed template
        # after publishing the corresponding instance manifest.
        write(path / "instance.json", item)
        write(path / "clean-shutdown.json", {"at": stamp(), "reason": "fresh snapshot overlay", "template": template["name"]})
        (path / "snapshot-pending.json").unlink()
        sync_dir(path)
        print(f"Saved template {template['name']}; {path.name} now uses a fresh writable overlay")

    def snapshot(self, name, template, boot_name=None):
        path, item = self.instance(name)
        target = self.templates / named(template)
        if target.exists():
            raise Error(f"Template already exists: {template}")
        self.stopped(path, item, clean=True)
        _, parent = self.template(item["template"])
        boot, boot_hash = self.boot_image(boot_name or parent.get("opencore", "OpenCore.qcow2"))
        disk = path / "disk.qcow2"
        source_stat = disk.stat()
        manifest = {"schema": 1, "name": template, "created": stamp(), "user": item["user"],
                    "source_instance": name, "parent_template": item["template"], "clean_shutdown": True,
                    "audio_profile": audio_profile(item),
                    "nvram_sha256": digest(path / "OVMF_VARS.fd"),
                    "opencore": boot.name, "opencore_sha256": boot_hash}
        item = {**item, "template": template}
        target.mkdir(mode=0o700)
        try:
            copy_private(path / "OVMF_VARS.fd", target / "OVMF_VARS.fd")
            with (target / "OVMF_VARS.fd").open("rb") as handle:
                os.fsync(handle.fileno())
            sync_dir(target)
            sync_dir(self.templates)
            write(path / "snapshot-pending.json", {"schema": 1, "instance": item, "template": manifest,
                                                    "source_disk_identity": [source_stat.st_dev, source_stat.st_ino]})
        except BaseException:
            # Before journal commit, neither the original disk nor its manifest
            # has changed. After commit, preserve everything for recovery.
            if not (path / "snapshot-pending.json").exists():
                shutil.rmtree(target)
            raise
        self.recover_snapshot(path)

    def recover_reset(self, path):
        journal = read(path / "reset-pending.json")
        item = journal["instance"]
        if journal.get("schema") != 1 or item["name"] != path.name:
            raise Error(f"Invalid reset journal: {path}")
        if self.active(path, item):
            raise Error("Cannot recover a reset while the instance is running")
        history = path / journal["history"]
        if not history.resolve().is_relative_to((path / "history").resolve()):
            raise Error("Reset history path escapes the instance")
        pairs = [("disk.qcow2", "disk.next.qcow2"), ("OVMF_VARS.fd", "OVMF_VARS.next.fd")]
        for current, pending in pairs:
            for file in (path / current, path / pending, history / current):
                if file.exists():
                    assert_not_open(file)
        # Each rename is restartable. The journal prevents booting a mixed pair.
        for current, pending in pairs:
            live, next_file, old = path / current, path / pending, history / current
            if not old.exists():
                if not live.exists() or not next_file.exists():
                    raise Error(f"Reset recovery is missing an original or prepared file: {current}")
                os.rename(live, old)
                sync_dir(history)
                sync_dir(path)
            if next_file.exists():
                if live.exists():
                    raise Error(f"Reset recovery found conflicting current and prepared files: {current}")
                os.rename(next_file, live)
                sync_dir(path)
            elif not live.exists():
                raise Error(f"Reset recovery is missing the restored file: {current}")
        write(path / "instance.json", item)
        write(path / "clean-shutdown.json", {"at": stamp(), "reason": "reset to clean template", "template": item["template"]})
        (path / "reset-pending.json").unlink()
        sync_dir(path)
        print(f"Reset {path.name} to {item['template']}; previous state preserved in {history}")

    def reset(self, name, template):
        path, item = self.instance(name)
        source, base = self.template(template or item["template"])
        restored_audio = audio_profile(base)
        template = source.name
        self.stopped(path, item)
        self.verify_one(source, None, check=False)
        history = path / "history" / stamp()
        history.mkdir(parents=True, mode=0o700)
        pending_disk = path / "disk.next.qcow2"
        pending_vars = path / "OVMF_VARS.next.fd"
        if pending_disk.exists() or pending_vars.exists():
            raise Error("Unfinished reset preparation exists without a journal; inspect the files before retrying")
        try:
            self.overlay(pending_disk, source / "disk.qcow2")
            copy_private(source / "OVMF_VARS.fd", pending_vars)
            copy_private(path / "instance.json", history / "instance.json")
            if (path / "clean-shutdown.json").exists():
                copy_private(path / "clean-shutdown.json", history / "clean-shutdown.json")
            item["template"] = template
            item["audio_profile"] = restored_audio
            write(path / "reset-pending.json", {"schema": 1, "instance": item, "history": str(history.relative_to(path))})
        except BaseException:
            # If the journal was committed before an fsync/interruption error,
            # retain the prepared files so the next command can finish safely.
            if not (path / "reset-pending.json").exists():
                pending_disk.unlink(missing_ok=True)
                pending_vars.unlink(missing_ok=True)
                shutil.rmtree(history)
            raise
        self.recover_reset(path)

    def verify_one(self, path, item, check=True):
        chain = json.loads(qemu_img("info", "--backing-chain", "--output=json", path / "disk.qcow2", capture=True))
        for layer in chain:
            file = Path(layer["filename"]).resolve()
            if not file.is_relative_to(self.root.resolve()):
                raise Error(f"Backing file escapes this store: {file}")
            if layer["format"] != "qcow2":
                raise Error(f"Unexpected disk format: {file}")
            if check:
                assert_not_open(file)
                qemu_img("check", file)
        if item is None:
            template = read(path / "template.json")
            if digest(path / "OVMF_VARS.fd") != template["nvram_sha256"]:
                raise Error(f"Template NVRAM changed: {path}")
            boot, boot_hash = self.boot_image(template.get("opencore", "OpenCore.qcow2"))
            expected = template.get("opencore_sha256", read(self.root / "store.json")["opencore_sha256"])
            if boot_hash != expected:
                raise Error(f"Template boot image changed: {boot}")
        elif str(Path(chain[1]["filename"]).resolve()) != str((self.templates / item["template"] / "disk.qcow2").resolve()):
            raise Error(f"Instance backing file disagrees with its manifest: {path}")

    def verify(self, name):
        store = read(self.root / "store.json")
        for file, key in [("OVMF_CODE.fd", "firmware_sha256"), ("OpenCore.qcow2", "opencore_sha256")]:
            if digest(self.root / "support" / file) != store[key]:
                raise Error(f"Support image changed: {file}")
        if name:
            path, item = self.instance(name)
            self.stopped(path, item)
            self.verify_one(path, item)
        else:
            for path in sorted(self.templates.iterdir()):
                self.verify_one(path, None)
        print("Disk chains, qcow2 metadata, firmware and NVRAM verified")

    def listing(self):
        result = {"root": str(self.root), "templates": [], "instances": []}
        for path in sorted(self.templates.glob("*/template.json")):
            item = read(path)
            stat = (path.parent / "disk.qcow2").stat()
            result["templates"].append({"name": item["name"], "created": item["created"], "parent": item.get("parent_template"),
                                        "allocated_bytes": stat.st_blocks * 512})
        for path in sorted(self.instances.glob("*/instance.json")):
            item = read(path)
            item["running"] = self.active(path.parent, item) is not None
            item["clean_shutdown"] = (path.parent / "clean-shutdown.json").exists()
            disk = path.parent / "disk.qcow2"
            item["allocated_bytes"] = disk.stat().st_blocks * 512 if disk.exists() else None
            item["pending_transaction"] = next((kind for kind in ("snapshot", "reset") if (path.parent / f"{kind}-pending.json").exists()), None)
            result["instances"].append(item)
        print(json.dumps(result, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=os.environ.get("CIV3R_MACVM_ROOT", "/mnt/datasets/macvm"))
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("import", help="Move a stopped legacy disk into a new store, without copying it")
    p.add_argument("source", type=Path)
    p.add_argument("template", type=named)
    p.add_argument("--clean-shutdown", action="store_true")
    p = sub.add_parser("clone", help="Create a cheap writable instance of a named template")
    p.add_argument("name", type=named)
    p.add_argument("--template", required=True, type=named)
    p.add_argument("--ssh-port", type=int)
    p.add_argument("--vnc-port", type=int)
    for cmd in ("start", "run", "stop", "ssh", "status", "verify", "recover"):
        p = sub.add_parser(cmd)
        if cmd == "status":
            continue
        p.add_argument("name", type=named, nargs="?" if cmd == "verify" else None)
        if cmd == "stop":
            p.add_argument("--timeout", type=int, default=180)
        if cmd == "ssh":
            p.add_argument("args", nargs=argparse.REMAINDER)
    p = sub.add_parser("snapshot", help="Freeze a cleanly stopped instance as a named reusable template")
    p.add_argument("name", type=named)
    p.add_argument("template", type=named)
    p.add_argument("--boot-image", help="Self-contained immutable qcow2 filename in store support")
    p = sub.add_parser("configure", help="Set a stopped instance's RAM, shared (memfd) RAM or extra QEMU arguments")
    p.add_argument("name", type=named)
    p.add_argument("--ram-mib", type=int)
    p.add_argument("--shared-ram", choices=("on", "off"))
    p.add_argument("--audio", choices=AUDIO_PROFILES, help="Guest audio hardware; usb-null uses no host audio")
    p.add_argument("--qemu-extra", help="JSON list of extra QEMU arguments, appended last")
    p = sub.add_parser("reset", help="Restore template state, archiving the previous overlay and NVRAM")
    p.add_argument("name", type=named)
    p.add_argument("--to", type=named)
    args = parser.parse_args()
    store = Store(args.root)
    # SSH and foreground QEMU must not inherit a lock which blocks other commands.
    if args.command == "ssh":
        with store.lock():
            path, item = store.instance(args.name)
            command = store.ssh_command(path, item) + args.args
        os.execvp(command[0], command)
    with store.lock():
        if args.command == "import":
            store.import_legacy(args.source, args.template, args.clean_shutdown)
        elif args.command == "clone":
            store.clone(args.name, args.template, args.ssh_port, args.vnc_port)
        elif args.command in ("start", "run"):
            store.start(args.name)
        elif args.command == "stop":
            store.stop(args.name, args.timeout)
        elif args.command == "snapshot":
            store.snapshot(args.name, args.template, args.boot_image)
        elif args.command == "configure":
            store.configure(args.name, args.ram_mib,
                            None if args.shared_ram is None else args.shared_ram == "on", args.qemu_extra, args.audio)
        elif args.command == "reset":
            store.reset(args.name, args.to)
        elif args.command == "recover":
            store.instance(args.name)
            print(f"{args.name} has no unfinished VM transaction")
        elif args.command == "verify":
            store.verify(args.name)
        elif args.command == "status":
            store.listing()


if __name__ == "__main__":
    try:
        main()
    except (Error, OSError, subprocess.CalledProcessError, ValueError, KeyError) as error:
        print(f"macos-vm: {error}", file=sys.stderr)
        sys.exit(1)

#!/usr/bin/python3

import argparse
import os
import sys
import pathlib
import shutil
import subprocess

from plumbum import colors, local
from plumbum.cmd import xargo, sudo, tunctl, ifconfig, whoami
from plumbum.commands import ProcessExecutionError


def exception_handler(exception_type, exception, traceback):
    print("%s: %s" % (exception_type.__name__, exception))


#
# run.py script settings
#
SCRIPT_PATH = pathlib.Path(os.path.dirname(os.path.realpath(__file__)))
CARGO_DEFAULT_ARGS = ["--color", "always"]
ARCH = "x86_64"
# TODO: should be generated for enabling parallel builds
QEMU_TAP_NAME = 'tap0'
QEMU_TAP_ZONE = '172.31.0.20/24'

#
# Important globals
#
BOOTLOADER_PATH = (SCRIPT_PATH / '..').resolve() / 'bootloader'
TARGET_PATH = (SCRIPT_PATH / '..').resolve() / 'target'
KERNEL_PATH = SCRIPT_PATH
LIBS_PATH = (SCRIPT_PATH / '..').resolve() / 'lib'
USR_PATH = (SCRIPT_PATH / '..').resolve() / 'usr'

UEFI_TARGET = "{}-uefi".format(ARCH)
KERNEL_TARGET = "{}-bespin".format(ARCH)
USER_TARGET = "{}-bespin-none".format(ARCH)

#
# Command line argument parser
#
parser = argparse.ArgumentParser()
# General build arguments
parser.add_argument("-v", "--verbose", action="store_true",
                    help="increase output verbosity")
parser.add_argument("-n", "--norun", action="store_true",
                    help="Only build, don't run")
parser.add_argument("-r", "--release", action="store_true",
                    help="Do a release build.")
parser.add_argument("-f", "--kfeatures", type=str, nargs='+', default=[],
                    help="Cargo features to enable (in the kernel).")
parser.add_argument("-u", "--ufeatures", type=str, nargs='+', default=[],
                    help="Cargo features to enable (in user-space, use module_name:feature_name syntax to specify module specific features, e.g. init:print-test).")
parser.add_argument('-m', '--mods', nargs='+', default=['init'],
                    help='User-space modules to be included in build & deployment', required=False)
parser.add_argument("-c", "--cmd", type=str,
                    help="Command line arguments passed to the kernel.")
parser.add_argument('-t', '--machine',
                    help='Which machine to run on (defaults to qemu)', required=False, default='qemu')
# QEMU related arguments
parser.add_argument("-q", "--qemu-settings", type=str,
                    help="Pass generic QEMU arguments.")
parser.add_argument("-a", "--qemu-nodes", type=int,
                    help="How many NUMA nodes (for qemu).")
parser.add_argument("-s", "--qemu-cores", type=int,
                    help="How many cores nodes (evenly divided across NUMA nodes, for qemu).")
parser.add_argument("-d", "--qemu-debug-cpu", action="store_true",
                    help="Debug CPU reset (for qemu)")
parser.add_argument("-o", "--qemu-monitor", action="store_true",
                    help="Launch the QEMU monitor (for qemu)")

BESPIN_EXIT_CODES = {
    0: "[SUCCESS]",
    1: "[FAIL] ReturnFromMain: main() function returned to arch_indepdendent part.",
    2: "[FAIL] Encountered kernel panic.",
    3: "[FAIL] Encountered OOM.",
    4: "[FAIL] Encountered unexpected Interrupt.",
    5: "[FAIL] General Protection Fault.",
    6: "[FAIL] Unexpected Page Fault.",
    7: "[FAIL] Unexpected process exit code when running a user-space test.",
    8: "[FAIL] Unexpected exception during kernel initialization.",
    9: "[FAIL] Got unrecoverable error (machine check, double fault)."
}


def log(msg):
    print(colors.bold | ">>>", end=" "),
    print(colors.bold.reset & colors.info | msg)


def build_bootloader(args):
    "Builds the bootloader, copies the binary in the target UEFI directory"
    log("Build bootloader")
    uefi_build_args = ['build', '--target', UEFI_TARGET]
    uefi_build_args += ['--package', 'bootloader']
    uefi_build_args += CARGO_DEFAULT_ARGS

    with local.cwd(BOOTLOADER_PATH):
        with local.env(RUST_TARGET_PATH=BOOTLOADER_PATH.absolute()):
            xargo(*uefi_build_args)


def build_kernel(args):
    "Builds the kernel binary"
    log("Build kernel")
    with local.cwd(KERNEL_PATH):
        with local.env(RUST_TARGET_PATH=(KERNEL_PATH / 'src' / 'arch' / ARCH).absolute()):
            # TODO(cross-compilation): in case we use a cross compiler/linker
            # also set: CARGO_TARGET_X86_64_BESPIN_LINKER=x86_64-elf-ld
            build_args = ['build', '--target', KERNEL_TARGET]
            for feature in args.kfeatures:
                build_args += ['--features', feature]
            build_args += CARGO_DEFAULT_ARGS
            xargo(*build_args)


def build_user_libraries(args):
    "Builds bespin vibrio lib to provide runtime support for other rump based apps"
    log("Build user-space lib vibrio")
    build_args = ['build', '--target', USER_TARGET]
    build_args += ["--features", "rumprt"]
    build_args += CARGO_DEFAULT_ARGS

    # Make sure we build a static (.a) vibrio library
    # For linking with rumpkernel
    with local.cwd(LIBS_PATH / "vibrio"):
        with local.env(RUST_TARGET_PATH=USR_PATH.absolute()):
            xargo(*build_args)


def build_userspace(args):
    "Builds user-space programs"
    build_args_default = ['build', '--target', USER_TARGET]
    build_args_default += CARGO_DEFAULT_ARGS

    for module in args.mods:
        if not (USR_PATH / module).exists():
            log("User module {} not found, skipping.".format(module))
            continue
        with local.cwd(USR_PATH / module):
            with local.env(RUST_TARGET_PATH=USR_PATH.absolute()):
                build_args = build_args_default.copy()
                for feature in args.ufeatures:
                    if ':' in feature:
                        mod_part, feature_part = feature.split(':')
                        if module == mod_part:
                            build_args += ['--features', feature_part]
                    else:
                        build_args += ['--features', feature]
                log("Build user-module {}".format(module))
                xargo(*build_args)


def deploy(args):
    """
    Deploys everything that got built to the UEFI ESP directory
    Also builds a disk image (.img file)
    """
    log("Deploy binaries")

    # Clean up / create ESP dir structure
    debug_release = 'release' if args.release else 'debug'
    uefi_build_path = TARGET_PATH / UEFI_TARGET / debug_release
    user_build_path = TARGET_PATH / USER_TARGET / debug_release
    kernel_build_path = TARGET_PATH / KERNEL_TARGET / debug_release

    # Clean and create_esp dir:
    esp_path = uefi_build_path / 'esp'
    if esp_path.exists() and esp_path.is_dir():
        shutil.rmtree(esp_path, ignore_errors=False)
    esp_boot_path = esp_path / "EFI" / "Boot"
    esp_boot_path.mkdir(parents=True, exist_ok=True)

    # Deploy bootloader
    shutil.copy2(kernel_build_path / 'bespin', os.getcwd())
    shutil.copy2(kernel_build_path / 'bespin', esp_path / 'kernel')

    # Deploy kernel
    shutil.copy2(uefi_build_path / 'bootloader.efi',
                 esp_boot_path / 'BootX64.efi')

    # Write kernel cmd-line file in ESP dir
    with open(esp_path / 'cmdline.in', 'w') as cmdfile:
        cmdfile.write('./kernel {}'.format(args.cmd))

    # Deploy user-modules
    for module in args.mods:
        if not (user_build_path / module).is_file():
            continue
        if module != "rkapps":
            shutil.copy2(user_build_path / module, esp_path)
        else:
            # TODO(ugly): Special handling of the rkapps module
            # (they end up being built as multiple .bin binaries)
            to_copy = [app for app in user_build_path.glob(
                "*.bin") if app.is_file()]
            for app in to_copy:
                shutil.copy2(app, esp_path)


def run(args):
    """
    Run the system on a hardware/emulation platform
    Returns: A bespin exit error code.
    """
    def run_qemu(args):
        log("Starting QEMU")
        debug_release = 'release' if args.release else 'debug'
        esp_path = TARGET_PATH / UEFI_TARGET / debug_release / 'esp'

        qemu_default_args = ['-no-reboot']
        # Setup KVM and required guest hardware features
        qemu_default_args += ['-enable-kvm']
        qemu_default_args += ['-cpu',
                              'host,migratable=no,+invtsc,+tsc,+x2apic,+fsgsbase']
        # Use serial communication
        # '-nographic',
        qemu_default_args += ['-display', 'none', '-serial', 'stdio']

        # Add UEFI bootloader support
        qemu_default_args += ['-drive',
                              'if=pflash,format=raw,file={}/OVMF_CODE.fd,readonly=on'.format(BOOTLOADER_PATH)]
        qemu_default_args += ['-drive',
                              'if=pflash,format=raw,file={}/OVMF_VARS.fd,readonly=on'.format(BOOTLOADER_PATH)]
        qemu_default_args += ['-device', 'ahci,id=ahci,multifunction=on']
        qemu_default_args += ['-drive',
                              'if=none,format=raw,file=fat:rw:{},id=esp'.format(esp_path)]
        qemu_default_args += ['-device', 'ide-drive,bus=ahci.0,drive=esp']

        # Debug port to exit qemu and communicate back exit-code for tests
        qemu_default_args += ['-device',
                              'isa-debug-exit,iobase=0xf4,iosize=0x04']

        # Enable networking with outside world
        qemu_default_args += ['-net', 'nic,model=e1000,netdev=n0']
        qemu_default_args += ['-netdev', 'tap,id=n0,script=no,ifname=tap0']

        if args.qemu_debug_cpu:
            qemu_default_args += ['-d', 'int,cpu_reset']
        if args.qemu_monitor:
            qemu_default_args += ['-monitor',
                                  'telnet:127.0.0.1:55555,server,nowait']
        if not args.qemu_settings:
            qemu_default_args += ['-m', '1024']

        qemu_args = qemu_default_args.copy()
        if args.qemu_settings:
            qemu_args += args.qemu_settings.split()

        # Create a tap interface to communicate with guest and give it an IP
        user = (whoami)().strip()
        group = (local['id']['-gn'])().strip()
        (sudo[tunctl[['-t', QEMU_TAP_NAME, '-u', user, '-g', group]]])()
        (sudo[ifconfig[QEMU_TAP_NAME, 'ip', QEMU_TAP_ZONE]])

        # TODO(cosmetics): Ideally we would do something like this:
        #   qemu = local['qemu-system-x86_64']
        #   (qemu)(*qemu_args, timeout=320) & FG(buffering=None)
        # But it somehow buffers the qemu output, and I couldn't figure out why :/

        # Run a QEMU instance
        cmd = ' '.join(['qemu-system-x86_64'] + qemu_args)
        execution = subprocess.run(cmd, shell=True, stderr=subprocess.PIPE)
        bespin_exit_code = execution.returncode >> 1
        if BESPIN_EXIT_CODES.get(bespin_exit_code):
            print(BESPIN_EXIT_CODES[bespin_exit_code])
        else:
            print(
                "[FAIL] Kernel exited with unknown error status {}... Update the script!".format(bespin_exit_code))

        if bespin_exit_code != 0:
            log("Invocation was: {}".format(cmd))
            if execution.stderr:
                print("STDERR: {}".format(execution.stderr.decode('utf-8')))

        return bespin_exit_code

    if args.machine == 'qemu':
        return run_qemu(args)
    else:
        log("Machine {} not supported".format(args.machine))
        return 99


#
# Main routine of run.py
#
if __name__ == '__main__':
    "Execution pipeline for building and launching bespin"
    args = parser.parse_args()

    if args.machine != 'qemu' and (args.qemu_debug_cpu or args.qemu_settings or args.qemu_monitor or args.qemu_cores or args.qemu_nodes):
        log("Can't specify QEMU specific arguments for non-qemu hardware")
        sys.exit(99)

    if args.release:
        CARGO_DEFAULT_ARGS.append("--release")
    if args.verbose:
        CARGO_DEFAULT_ARGS.append("--verbose")
    else:
        # Minimize python exception backtraces
        sys.excepthook = exception_handler

    # Build
    build_bootloader(args)
    build_kernel(args)
    build_user_libraries(args)
    build_userspace(args)

    # Deploy
    deploy(args)

    # Run
    if not args.norun:
        r = run(args)
        sys.exit(r)
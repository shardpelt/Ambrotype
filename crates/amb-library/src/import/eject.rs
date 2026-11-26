// SPDX-License-Identifier: GPL-3.0-or-later
//! Eject the card an import came from: find the mount the folder is on and
//! hand it to the system's own tool. Only ever runs when the person asked.

use std::path::Path;
use std::process::Command;

const NOT_REMOVABLE: &str =
    "This folder is not on a card or removable drive, so there is nothing to eject.";

/// A system tool that ejects: the program and its arguments, where `{mount}`
/// is the mount point (a drive letter on Windows) and `{device}` the device.
struct Tool {
    program: &'static str,
    args: &'static [&'static str],
}

/// The tools to try, in order; the first that succeeds ends it.
#[cfg(target_os = "linux")]
const TOOLS: &[Tool] = &[
    Tool {
        program: "gio",
        args: &["mount", "-e", "{mount}"],
    },
    Tool {
        program: "udisksctl",
        args: &["unmount", "-b", "{device}"],
    },
];
#[cfg(target_os = "macos")]
const TOOLS: &[Tool] = &[Tool {
    program: "diskutil",
    args: &["eject", "{mount}"],
}];
#[cfg(windows)]
const TOOLS: &[Tool] = &[Tool {
    program: "powershell",
    args: &[
        "-NoProfile",
        "-Command",
        "(New-Object -comObject Shell.Application).Namespace(17).ParseName('{mount}').InvokeVerb('Eject')",
    ],
}];
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
const TOOLS: &[Tool] = &[];

/// Where a folder is mounted: the mount point and its device.
#[derive(Debug, PartialEq)]
struct Mount {
    point: String,
    device: String,
}

/// Mounts text lines (`/proc/self/mounts`) hold a space as `\040`.
fn unescape_mount(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('\\') {
        out.push_str(&rest[..i]);
        let code = rest
            .get(i + 1..i + 4)
            .and_then(|d| u8::from_str_radix(d, 8).ok());
        match code {
            Some(c) => {
                out.push(c as char);
                rest = &rest[i + 4..];
            }
            None => {
                out.push('\\');
                rest = &rest[i + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The mount `path` is on, from the text of `/proc/self/mounts`: the one with
/// the longest mount point that is a prefix of `path`. `None` for the root
/// filesystem and for anything that isn't a real device (tmpfs, proc, ...).
fn mount_of(path: &Path, mounts: &str) -> Option<Mount> {
    let best = mounts
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some((unescape_mount(parts.next()?), unescape_mount(parts.next()?)))
        })
        .filter(|(_, point)| path.starts_with(point))
        .max_by_key(|(_, point)| Path::new(point).components().count())?;
    let (device, point) = best;
    (point != "/" && device.starts_with("/dev/")).then_some(Mount { point, device })
}

/// The volume `path` is on under macOS: `/Volumes/<name>`.
#[cfg(any(target_os = "macos", test))]
fn volume_of(path: &Path) -> Option<std::path::PathBuf> {
    let rest = path.strip_prefix("/Volumes").ok()?;
    let name = rest.components().next()?;
    Some(Path::new("/Volumes").join(name))
}

/// The drive letter `path` is on under Windows, as `D:`; the system drive
/// doesn't count.
#[cfg(any(windows, test))]
fn drive_of(path: &str, system_drive: &str) -> Option<String> {
    let path = path.strip_prefix(r"\\?\").unwrap_or(path);
    let mut chars = path.chars();
    let (letter, colon) = (chars.next()?, chars.next()?);
    let drive = format!("{}:", letter.to_ascii_uppercase());
    (letter.is_ascii_alphabetic() && colon == ':' && !drive.eq_ignore_ascii_case(system_drive))
        .then_some(drive)
}

#[cfg(target_os = "linux")]
fn find_mount(source: &Path) -> Option<Mount> {
    mount_of(source, &std::fs::read_to_string("/proc/self/mounts").ok()?)
}

#[cfg(target_os = "macos")]
fn find_mount(source: &Path) -> Option<Mount> {
    let point = volume_of(source)?.display().to_string();
    Some(Mount {
        device: point.clone(),
        point,
    })
}

#[cfg(windows)]
fn find_mount(source: &Path) -> Option<Mount> {
    let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let point = drive_of(&source.display().to_string(), &system)?;
    Some(Mount {
        device: point.clone(),
        point,
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn find_mount(_source: &Path) -> Option<Mount> {
    None
}

/// Eject the card or drive `source` is on, with the system's own tool.
/// The error says what to do next, in words for the person at the screen.
pub fn eject(source: &Path) -> Result<(), String> {
    let source = std::fs::canonicalize(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let mount = find_mount(&source).ok_or(NOT_REMOVABLE)?;
    let mut last = None;
    for tool in TOOLS {
        let args = tool.args.iter().map(|a| {
            a.replace("{mount}", &mount.point)
                .replace("{device}", &mount.device)
        });
        match Command::new(tool.program).args(args).output() {
            Ok(out) if out.status.success() => return Ok(()),
            Ok(out) => {
                let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
                last = Some(why);
            }
            // The tool isn't installed here: try the next.
            Err(_) => {}
        }
    }
    Err(match last {
        Some(why) if !why.is_empty() => {
            format!("Could not eject {}: {why}", mount.point)
        }
        _ => format!(
            "Could not eject {}. Eject it from your file manager instead.",
            mount.point
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNTS: &str = "\
sysfs /sys sysfs rw 0 0
/dev/nvme0n1p2 / ext4 rw 0 0
/dev/nvme0n1p1 /boot/efi vfat rw 0 0
tmpfs /run/user/1000 tmpfs rw 0 0
/dev/sdb1 /media/me/EOS_DIGITAL ext4 rw 0 0
/dev/sdc1 /media/me/MY\\040CARD vfat rw 0 0
";

    #[test]
    fn a_card_is_found_by_its_longest_mount_prefix() {
        let m = mount_of(Path::new("/media/me/EOS_DIGITAL/DCIM/100EOS"), MOUNTS).unwrap();
        assert_eq!(m.point, "/media/me/EOS_DIGITAL");
        assert_eq!(m.device, "/dev/sdb1");
        let m = mount_of(Path::new("/media/me/MY CARD/DCIM"), MOUNTS).unwrap();
        assert_eq!(m.point, "/media/me/MY CARD");
    }

    #[test]
    fn the_root_filesystem_and_virtual_mounts_are_not_ejectable() {
        assert_eq!(mount_of(Path::new("/home/me/photos"), MOUNTS), None);
        assert_eq!(mount_of(Path::new("/run/user/1000/x"), MOUNTS), None);
        // A folder merely named like a card's isn't on it.
        assert_eq!(mount_of(Path::new("/media/me/EOS_DIGITAL2"), MOUNTS), None);
    }

    #[test]
    fn macos_volumes_and_windows_drives() {
        assert_eq!(
            volume_of(Path::new("/Volumes/CARD/DCIM")),
            Some(std::path::PathBuf::from("/Volumes/CARD"))
        );
        assert_eq!(volume_of(Path::new("/Users/me")), None);
        assert_eq!(drive_of(r"\\?\e:\DCIM", "C:"), Some("E:".into()));
        assert_eq!(drive_of(r"C:\Users", "C:"), None);
        assert_eq!(drive_of("/mnt/x", "C:"), None);
    }
}

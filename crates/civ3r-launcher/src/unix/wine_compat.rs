//! Prepare the measured Intel macOS Wine compatibility before any Wine code runs.
//!
//! Wine 11.0_1 can return to 32-bit code with stale SS state in this VM. Its
//! loader searches its installed directory before WINEDLLPATH. A private runtime
//! view therefore owns an unchanged native loader and ntdll.so, plus one patched
//! CPU DLL. Everything else is a link to the installed dependency. No live code,
//! shared Wine file, prefix DLL, game asset or registry value is changed.
use sha2::{Digest, Sha256};
use std::{
    ffi::OsString,
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::Command,
};

const DLL_LEN: usize = 36_864;
const LOADER_LEN: usize = 27_680;
const NTDLL_LEN: usize = 577_552;
const DLL_SHA: [u8; 32] = hex("cbbf4d054d5488e19d82ea24e7c280c3408ee43b00fce708d1b559acada9d21e");
const LOADER_SHA: [u8; 32] =
    hex("61ae8ab79e35f4982d5f6be14a3a26dcc5383578a298d9f767811086b40020d3");
const NTDLL_SHA: [u8; 32] = hex("dfb9d4d4d6e7fd3dc537c1bacd69cb3a6bb46ae98311bb5358df3c4eab470c3f");
const BIN_SHA: [u8; 32] = hex("bad3b6126b6612680e26302c0e0b7e56d6c7036eac9bf6313878bdd9039b003f");
const PATCHED_SHA: [u8; 32] =
    hex("d0da82572f7cc2eb3a63a135bb9abe25427916badde18c99b75f547883510d22");
const MAX_LINKS: usize = 4_096;
const DISPATCH: usize = 0x1214;
const RETURN: usize = 0x1296;
const STUB: usize = 0x1b00;
const BRANCH: [u8; 6] = [0xe9, 0x65, 0x08, 0x00, 0x00, 0x90];
// Reload the saved 32-bit SS while still on the 64-bit stack, then execute
// Wine's original xchg rsp,r14; ljmp [r14]. No extra stack space or ABI state.
const RELOAD: [u8; 15] = [
    0x41, 0x8b, 0x95, 0xc8, 0x00, 0x00, 0x00, 0x8e, 0xd2, 0x4c, 0x87, 0xf4, 0x41, 0xff, 0x2e,
];
const DISPATCH_BYTES: [u8; 136] = [
    0x4c, 0x87, 0xf4, 0x41, 0x89, 0xbd, 0x9c, 0x00, 0x00, 0x00, 0x41, 0x89, 0xb5, 0xa0, 0x00, 0x00,
    0x00, 0x41, 0x89, 0x9d, 0xa4, 0x00, 0x00, 0x00, 0x41, 0x89, 0xad, 0xb4, 0x00, 0x00, 0x00, 0x41,
    0x8b, 0x16, 0x41, 0x89, 0x95, 0xb8, 0x00, 0x00, 0x00, 0x8b, 0x15, 0xc9, 0x4d, 0x00, 0x00, 0x41,
    0x89, 0x95, 0xbc, 0x00, 0x00, 0x00, 0x49, 0x8d, 0x56, 0x14, 0x41, 0x89, 0x95, 0xc4, 0x00, 0x00,
    0x00, 0x49, 0x8b, 0x4e, 0x04, 0x41, 0x8b, 0x56, 0x0c, 0x45, 0x8b, 0x46, 0x10, 0xff, 0x15, 0x99,
    0x4d, 0x00, 0x00, 0x41, 0x89, 0x85, 0xb0, 0x00, 0x00, 0x00, 0x41, 0x0f, 0xba, 0x75, 0xfc, 0x00,
    0x0f, 0x82, 0x39, 0xff, 0xff, 0xff, 0x41, 0x8b, 0x95, 0xb8, 0x00, 0x00, 0x00, 0x89, 0x14, 0x24,
    0x41, 0x8b, 0x95, 0xbc, 0x00, 0x00, 0x00, 0x89, 0x54, 0x24, 0x04, 0x45, 0x8b, 0xb5, 0xc4, 0x00,
    0x00, 0x00, 0x4c, 0x87, 0xf4, 0x41, 0xff, 0x2e,
];

const fn digit(c: u8) -> u8 {
    if c <= b'9' { c - b'0' } else { c - b'a' + 10 }
}
const fn hex(input: &str) -> [u8; 32] {
    let bytes = input.as_bytes();
    let mut out = [0; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = digit(bytes[i * 2]) * 16 + digit(bytes[i * 2 + 1]);
        i += 1;
    }
    out
}

fn unsupported(reason: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!(
        "Intel macOS Wine compatibility cannot be prepared: {reason}. Use the unmodified Wine Stable 11.0_1 osx64 runtime; other Intel macOS Wine builds require validation before use"
    ))
}
fn read_exact_image(path: &Path, len: usize, digest: [u8; 32]) -> io::Result<Vec<u8>> {
    let file = File::open(path).map_err(unsupported)?;
    if !file.metadata()?.is_file() {
        return Err(unsupported(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    let mut bytes = Vec::with_capacity(len);
    file.take(len as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() != len || Sha256::digest(&bytes).as_slice() != digest {
        return Err(unsupported(format!(
            "{} has an unrecognized image signature",
            path.display()
        )));
    }
    Ok(bytes)
}
fn dword(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn word(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}
fn patched(mut bytes: Vec<u8>) -> io::Result<Vec<u8>> {
    // The full digest fixes the exact version, all sections, imports, unwind
    // metadata, builtin marker and relocation table. Explicit layout checks
    // document the offsets needed by this one compatibility transformation.
    if bytes.len() != DLL_LEN || Sha256::digest(&bytes).as_slice() != DLL_SHA {
        return Err(unsupported(
            "wow64cpu.dll is not the exact measured Wine 11.0_1 image",
        ));
    }
    if &bytes[..2] != b"MZ"
        || dword(&bytes, 0x3c) != 0x80
        || &bytes[0x80..0x84] != b"PE\0\0"
        || word(&bytes, 0x84) != 0x8664
        || word(&bytes, 0x86) != 9
        || dword(&bytes, 0x88) != 0x69e0_eabd
        || word(&bytes, 0x94) != 0xf0
        || word(&bytes, 0x98) != 0x20b
        || dword(&bytes, 0xa8) != 0x1920
        || dword(&bytes, 0xd0) != 0xb000
        || &bytes[0x188..0x190] != b".text\0\0\0"
        || dword(&bytes, 0x190) != 0x9d0
        || dword(&bytes, 0x194) != 0x1000
        || dword(&bytes, 0x198) != 0x1000
        || dword(&bytes, 0x19c) != 0x1000
        || dword(&bytes, 0x1ac) != 0x6000_0020
        || &bytes[0x200..0x208] != b".pdata\0\0"
        || dword(&bytes, 0x208) != 0x114
        || dword(&bytes, 0x20c) != 0x4000
        || dword(&bytes, 0x210) != 0x1000
        || dword(&bytes, 0x214) != 0x4000
        || dword(&bytes, 0x120) != 0x4000
        || dword(&bytes, 0x124) != 0x114
        || bytes[0x406c..0x4078] != [0x14, 0x12, 0, 0, 0x9c, 0x12, 0, 0, 0x30, 0x50, 0, 0]
        || bytes[0x5030..0x5040]
            != [
                1, 0, 5, 0, 0, 0x42, 0, 0x70, 0, 0x60, 0, 0x30, 0, 0x50, 0, 0,
            ]
        || bytes[0x4114..0x5000].iter().any(|&byte| byte != 0)
        || bytes[DISPATCH..DISPATCH + DISPATCH_BYTES.len()] != DISPATCH_BYTES
        || bytes[0x19d0..0x2000].iter().any(|&byte| byte != 0)
    {
        return Err(unsupported("the measured CPU DLL layout does not match"));
    }
    bytes[RETURN..RETURN + BRANCH.len()].copy_from_slice(&BRANCH);
    bytes[STUB..STUB + RELOAD.len()].copy_from_slice(&RELOAD);
    // Keep the new return code within .text's declared virtual size, rather
    // than relying on loader-specific treatment of trailing raw padding.
    bytes[0x190..0x194].copy_from_slice(&0xb0fu32.to_le_bytes());
    // The new code runs on the dispatcher's existing non-leaf stack. Share
    // its unwind record instead of incorrectly treating the stub as a leaf.
    // The appended range follows every original .text function in RVA order.
    for (offset, value) in [
        (0x4114, 0x1b00u32),
        (0x4118, 0x1b0f),
        (0x411c, 0x5030),
        (0x208, 0x120),
        (0x124, 0x120),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    if Sha256::digest(&bytes).as_slice() != PATCHED_SHA {
        return Err(unsupported(
            "private CPU DLL transformation did not match its expected digest",
        ));
    }
    Ok(bytes)
}

/// Owns at most one 642,096-byte session view. Drop runs after owned Wine
/// children have been stopped. Symlinks are removed without following targets.
pub(super) struct Runtime {
    executable: PathBuf,
    private: Option<PathBuf>,
}
impl Runtime {
    #[cfg(test)]
    fn executable(&self) -> &Path {
        &self.executable
    }

    /// The original Unix libraries import @rpath/ntdll.so. Their symlink
    /// targets have the original @loader_path, so prefer our copied ntdll for
    /// each child to prevent two independent Wine global-state instances.
    pub(super) fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        if let Some(private) = &self.private {
            command.env(
                "DYLD_LIBRARY_PATH",
                library_path(private, std::env::var_os("DYLD_LIBRARY_PATH")),
            );
        }
        command
    }
    pub(super) fn prepare(wine: &Path, session: &Path) -> io::Result<Self> {
        if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
            Self::prepare_intel(wine, session)
        } else {
            Ok(Self {
                executable: wine.to_owned(),
                private: None,
            })
        }
    }
    fn prepare_intel(wine: &Path, session: &Path) -> io::Result<Self> {
        let wine = wine.canonicalize().map_err(unsupported)?;
        let parent = wine
            .parent()
            .ok_or_else(|| unsupported("Wine has no directory"))?;
        let root = if parent.file_name().is_some_and(|v| v == "bin") {
            read_exact_image(&wine, 31_856, BIN_SHA)?;
            parent.parent()
        } else if parent.ends_with("lib/wine/x86_64-unix") {
            read_exact_image(&wine, LOADER_LEN, LOADER_SHA)?;
            parent.ancestors().nth(3)
        } else {
            None
        }
        .ok_or_else(|| unsupported("Wine has an unrecognized installation layout"))?;
        let unix = root.join("lib/wine/x86_64-unix");
        let loader = read_exact_image(&unix.join("wine"), LOADER_LEN, LOADER_SHA)?;
        let ntdll = read_exact_image(&unix.join("ntdll.so"), NTDLL_LEN, NTDLL_SHA)?;
        let dll = patched(read_exact_image(
            &root.join("lib/wine/x86_64-windows/wow64cpu.dll"),
            DLL_LEN,
            DLL_SHA,
        )?)?;
        let destination = session.join("wine-compat");
        DirBuilder::new().mode(0o700).create(&destination)?;
        let result = Self {
            executable: destination.join("lib/wine/x86_64-unix/wine"),
            private: Some(destination.clone()),
        };
        let mut links = 0;
        prepare_directory(root, &destination, Path::new(""), &mut links)?;
        // Publication happens only after all checks, links and complete file
        // writes. No child knows this private path until prepare returns.
        publish(
            &destination.join("lib/wine/x86_64-unix/wine"),
            &loader,
            0o500,
        )?;
        publish(
            &destination.join("lib/wine/x86_64-unix/ntdll.so"),
            &ntdll,
            0o400,
        )?;
        publish(
            &destination.join("lib/wine/x86_64-windows/wow64cpu.dll"),
            &dll,
            0o400,
        )?;
        eprintln!(
            "civ3-play-macos: prepared private Wine 11.0_1 SS-return compatibility at {}",
            destination.display()
        );
        Ok(result)
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(path) = &self.private {
            let _ = fs::remove_dir_all(path);
        }
    }
}
fn library_path(private: &Path, inherited: Option<OsString>) -> OsString {
    let mut path = private.join("lib/wine/x86_64-unix").into_os_string();
    if let Some(inherited) = inherited.filter(|value| !value.is_empty()) {
        path.push(":");
        path.push(inherited);
    }
    path
}
fn prepare_directory(
    root: &Path,
    destination: &Path,
    relative: &Path,
    links: &mut usize,
) -> io::Result<()> {
    for entry in fs::read_dir(root.join(relative))? {
        let entry = entry?;
        *links += 1;
        if *links > MAX_LINKS {
            return Err(unsupported(
                "runtime directory exceeds the private-view entry limit",
            ));
        }
        let next = relative.join(entry.file_name());
        let target = destination.join(&next);
        if [
            "lib",
            "lib/wine",
            "lib/wine/x86_64-unix",
            "lib/wine/x86_64-windows",
        ]
        .iter()
        .any(|&v| next == Path::new(v))
        {
            if !entry.file_type()?.is_dir() {
                return Err(unsupported(
                    "runtime directory layout contains an unexpected link",
                ));
            }
            DirBuilder::new().mode(0o700).create(&target)?;
            prepare_directory(root, destination, &next, links)?;
        } else if ![
            "lib/wine/x86_64-unix/wine",
            "lib/wine/x86_64-unix/ntdll.so",
            "lib/wine/x86_64-windows/wow64cpu.dll",
        ]
        .iter()
        .any(|&v| next == Path::new(v))
        {
            symlink(root.join(&next), target)?;
        }
    }
    Ok(())
}
fn publish(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    let temporary = path.with_extension("preparing");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    // No concurrent reader exists yet. Rename gives one complete file even
    // if later preparation fails. The owning guard removes partial views.
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_and_unknown_images_are_rejected_before_indexing() {
        for len in [0, 1, 128, DLL_LEN - 1, DLL_LEN, DLL_LEN + 1] {
            assert!(patched(vec![0; len]).is_err());
        }
    }
    #[test]
    fn jump_targets_stub_and_stub_preserves_original_return() {
        let displacement = i32::from_le_bytes(BRANCH[1..5].try_into().unwrap());
        assert_eq!((RETURN + 5) as i32 + displacement, STUB as i32);
        assert_eq!(&RELOAD[9..], &DISPATCH_BYTES[130..]);
        assert_eq!(STUB + RELOAD.len(), 0x1b0f);
    }
    #[test]
    fn unknown_executable_is_rejected_without_creating_a_view() {
        let temp = std::env::temp_dir().join(format!("c3r-wine-compat-{}", std::process::id()));
        fs::create_dir(&temp).unwrap();
        fs::write(temp.join("wine"), b"unknown").unwrap();
        assert!(Runtime::prepare_intel(&temp.join("wine"), &temp).is_err());
        assert!(!temp.join("wine-compat").exists());
        fs::remove_dir_all(temp).unwrap();
    }
    #[test]
    fn complete_files_publish_with_final_permissions() {
        let temp = std::env::temp_dir().join(format!("c3r-wine-publish-{}", std::process::id()));
        fs::create_dir(&temp).unwrap();
        let file = temp.join("wine");
        publish(&file, b"complete image", 0o500).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"complete image");
        assert_eq!(
            fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o500
        );
        assert!(!file.with_extension("preparing").exists());
        fs::remove_dir_all(temp).unwrap();
    }
    #[test]
    fn entry_limit_failure_cleans_owned_view_without_following_links() {
        let temp = std::env::temp_dir().join(format!("c3r-wine-limit-{}", std::process::id()));
        let source = temp.join("source");
        let private = temp.join("private");
        fs::create_dir(&temp).unwrap();
        fs::create_dir(&source).unwrap();
        fs::create_dir(&private).unwrap();
        fs::write(source.join("untouched"), b"source").unwrap();
        symlink(source.join("untouched"), private.join("link")).unwrap();
        let runtime = Runtime {
            executable: private.join("wine"),
            private: Some(private.clone()),
        };
        let mut links = MAX_LINKS;
        assert!(prepare_directory(&source, &private, Path::new(""), &mut links).is_err());
        drop(runtime);
        assert!(!private.exists());
        assert_eq!(fs::read(source.join("untouched")).unwrap(), b"source");
        fs::remove_dir_all(temp).unwrap();
    }
    #[test]
    fn child_library_path_prefers_private_and_preserves_inherited_entries() {
        let private = Path::new("/private/session/wine-compat");
        assert_eq!(
            library_path(private, None),
            "/private/session/wine-compat/lib/wine/x86_64-unix"
        );
        assert_eq!(
            library_path(private, Some("".into())),
            library_path(private, None)
        );
        assert_eq!(
            library_path(private, Some("/custom/one:/custom/two".into())),
            "/private/session/wine-compat/lib/wine/x86_64-unix:/custom/one:/custom/two"
        );
        let unmodified = Runtime {
            executable: "/original/wine".into(),
            private: None,
        };
        assert!(
            unmodified
                .command()
                .get_envs()
                .all(|(name, _)| name != "DYLD_LIBRARY_PATH")
        );
        let mut modified = Runtime {
            executable: private.join("wine"),
            private: Some(private.to_owned()),
        };
        assert!(
            modified
                .command()
                .get_envs()
                .any(|(name, value)| name == "DYLD_LIBRARY_PATH" && value.is_some())
        );
        // Do not remove the synthetic path used by this command-only test.
        modified.private = None;
    }
    #[test]
    #[ignore = "requires CIV3R_TEST_WINE_ROOT with the external official Wine 11.0_1 runtime"]
    fn exact_image_and_private_view_are_verified() {
        let source = PathBuf::from(
            std::env::var_os("CIV3R_TEST_WINE_ROOT").expect("external Wine runtime root"),
        );
        let temp = std::env::temp_dir().join(format!("c3r-wine-offline-{}", std::process::id()));
        fs::create_dir(&temp).unwrap();
        let original = fs::read(source.join("lib/wine/x86_64-windows/wow64cpu.dll")).unwrap();
        let expected = patched(original.clone()).unwrap();
        assert_eq!(expected[RETURN..RETURN + 6], BRANCH);
        assert_eq!(expected[STUB..STUB + 15], RELOAD);
        assert_eq!(dword(&expected, 0x190), 0xb0f);
        assert_eq!(dword(&expected, 0x208), 0x120);
        assert_eq!(dword(&expected, 0x124), 0x120);
        assert_eq!(
            [
                dword(&expected, 0x4114),
                dword(&expected, 0x4118),
                dword(&expected, 0x411c)
            ],
            [0x1b00, 0x1b0f, 0x5030]
        );
        let changes = [
            (0x124, 0x128),
            (0x190, 0x194),
            (0x208, 0x20c),
            (RETURN, RETURN + 6),
            (STUB, STUB + 15),
            (0x4114, 0x4120),
        ];
        for at in 0..DLL_LEN {
            if !changes
                .iter()
                .any(|&(begin, end)| (begin..end).contains(&at))
            {
                assert_eq!(expected[at], original[at], "unplanned change at {at:x}");
            }
        }
        assert!(
            patched(expected.clone()).is_err(),
            "already modified shared input must be rejected"
        );
        for at in [0x40, 0x88, 0x190, DISPATCH, RETURN, STUB, DLL_LEN - 1] {
            let mut changed = original.clone();
            changed[at] ^= 1;
            assert!(patched(changed).is_err(), "mutated source byte {at:x}");
        }
        let runtime = Runtime::prepare_intel(&source.join("bin/wine"), &temp).unwrap();
        assert!(runtime.executable().is_file());
        let view = temp.join("wine-compat");
        let private_dll = view.join("lib/wine/x86_64-windows/wow64cpu.dll");
        assert!(
            !fs::symlink_metadata(&private_dll)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(private_dll).unwrap(), expected);
        assert_eq!(
            fs::read(source.join("lib/wine/x86_64-windows/wow64cpu.dll")).unwrap(),
            original
        );
        assert!(
            fs::symlink_metadata(view.join("bin"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::metadata(runtime.executable())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o500
        );
        drop(runtime);
        assert!(!view.exists());
        assert!(source.join("bin/wine").exists());
        fs::remove_dir(temp).unwrap();
    }
}

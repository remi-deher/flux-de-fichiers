//! Landlock par appels système directs : aucune dépendance de plus (ADR 0034, point 4).
//!
//! Landlock limite les fichiers (noyau 5.13) et les connexions TCP (noyau 6.7) d'un processus et de ses
//! descendants, sans droit particulier. Les numéros d'appel sont les mêmes sur x86_64 et aarch64.

use super::{ConfineError, last_os_error, unsupported};
use forgearr_shared::confinement::{ConfinementProfile, LANDLOCK_ABI_WITH_NETWORK, NetworkAccess};
use std::{
    ffi::CString,
    fs::File,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::Path,
};

const SYS_LANDLOCK_CREATE_RULESET: libc::c_long = 444;
const SYS_LANDLOCK_ADD_RULE: libc::c_long = 445;
const SYS_LANDLOCK_RESTRICT_SELF: libc::c_long = 446;
const CREATE_RULESET_VERSION_FLAG: u32 = 1;
const RULE_PATH_BENEATH: libc::c_long = 1;
const NO_FLAGS: libc::c_long = 0;

const FS_EXECUTE: u64 = 1 << 0;
const FS_WRITE_FILE: u64 = 1 << 1;
const FS_READ_FILE: u64 = 1 << 2;
const FS_READ_DIR: u64 = 1 << 3;
const FS_REMOVE_DIR: u64 = 1 << 4;
const FS_REMOVE_FILE: u64 = 1 << 5;
const FS_MAKE_CHAR: u64 = 1 << 6;
const FS_MAKE_DIR: u64 = 1 << 7;
const FS_MAKE_REG: u64 = 1 << 8;
const FS_MAKE_SOCK: u64 = 1 << 9;
const FS_MAKE_FIFO: u64 = 1 << 10;
const FS_MAKE_BLOCK: u64 = 1 << 11;
const FS_MAKE_SYM: u64 = 1 << 12;
const FS_REFER: u64 = 1 << 13;
const FS_TRUNCATE: u64 = 1 << 14;
const FS_IOCTL_DEV: u64 = 1 << 15;
const NET_BIND_TCP: u64 = 1 << 0;
const NET_CONNECT_TCP: u64 = 1 << 1;

const ABI_WITH_REFER: u32 = 2;
const ABI_WITH_TRUNCATE: u32 = 3;
const ABI_WITH_IOCTL_DEV: u32 = 5;
const FIRST_ABI: u32 = 1;

/// Droits présents dès la première version de Landlock.
const FS_FIRST_ABI_ACCESS: u64 = FS_EXECUTE
    | FS_WRITE_FILE
    | FS_READ_FILE
    | FS_READ_DIR
    | FS_REMOVE_DIR
    | FS_REMOVE_FILE
    | FS_MAKE_CHAR
    | FS_MAKE_DIR
    | FS_MAKE_REG
    | FS_MAKE_SOCK
    | FS_MAKE_FIFO
    | FS_MAKE_BLOCK
    | FS_MAKE_SYM;
const NETWORK_ACCESS: u64 = NET_BIND_TCP | NET_CONNECT_TCP;

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
}

#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PathAccess {
    Read,
    Write,
}

/// Version de Landlock de ce noyau, 0 s'il est absent ou désactivé.
pub fn abi() -> u32 {
    let no_attribute: *const RulesetAttr = std::ptr::null();
    let no_size = libc::c_long::from(0_u8);
    let flag = libc::c_long::from(CREATE_RULESET_VERSION_FLAG);
    // SAFETY: avec le drapeau de version, l'appel ne lit ni l'attribut (nul) ni sa taille (nulle) et ne
    // crée aucun descripteur ; il rend la version ou -1.
    let version = unsafe {
        libc::syscall(SYS_LANDLOCK_CREATE_RULESET, no_attribute, no_size, flag)
    };
    u32::try_from(version).unwrap_or(0)
}

/// Droits sur les fichiers que le profil gère à cette version : ce qui n'est pas accordé est refusé.
fn handled_fs_access(abi: u32) -> u64 {
    let mut access = FS_FIRST_ABI_ACCESS;
    if abi >= ABI_WITH_REFER {
        access |= FS_REFER;
    }
    if abi >= ABI_WITH_TRUNCATE {
        access |= FS_TRUNCATE;
    }
    if abi >= ABI_WITH_IOCTL_DEV {
        access |= FS_IOCTL_DEV;
    }
    access
}

/// Droits accordés sur un chemin : lecture et exécution, ou lecture et écriture. Un fichier ne reçoit que
/// des droits de fichier ; ce que la version du noyau ne gère pas est retiré.
fn path_rights(access: PathAccess, is_dir: bool, handled: u64) -> u64 {
    let rights = match (access, is_dir) {
        (PathAccess::Read, true) => FS_EXECUTE | FS_READ_FILE | FS_READ_DIR,
        (PathAccess::Read, false) => FS_EXECUTE | FS_READ_FILE,
        (PathAccess::Write, true) => {
            handled & !(FS_EXECUTE | FS_MAKE_CHAR | FS_MAKE_BLOCK | FS_IOCTL_DEV)
        }
        (PathAccess::Write, false) => FS_READ_FILE | FS_WRITE_FILE | FS_TRUNCATE,
    };
    rights & handled
}

/// Droits sur le réseau que le profil gère : tout TCP est refusé sauf demande contraire.
fn handled_network_access(profile: &ConfinementProfile, abi: u32) -> Result<u64, ConfineError> {
    if profile.network == NetworkAccess::Any {
        return Ok(0);
    }
    if abi >= LANDLOCK_ABI_WITH_NETWORK {
        return Ok(NETWORK_ACCESS);
    }
    unsupported(profile.enforcement, "le filtrage du réseau (Landlock 4)")?;
    Ok(0)
}

/// Taille de l'attribut de création : celle qu'une version plus ancienne du noyau connaît.
fn ruleset_attr_size(abi: u32) -> usize {
    if abi >= LANDLOCK_ABI_WITH_NETWORK {
        std::mem::size_of::<RulesetAttr>()
    } else {
        std::mem::size_of::<u64>()
    }
}

fn create_ruleset(handled_fs: u64, handled_net: u64, abi: u32) -> Result<OwnedFd, ConfineError> {
    let attr = RulesetAttr {
        handled_access_fs: handled_fs,
        handled_access_net: handled_net,
    };
    let size = ruleset_attr_size(abi);
    let flags = NO_FLAGS;
    // SAFETY: `attr` est une structure `repr(C)` vivante pendant l'appel et `size` n'en dépasse pas la
    // taille ; l'appel rend un descripteur neuf ou -1.
    let fd = unsafe { libc::syscall(SYS_LANDLOCK_CREATE_RULESET, &raw const attr, size, flags) };
    let fd = i32::try_from(fd).map_err(|_| last_os_error("landlock_create_ruleset"))?;
    if fd < 0 {
        return Err(last_os_error("landlock_create_ruleset"));
    }
    // SAFETY: `fd` vient d'être créé par le noyau et n'a pas d'autre propriétaire.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// Ouvre un chemin sans l'ouvrir en lecture (`O_PATH`) ; `None` s'il n'existe pas : il n'y a alors rien
/// à autoriser.
fn open_path(path: &Path) -> Result<Option<File>, ConfineError> {
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| ConfineError::InvalidPath(path.to_path_buf()))?;
    // SAFETY: `c_path` est une chaîne C valide pendant l'appel.
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    if fd >= 0 {
        // SAFETY: `fd` vient d'être ouvert et n'a pas d'autre propriétaire.
        return Ok(Some(File::from(unsafe { OwnedFd::from_raw_fd(fd) })));
    }
    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::NotFound {
        return Ok(None);
    }
    Err(ConfineError::Syscall {
        operation: "ouverture d'un chemin du profil",
        source: error,
    })
}

fn add_path_rule(
    ruleset: &OwnedFd,
    path: &Path,
    access: PathAccess,
    handled: u64,
) -> Result<(), ConfineError> {
    let Some(target) = open_path(path)? else {
        return Ok(());
    };
    let is_dir = target
        .metadata()
        .map_err(|source| ConfineError::Syscall {
            operation: "lecture d'un chemin du profil",
            source,
        })?
        .is_dir();
    let attr = PathBeneathAttr {
        allowed_access: path_rights(access, is_dir, handled),
        parent_fd: target.as_raw_fd(),
    };
    // SAFETY: `attr` est une structure `repr(C)` vivante pendant l'appel, `ruleset` et `target` sont des
    // descripteurs ouverts.
    let result = unsafe {
        libc::syscall(
            SYS_LANDLOCK_ADD_RULE,
            libc::c_long::from(ruleset.as_raw_fd()),
            RULE_PATH_BENEATH,
            &raw const attr,
            NO_FLAGS,
        )
    };
    if result < 0 {
        return Err(last_os_error("landlock_add_rule"));
    }
    Ok(())
}

/// Restreint le processus courant (irréversible, héritée par ses enfants) aux chemins et au réseau du
/// profil. `PR_SET_NO_NEW_PRIVS` doit déjà être posé.
pub fn restrict(profile: &ConfinementProfile, abi: u32) -> Result<(), ConfineError> {
    if abi < FIRST_ABI {
        return unsupported(profile.enforcement, "Landlock");
    }
    let handled_fs = handled_fs_access(abi);
    let handled_net = handled_network_access(profile, abi)?;
    let ruleset = create_ruleset(handled_fs, handled_net, abi)?;
    let rules = [
        (PathAccess::Read, &profile.read_paths),
        (PathAccess::Write, &profile.write_paths),
    ];
    for (access, paths) in rules {
        for path in paths {
            add_path_rule(&ruleset, path, access, handled_fs)?;
        }
    }
    // SAFETY: `ruleset` est un descripteur Landlock ouvert ; l'appel ne lit aucune mémoire.
    let result = unsafe {
        libc::syscall(
            SYS_LANDLOCK_RESTRICT_SELF,
            libc::c_long::from(ruleset.as_raw_fd()),
            NO_FLAGS,
        )
    };
    if result < 0 {
        return Err(last_os_error("landlock_restrict_self"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::confinement::Enforcement;

    #[test]
    fn the_first_version_handles_only_the_original_rights() {
        let access = handled_fs_access(FIRST_ABI);
        assert_eq!(access, FS_FIRST_ABI_ACCESS);
        assert_eq!(access & (FS_REFER | FS_TRUNCATE | FS_IOCTL_DEV), 0);
    }

    #[test]
    fn newer_versions_add_their_rights() {
        assert_ne!(handled_fs_access(ABI_WITH_REFER) & FS_REFER, 0);
        assert_eq!(handled_fs_access(ABI_WITH_REFER) & FS_TRUNCATE, 0);
        assert_ne!(handled_fs_access(ABI_WITH_TRUNCATE) & FS_TRUNCATE, 0);
        assert_eq!(handled_fs_access(ABI_WITH_TRUNCATE) & FS_IOCTL_DEV, 0);
        assert_ne!(handled_fs_access(ABI_WITH_IOCTL_DEV) & FS_IOCTL_DEV, 0);
    }

    #[test]
    fn a_read_only_directory_can_be_read_and_run_but_not_written() {
        let handled = handled_fs_access(ABI_WITH_IOCTL_DEV);
        let rights = path_rights(PathAccess::Read, true, handled);
        assert_eq!(rights, FS_EXECUTE | FS_READ_FILE | FS_READ_DIR);
        assert_eq!(rights & (FS_WRITE_FILE | FS_MAKE_REG | FS_REMOVE_FILE), 0);
    }

    #[test]
    fn a_read_only_file_gets_no_directory_right() {
        let rights = path_rights(PathAccess::Read, false, handled_fs_access(FIRST_ABI));
        assert_eq!(rights, FS_EXECUTE | FS_READ_FILE);
    }

    #[test]
    fn a_writable_directory_can_create_and_truncate_but_not_make_devices() {
        let handled = handled_fs_access(ABI_WITH_IOCTL_DEV);
        let rights = path_rights(PathAccess::Write, true, handled);
        let needed = FS_WRITE_FILE | FS_MAKE_REG | FS_MAKE_DIR | FS_REMOVE_FILE | FS_TRUNCATE;
        assert_eq!(rights & needed, needed);
        assert_eq!(rights & (FS_EXECUTE | FS_MAKE_CHAR | FS_MAKE_BLOCK | FS_IOCTL_DEV), 0);
    }

    #[test]
    fn a_writable_file_gets_only_file_rights_the_kernel_handles() {
        let old = path_rights(PathAccess::Write, false, handled_fs_access(FIRST_ABI));
        assert_eq!(old, FS_READ_FILE | FS_WRITE_FILE);
        let recent = path_rights(PathAccess::Write, false, handled_fs_access(ABI_WITH_TRUNCATE));
        assert_eq!(recent, FS_READ_FILE | FS_WRITE_FILE | FS_TRUNCATE);
    }

    #[test]
    fn network_is_filtered_unless_the_profile_allows_it() -> Result<(), ConfineError> {
        let denying = ConfinementProfile::default();
        let allowing = ConfinementProfile::default().with_network(NetworkAccess::Any);
        assert_eq!(handled_network_access(&denying, LANDLOCK_ABI_WITH_NETWORK)?, NETWORK_ACCESS);
        assert_eq!(handled_network_access(&allowing, LANDLOCK_ABI_WITH_NETWORK)?, 0);
        assert_eq!(handled_network_access(&allowing, FIRST_ABI)?, 0);
        Ok(())
    }

    #[test]
    fn an_old_kernel_follows_the_enforcement_for_the_network() -> Result<(), ConfineError> {
        let required = ConfinementProfile::default();
        let result = handled_network_access(&required, ABI_WITH_TRUNCATE);
        assert!(matches!(result, Err(ConfineError::Unsupported(_))));
        let best_effort = required.with_enforcement(Enforcement::BestEffort);
        assert_eq!(handled_network_access(&best_effort, ABI_WITH_TRUNCATE)?, 0);
        Ok(())
    }

    #[test]
    fn the_attribute_size_is_the_one_an_old_kernel_knows() {
        assert_eq!(ruleset_attr_size(FIRST_ABI), 8);
        assert_eq!(ruleset_attr_size(LANDLOCK_ABI_WITH_NETWORK), 16);
    }

    #[test]
    fn without_landlock_the_restriction_follows_the_enforcement() {
        let required = ConfinementProfile::default();
        assert!(matches!(restrict(&required, 0), Err(ConfineError::Unsupported(_))));
        let best_effort = required.with_enforcement(Enforcement::BestEffort);
        assert!(restrict(&best_effort, 0).is_ok());
    }

    #[test]
    fn a_missing_path_is_skipped_and_an_existing_one_is_opened() -> anyhow::Result<()> {
        assert!(open_path(Path::new("/nonexistent/forgearr-test"))?.is_none());
        assert!(open_path(Path::new("/"))?.is_some());
        Ok(())
    }

    #[test]
    fn a_path_with_a_nul_byte_is_refused() {
        let path = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/a\0b"));
        assert!(matches!(open_path(path), Err(ConfineError::InvalidPath(_))));
    }
}

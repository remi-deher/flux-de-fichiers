//! Filtre seccomp écrit à la main : une liste d'appels système refusés (ADR 0034, point 4).
//!
//! Le filtre refuse (`EPERM`) les appels qu'aucun script ni outil de média n'utilise légitimement : monter
//! un système de fichiers, tracer un autre processus, charger du code dans le noyau, changer d'espace de
//! noms. Un processus qui change d'architecture d'appels est tué.

use super::{ConfineError, last_os_error, unsupported};
use forgearr_shared::confinement::Enforcement;

const BPF_LOAD_WORD_ABSOLUTE: u16 = 0x20;
const BPF_JUMP_IF_EQUAL: u16 = 0x15;
const BPF_RETURN: u16 = 0x06;
/// Position du numéro d'appel et de l'architecture dans `struct seccomp_data`.
const SECCOMP_DATA_NR_OFFSET: u32 = 0;
const SECCOMP_DATA_ARCH_OFFSET: u32 = 4;
const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
const SECCOMP_MODE_FILTER: libc::c_ulong = 2;
const EPERM_CODE: u32 = 1;
const UNUSED_ARGUMENT: libc::c_ulong = 0;
const NO_JUMP: u8 = 0;
const SKIP_ONE_INSTRUCTION: u8 = 1;

#[cfg(target_arch = "x86_64")]
const AUDIT_ARCH: u32 = 0xC000_003E;
#[cfg(target_arch = "aarch64")]
const AUDIT_ARCH: u32 = 0xC000_00B7;

/// Appels système refusés à tout processus confiné.
fn denied_syscalls() -> Vec<libc::c_long> {
    vec![
        libc::SYS_mount,
        libc::SYS_umount2,
        libc::SYS_pivot_root,
        libc::SYS_ptrace,
        libc::SYS_process_vm_readv,
        libc::SYS_process_vm_writev,
        libc::SYS_bpf,
        libc::SYS_perf_event_open,
        libc::SYS_userfaultfd,
        libc::SYS_keyctl,
        libc::SYS_add_key,
        libc::SYS_request_key,
        libc::SYS_init_module,
        libc::SYS_finit_module,
        libc::SYS_delete_module,
        libc::SYS_kexec_load,
        libc::SYS_reboot,
        libc::SYS_swapon,
        libc::SYS_swapoff,
        libc::SYS_setns,
        libc::SYS_unshare,
        libc::SYS_open_by_handle_at,
    ]
}

fn statement(code: u16, argument: u32) -> libc::sock_filter {
    libc::sock_filter {
        code,
        jt: NO_JUMP,
        jf: NO_JUMP,
        k: argument,
    }
}

fn jump_if_equal(expected: u32, if_true: u8, if_false: u8) -> libc::sock_filter {
    libc::sock_filter {
        code: BPF_JUMP_IF_EQUAL,
        jt: if_true,
        jf: if_false,
        k: expected,
    }
}

/// Programme BPF : tuer si l'architecture n'est pas `audit_arch`, refuser chaque appel de `denied`, laisser
/// passer le reste.
fn build_filter(
    audit_arch: u32,
    denied: &[libc::c_long],
) -> Result<Vec<libc::sock_filter>, ConfineError> {
    let mut program = vec![
        statement(BPF_LOAD_WORD_ABSOLUTE, SECCOMP_DATA_ARCH_OFFSET),
        jump_if_equal(audit_arch, SKIP_ONE_INSTRUCTION, NO_JUMP),
        statement(BPF_RETURN, SECCOMP_RET_KILL_PROCESS),
        statement(BPF_LOAD_WORD_ABSOLUTE, SECCOMP_DATA_NR_OFFSET),
    ];
    for syscall in denied {
        let number = u32::try_from(*syscall)
            .map_err(|_| ConfineError::Unsupported("un numéro d'appel système invalide"))?;
        program.push(jump_if_equal(number, NO_JUMP, SKIP_ONE_INSTRUCTION));
        program.push(statement(BPF_RETURN, SECCOMP_RET_ERRNO | EPERM_CODE));
    }
    program.push(statement(BPF_RETURN, SECCOMP_RET_ALLOW));
    Ok(program)
}

/// Seccomp est-il disponible dans ce noyau et ce conteneur ?
pub fn available() -> bool {
    // SAFETY: `PR_GET_SECCOMP` ne lit ni n'écrit aucune mémoire ; il rend le mode courant ou -1.
    unsafe { libc::prctl(libc::PR_GET_SECCOMP) >= 0 }
}

/// Installe le filtre sur le processus courant (irréversible, hérité par ses enfants).
/// `PR_SET_NO_NEW_PRIVS` doit déjà être posé.
pub fn install(enforcement: Enforcement) -> Result<(), ConfineError> {
    if !available() {
        return unsupported(enforcement, "seccomp");
    }
    let filters = build_filter(AUDIT_ARCH, &denied_syscalls())?;
    let length = u16::try_from(filters.len())
        .map_err(|_| ConfineError::Unsupported("un filtre seccomp de cette taille"))?;
    let program = libc::sock_fprog {
        len: length,
        filter: filters.as_ptr().cast_mut(),
    };
    // SAFETY: `program` et le tableau `filters` qu'il désigne vivent pendant l'appel, qui copie le filtre.
    let result = unsafe {
        libc::prctl(
            libc::PR_SET_SECCOMP,
            SECCOMP_MODE_FILTER,
            &raw const program,
            UNUSED_ARGUMENT,
            UNUSED_ARGUMENT,
        )
    };
    if result != 0 {
        return Err(last_os_error("seccomp"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXED_INSTRUCTIONS: usize = 5;
    const INSTRUCTIONS_PER_DENIED_CALL: usize = 2;

    #[test]
    fn the_filter_starts_by_checking_the_architecture() -> Result<(), ConfineError> {
        let filter = build_filter(AUDIT_ARCH, &[])?;
        assert_eq!(filter[0].code, BPF_LOAD_WORD_ABSOLUTE);
        assert_eq!(filter[0].k, SECCOMP_DATA_ARCH_OFFSET);
        assert_eq!((filter[1].code, filter[1].k), (BPF_JUMP_IF_EQUAL, AUDIT_ARCH));
        assert_eq!((filter[2].code, filter[2].k), (BPF_RETURN, SECCOMP_RET_KILL_PROCESS));
        Ok(())
    }

    #[test]
    fn the_filter_allows_everything_it_does_not_deny() -> Result<(), ConfineError> {
        let filter = build_filter(AUDIT_ARCH, &[])?;
        assert_eq!(filter.len(), FIXED_INSTRUCTIONS);
        let last = filter[filter.len() - 1];
        assert_eq!((last.code, last.k), (BPF_RETURN, SECCOMP_RET_ALLOW));
        Ok(())
    }

    #[test]
    fn each_denied_call_adds_a_comparison_and_an_error_return() -> Result<(), ConfineError> {
        let denied = [libc::SYS_mount, libc::SYS_ptrace];
        let filter = build_filter(AUDIT_ARCH, &denied)?;
        assert_eq!(filter.len(), FIXED_INSTRUCTIONS + denied.len() * INSTRUCTIONS_PER_DENIED_CALL);
        let mount = u32::try_from(libc::SYS_mount).unwrap_or(u32::MAX);
        assert_eq!((filter[4].code, filter[4].k), (BPF_JUMP_IF_EQUAL, mount));
        assert_eq!((filter[4].jt, filter[4].jf), (NO_JUMP, SKIP_ONE_INSTRUCTION));
        assert_eq!(filter[5].k, SECCOMP_RET_ERRNO | EPERM_CODE);
        Ok(())
    }

    #[test]
    fn a_negative_syscall_number_is_refused() {
        let result = build_filter(AUDIT_ARCH, &[-1]);
        assert!(matches!(result, Err(ConfineError::Unsupported(_))));
    }

    #[test]
    fn the_denied_list_has_no_duplicate() {
        let mut denied = denied_syscalls();
        let count = denied.len();
        denied.sort_unstable();
        denied.dedup();
        assert_eq!(denied.len(), count);
    }

    #[test]
    fn the_real_denied_list_fits_in_a_filter() -> Result<(), ConfineError> {
        let filter = build_filter(AUDIT_ARCH, &denied_syscalls())?;
        assert!(u16::try_from(filter.len()).is_ok());
        Ok(())
    }

    #[test]
    fn without_seccomp_the_installation_follows_the_enforcement() {
        if available() {
            return;
        }
        let required = install(Enforcement::Require);
        assert!(matches!(required, Err(ConfineError::Unsupported(_))));
        assert!(install(Enforcement::BestEffort).is_ok());
    }
}

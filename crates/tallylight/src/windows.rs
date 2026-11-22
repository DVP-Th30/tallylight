//! Ce que Windows demande en propre : savoir si un processus existe, faire tourner le démon sans
//! fenêtre, et activer les séquences d'écran dans l'ancienne console.

use std::{os::windows::io::IntoRawHandle, path::Path};

use anyhow::{Context, Result};
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_ACCESS_DENIED, GetLastError, STILL_ACTIVE},
    System::{
        Console::{
            ENABLE_VIRTUAL_TERMINAL_PROCESSING, FreeConsole, GetConsoleMode, GetStdHandle,
            STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetConsoleMode, SetStdHandle,
        },
        Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

/// Taille au-delà de laquelle le journal du démon repart de zéro à son lancement.
const LOG_MAX: u64 = 1024 * 1024;

/// Vrai si le processus existe (même s'il appartient à un autre utilisateur).
pub fn process_alive(pid: u32) -> bool {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            // Processus d'un autre utilisateur : il existe, sans qu'on puisse l'interroger
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        let mut code = 0u32;
        let known = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        // Un processus terminé reste ouvrable tant que quelqu'un garde une poignée sur lui
        !known || code == STILL_ACTIVE as u32
    }
}

/// Détache le démon de sa console et envoie ce qu'il écrit dans `log`. La tâche planifiée lance un
/// programme de console : sans cela, une fenêtre resterait ouverte tant que le démon tourne.
pub fn detach_console(log: &Path) -> Result<()> {
    if std::fs::metadata(log).is_ok_and(|meta| meta.len() > LOG_MAX) {
        let _ = std::fs::remove_file(log);
    }
    let output = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .with_context(|| format!("could not open {}", log.display()))?;
    let errors = output.try_clone()?;
    unsafe {
        // Les poignées restent ouvertes jusqu'à la fin du processus : la bibliothèque standard
        // relit la sortie courante à chaque écriture
        SetStdHandle(STD_OUTPUT_HANDLE, output.into_raw_handle());
        SetStdHandle(STD_ERROR_HANDLE, errors.into_raw_handle());
        FreeConsole();
    }
    Ok(())
}

/// Fait comprendre à la console les séquences d'effacement d'écran de `tallylight watch`
/// (l'ancienne console de Windows les affiche telles quelles sans ce réglage).
pub fn enable_ansi() {
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode = 0;
        if GetConsoleMode(handle, &mut mode) != 0 {
            SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_process_is_alive_and_a_finished_one_is_not() {
        assert!(process_alive(std::process::id()));
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "exit 0"])
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        // `child` garde une poignée : le processus terminé reste ouvrable, mais n'est plus actif
        assert!(!process_alive(pid));
        assert!(!process_alive(0));
    }
}

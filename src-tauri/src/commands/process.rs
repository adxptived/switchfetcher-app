//! Process detection commands

use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Information about running Codex processes
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodexProcessInfo {
    /// Number of running codex processes
    pub count: usize,
    /// Number of background IDE/extension codex processes (like Antigravity)
    pub background_count: usize,
    /// Whether switching is allowed (no processes running)
    pub can_switch: bool,
    /// Process IDs of running codex processes
    pub pids: Vec<u32>,
}

/// Information about running Claude processes
#[derive(Debug, Clone, serde::Serialize)]
pub struct ClaudeProcessInfo {
    /// Number of running claude processes
    pub count: usize,
    /// Number of background claude processes
    pub background_count: usize,
    /// Whether switching is allowed (no processes running)
    pub can_switch: bool,
    /// Process IDs of running claude processes
    pub pids: Vec<u32>,
}

/// Information about running Gemini processes
#[derive(Debug, Clone, serde::Serialize)]
pub struct GeminiProcessInfo {
    /// Number of running gemini processes
    pub count: usize,
    /// Number of background gemini processes
    pub background_count: usize,
    /// Whether switching is allowed (no processes running)
    pub can_switch: bool,
    /// Process IDs of running gemini processes
    pub pids: Vec<u32>,
}

/// Check for running Codex processes
#[tauri::command]
pub async fn check_codex_processes() -> Result<CodexProcessInfo, String> {
    let (pids, bg_count) = find_codex_processes().map_err(|e| e.to_string())?;
    let count = pids.len();

    Ok(CodexProcessInfo {
        count,
        background_count: bg_count,
        can_switch: count == 0,
        pids,
    })
}

/// Check for running Claude processes
#[tauri::command]
pub async fn check_claude_processes() -> Result<ClaudeProcessInfo, String> {
    let pids = find_named_processes("claude").map_err(|e| e.to_string())?;
    let count = pids.len();

    Ok(ClaudeProcessInfo {
        count,
        background_count: 0,
        can_switch: count == 0,
        pids,
    })
}

/// Check for running Gemini processes
#[tauri::command]
pub async fn check_gemini_processes() -> Result<GeminiProcessInfo, String> {
    let pids = find_named_processes("gemini").map_err(|e| e.to_string())?;
    let count = pids.len();

    Ok(GeminiProcessInfo {
        count,
        background_count: 0,
        can_switch: count == 0,
        pids,
    })
}

/// Find all running codex processes. Returns (active_pids, background_count)
fn find_codex_processes() -> anyhow::Result<(Vec<u32>, usize)> {
    let mut pids = Vec::new();
    #[allow(unused_mut)]
    let mut bg_count = 0;

    #[cfg(unix)]
    {
        // Use ps with custom format to get the pid and full command line
        let output = Command::new("ps").args(["-eo", "pid,command"]).output();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines().skip(1) {
                // Skip header
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                // The first part is PID, the rest is the command string
                if let Some((pid_str, command)) = line.split_once(' ') {
                    let command = command.trim();

                    // Get the executable path/name (first word of the command string before args)
                    let executable = command.split_whitespace().next().unwrap_or("");

                    // Check if the executable is exactly "codex" or ends with "/codex"
                    let is_codex = executable == "codex" || executable.ends_with("/codex");

                    // Exclude if it's running from an extension or IDE integration (like Antigravity)
                    // These are expected background processes we shouldn't block on
                    let is_ide_plugin = command.contains(".antigravity")
                        || command.contains("openai.chatgpt")
                        || command.contains(".vscode");

                    // Skip our own app
                    let is_switcher = command.contains("switchfetcher")
                        || command.contains("Switchfetcher");

                    if is_codex && !is_switcher {
                        if let Ok(pid) = pid_str.trim().parse::<u32>() {
                            if pid != std::process::id() && !pids.contains(&pid) {
                                if is_ide_plugin {
                                    bg_count += 1;
                                } else {
                                    pids.push(pid);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    {
        if let Some((detected_pids, detected_bg_count)) = find_codex_processes_with_command_lines()? {
            pids = detected_pids;
            bg_count = detected_bg_count;
        } else {
            let output = Command::new("tasklist")
                .creation_flags(CREATE_NO_WINDOW)
                .args(["/FI", "IMAGENAME eq codex.exe", "/FO", "CSV", "/NH"])
                .output();

            if let Ok(output) = output {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() > 1 {
                        let name = parts[0].trim_matches('"').to_lowercase();
                        if name == "codex.exe" {
                            let pid_str = parts[1].trim_matches('"');
                            if let Ok(pid) = pid_str.parse::<u32>() {
                                if pid != std::process::id() {
                                    pids.push(pid);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok((pids, bg_count))
}

#[cfg(windows)]
fn find_codex_processes_with_command_lines() -> anyhow::Result<Option<(Vec<u32>, usize)>> {
    let output = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance Win32_Process -Filter \"name = 'codex.exe'\" | ForEach-Object { \"$($_.ProcessId)`t$($_.CommandLine)\" }",
        ])
        .output();

    let Ok(output) = output else {
        return Ok(None);
    };

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut pids = Vec::new();
    let mut bg_count = 0usize;

    for line in stdout.lines() {
        let Some((pid_str, command)) = line.split_once('\t') else {
            continue;
        };
        let Ok(pid) = pid_str.trim().parse::<u32>() else {
            continue;
        };
        if pid == std::process::id() {
            continue;
        }

        if is_ide_plugin_command(command) {
            bg_count += 1;
        } else if !pids.contains(&pid) {
            pids.push(pid);
        }
    }

    Ok(Some((pids, bg_count)))
}

fn is_ide_plugin_command(command: &str) -> bool {
    command.contains(".antigravity")
        || command.contains("openai.chatgpt")
        || command.contains(".vscode")
}

fn find_named_processes(process_name: &str) -> anyhow::Result<Vec<u32>> {
    #[cfg(unix)]
    {
        return find_named_processes_unix(process_name);
    }

    #[cfg(windows)]
    {
        return find_named_processes_windows(process_name);
    }

    #[allow(unreachable_code)]
    Ok(Vec::new())
}

#[cfg(unix)]
fn find_named_processes_unix(process_name: &str) -> anyhow::Result<Vec<u32>> {
    let output = Command::new("ps").args(["-eo", "pid,comm"]).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_name = process_name.to_ascii_lowercase();
    let mut pids = Vec::new();

    for line in stdout.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut parts = line.split_whitespace();
        let Some(pid_str) = parts.next() else {
            continue;
        };
        let Some(command_name) = parts.next() else {
            continue;
        };
        let command_name = command_name.rsplit('/').next().unwrap_or(command_name);

        if command_name.eq_ignore_ascii_case(&expected_name) {
            let Ok(pid) = pid_str.parse::<u32>() else {
                continue;
            };
            if pid != std::process::id() && !pids.contains(&pid) {
                pids.push(pid);
            }
        }
    }

    Ok(pids)
}

#[cfg(windows)]
fn find_named_processes_windows(process_name: &str) -> anyhow::Result<Vec<u32>> {
    let image_name = format!("{process_name}.exe");
    let output = Command::new("tasklist")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["/FI", &format!("IMAGENAME eq {image_name}"), "/FO", "CSV", "/NH"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_tasklist_named_processes(
        &stdout,
        &image_name,
        std::process::id(),
    ))
}

#[cfg(windows)]
fn parse_tasklist_named_processes(stdout: &str, image_name: &str, current_pid: u32) -> Vec<u32> {
    let expected_name = image_name.to_ascii_lowercase();
    let mut pids = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() <= 1 {
            continue;
        }

        let name = parts[0].trim().trim_matches('"').to_ascii_lowercase();
        if name != expected_name {
            continue;
        }

        let Ok(pid) = parts[1].trim().trim_matches('"').parse::<u32>() else {
            continue;
        };
        if pid != current_pid && !pids.contains(&pid) {
            pids.push(pid);
        }
    }

    pids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn parses_tasklist_named_process_output() {
        let current_pid = std::process::id();
        let output = format!(
            "\"codex.exe\",\"123\",\"Console\",\"1\",\"10,000 K\"\n\
             \"CODEX.EXE\",\"123\",\"Console\",\"1\",\"10,000 K\"\n\
             \"codex.exe\",\"{current_pid}\",\"Console\",\"1\",\"10,000 K\"\n\
             \"pwsh.exe\",\"456\",\"Console\",\"1\",\"10,000 K\"\n"
        );

        let pids = parse_tasklist_named_processes(&output, "codex.exe", current_pid);

        assert_eq!(pids, vec![123]);
    }
}

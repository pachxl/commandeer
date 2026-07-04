use serde::Serialize;
use std::fs;
use std::path::Path;

#[cfg(target_os = "windows")]
use serde::Deserialize;
#[cfg(target_os = "windows")]
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct ScriptInfo {
    pub name: String,
    pub path: String,
    pub ext: String,
    pub icon: Option<String>,
    pub folder: Option<String>,
    pub is_folder: bool,
}

/// Whether a directory entry should be surfaced as a runnable command.
///
/// Windows: batch/command scripts, shell shortcuts, and VS Code workspaces.
/// macOS/Linux: shell scripts, VS Code workspaces, or any regular file with the
/// executable bit set. Linux additionally surfaces `.desktop` launchers and
/// AppImages; macOS additionally surfaces `.command` Terminal scripts.
fn is_script_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|x| x.to_str());

    #[cfg(target_os = "windows")]
    {
        matches!(
            ext,
            Some("bat") | Some("cmd") | Some("lnk") | Some("code-workspace")
        )
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        #[cfg(target_os = "linux")]
        if matches!(
            ext,
            Some("sh") | Some("desktop") | Some("code-workspace") | Some("AppImage")
        ) {
            return true;
        }
        #[cfg(target_os = "macos")]
        if matches!(ext, Some("sh") | Some("command") | Some("code-workspace")) {
            return true;
        }
        is_executable(path)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

// .desktop parsing/icon helpers live in the shared desktop module.
#[cfg(target_os = "linux")]
use super::desktop::{resolve_desktop_icon, resolve_desktop_name};

fn collect_script_files(dir: &Path, folder: Option<String>) -> Vec<ScriptInfo> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    entries
        .filter_map(|e| e.ok())
        .filter(|e| is_script_file(&e.path()))
        .filter_map(|e| {
            let path = e.path();
            let ext = path
                .extension()
                .map(|x| x.to_string_lossy().into_owned())
                .unwrap_or_default();
            let stem = path.file_stem()?.to_string_lossy().into_owned();

            // Linux: a .desktop entry's Name= is friendlier than its filename.
            #[cfg(target_os = "linux")]
            let stem = if ext == "desktop" {
                resolve_desktop_name(&path).unwrap_or(stem)
            } else {
                stem
            };

            let path_str = path.to_string_lossy().replace('\\', "/");

            // Use <stem>.png if it exists; for .code-workspace fall back to vscode.png
            let stem_png = path.with_extension("png");
            let png_path = if ext == "code-workspace" && !stem_png.exists() {
                path.parent().map(|p| p.join("vscode.png")).unwrap_or(stem_png)
            } else {
                stem_png
            };
            let icon = if png_path.exists() {
                fs::read(&png_path).ok().map(|bytes| {
                    format!("data:image/png;base64,{}", base64_encode(&bytes))
                })
            } else {
                None
            };

            // Linux: fall back to the icon declared inside a .desktop entry.
            #[cfg(target_os = "linux")]
            let icon = icon.or_else(|| {
                if ext == "desktop" {
                    resolve_desktop_icon(&path)
                } else {
                    None
                }
            });

            Some(ScriptInfo { name: stem, path: path_str, ext, icon, folder: folder.clone(), is_folder: false })
        })
        .collect()
}

#[tauri::command]
pub async fn list_scripts(scripts_dir: String) -> Result<Vec<ScriptInfo>, String> {
    if scripts_dir.is_empty() {
        return Ok(vec![]);
    }

    let dir = Path::new(&scripts_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut scripts: Vec<ScriptInfo> = Vec::new();

    // Root-level scripts
    scripts.extend(collect_script_files(dir, None));

    // Subdirectories: folder entry + their scripts
    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let folder_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Folder icon: look for <FolderName>.png alongside the folder in scripts_dir
        let folder_png = dir.join(format!("{}.png", folder_name));
        let folder_icon = if folder_png.exists() {
            fs::read(&folder_png).ok().map(|bytes| {
                format!("data:image/png;base64,{}", base64_encode(&bytes))
            })
        } else {
            None
        };

        scripts.push(ScriptInfo {
            name: folder_name.clone(),
            path: path.to_string_lossy().replace('\\', "/"),
            ext: String::new(),
            icon: folder_icon,
            folder: None,
            is_folder: true,
        });

        scripts.extend(collect_script_files(&path, Some(folder_name)));
    }

    // Windows: for .lnk files without a PNG icon, extract the shell icon via PowerShell
    #[cfg(target_os = "windows")]
    {
        let lnk_indices: Vec<usize> = scripts
            .iter()
            .enumerate()
            .filter(|(_, s)| s.ext == "lnk" && s.icon.is_none())
            .map(|(i, _)| i)
            .collect();

        if !lnk_indices.is_empty() {
            let win_paths: Vec<String> = lnk_indices
                .iter()
                .map(|&i| scripts[i].path.replace('/', "\\"))
                .collect();

            let icons = extract_lnk_icons(win_paths).await;

            for i in lnk_indices {
                let win_path = scripts[i].path.replace('/', "\\");
                if let Some(icon) = icons.get(&win_path) {
                    scripts[i].icon = Some(icon.clone());
                }
            }
        }
    }

    Ok(scripts)
}

#[cfg(target_os = "windows")]
#[derive(Deserialize)]
struct IconResult {
    path: String,
    icon: Option<String>,
}

#[cfg(target_os = "windows")]
async fn extract_lnk_icons(paths: Vec<String>) -> HashMap<String, String> {
    if paths.is_empty() {
        return HashMap::new();
    }

    // Build a PowerShell array literal with properly escaped paths
    let arr = paths
        .iter()
        .map(|p| format!("'{}'", p.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");

    let ps_script = format!(
        r#"Add-Type -AssemblyName System.Drawing
$shell = New-Object -ComObject WScript.Shell
$paths = @({arr})

function Resolve-IconSource($lnkPath) {{
    try {{
        $sc = $shell.CreateShortcut($lnkPath)
        $iconLoc = $sc.IconLocation
        if ($iconLoc) {{
            $parts = $iconLoc -split ','
            $iconPath = $parts[0].Trim()
            if ($iconPath -and (Test-Path -LiteralPath $iconPath)) {{
                $idx = 0
                if ($parts.Count -gt 1) {{ [void][int]::TryParse($parts[1].Trim(), [ref]$idx) }}
                return @{{ path = $iconPath; index = $idx }}
            }}
        }}
        $tgt = $sc.TargetPath
        if ($tgt -and (Test-Path -LiteralPath $tgt)) {{
            return @{{ path = $tgt; index = 0 }}
        }}
    }} catch {{}}
    return @{{ path = $lnkPath; index = 0 }}
}}

if (-not ([System.Management.Automation.PSTypeName]'Win32IconExtractor').Type) {{
    Add-Type -TypeDefinition @"
using System;
using System.Drawing;
using System.Runtime.InteropServices;
public static class Win32IconExtractor {{
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    public static extern int ExtractIconEx(string lpszFile, int nIconIndex, IntPtr[] phiconLarge, IntPtr[] phiconSmall, int nIcons);
    [DllImport("user32.dll")] public static extern bool DestroyIcon(IntPtr hIcon);
    public static Icon Get(string path, int index) {{
        IntPtr[] large = new IntPtr[1];
        int n = ExtractIconEx(path, index, large, null, 1);
        if (n > 0 && large[0] != IntPtr.Zero) {{
            Icon ico = (Icon)Icon.FromHandle(large[0]).Clone();
            DestroyIcon(large[0]);
            return ico;
        }}
        return null;
    }}
}}
"@ -ReferencedAssemblies System.Drawing
}}

$result = $paths | ForEach-Object {{
    $p = $_
    try {{
        $src = Resolve-IconSource $p
        $ico = [Win32IconExtractor]::Get($src.path, $src.index)
        if ($ico -eq $null) {{
            $ico = [System.Drawing.Icon]::ExtractAssociatedIcon($src.path)
        }}
        $bmp = $ico.ToBitmap()
        $ms  = New-Object System.IO.MemoryStream
        $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
        $bmp.Dispose(); $ico.Dispose()
        [pscustomobject]@{{path=$p; icon=[Convert]::ToBase64String($ms.ToArray())}}
    }} catch {{
        [pscustomobject]@{{path=$p; icon=$null}}
    }}
}}
ConvertTo-Json -InputObject @($result) -Compress"#,
        arr = arr
    );

    let output = tokio::task::spawn_blocking(move || {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("powershell.exe")
            .args(["-NonInteractive", "-Command", &ps_script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
    })
    .await;

    let bytes = match output {
        Ok(Ok(o)) => o.stdout,
        _ => return HashMap::new(),
    };

    let json = String::from_utf8_lossy(&bytes);
    let results: Vec<IconResult> = serde_json::from_str(&json).unwrap_or_default();

    results
        .into_iter()
        .filter_map(|r| r.icon.map(|ico| (r.path, format!("data:image/png;base64,{ico}"))))
        .collect()
}

pub(crate) fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
        out.push(CHARS[b0 >> 2] as char);
        out.push(CHARS[((b0 & 3) << 4) | (b1 >> 4)] as char);
        out.push(if chunk.len() > 1 { CHARS[((b1 & 0xf) << 2) | (b2 >> 6)] as char } else { '=' });
        out.push(if chunk.len() > 2 { CHARS[b2 & 0x3f] as char } else { '=' });
    }
    out
}

const TEXT_PREVIEW_MAX_BYTES: usize = 32_768;
const TEXT_PREVIEW_MAX_LINES: usize = 80;

/// Best-effort plain-text preview of a file. Returns the first few thousand
/// bytes truncated to a safe line count. Binary files are rejected by a simple
/// null-byte check so the preview pane doesn't render garbage.
#[tauri::command]
pub async fn read_text_preview(path: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        if bytes.contains(&0) {
            return Err("Binary file".to_string());
        }
        let mut text = String::from_utf8_lossy(&bytes[..bytes.len().min(TEXT_PREVIEW_MAX_BYTES)]).to_string();
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() > TEXT_PREVIEW_MAX_LINES {
            text = lines[..TEXT_PREVIEW_MAX_LINES].join("\n");
            text.push_str("\n…");
        }
        Ok(text)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn run_script(path: String) -> Result<(), String> {
    let win_path = path.replace('/', "\\");

    tokio::task::spawn_blocking(move || {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let script_path = std::path::Path::new(&win_path);
        let working_dir = script_path.parent().map(|p| p.to_path_buf());

        let ext = script_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let escaped = win_path.replace('\'', "''");

        let ps_cmd = if ext == "code-workspace" {
            // Open directly with VS Code; -WindowStyle Hidden suppresses the code.cmd console window
            format!("Start-Process code -ArgumentList '\"{}\"' -WindowStyle Hidden", escaped)
        } else {
            // Everything else: let Windows shell handle via file association
            format!("Start-Process -FilePath '{}' -WindowStyle Hidden", escaped)
        };

        let mut cmd = std::process::Command::new("powershell.exe");
        cmd.args(["-NonInteractive", "-Command", &ps_cmd])
           .creation_flags(CREATE_NO_WINDOW);

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        cmd.spawn().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
    .map(|_| ())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub async fn run_script(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let script_path = std::path::Path::new(&path);
        let working_dir = script_path.parent().map(|p| p.to_path_buf());

        let ext = script_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        #[cfg(target_os = "linux")]
        if ext == "desktop" {
            // Launch through gio when present, else parse Exec= ourselves.
            return super::desktop::launch_desktop_file(script_path);
        }

        let mut cmd = if ext == "code-workspace" {
            let mut c = std::process::Command::new("code");
            c.arg(&path);
            c
        } else if cfg!(target_os = "macos") && ext == "command" {
            // Open in Terminal via LaunchServices, like double-clicking it —
            // direct exec would run it invisibly. (Checked before the
            // executable-bit test: .command files usually carry it.)
            let mut c = std::process::Command::new("open");
            c.arg(&path);
            c
        } else if is_executable(script_path) {
            // Directly runnable: scripts with a shebang, AppImages, binaries.
            std::process::Command::new(&path)
        } else if ext == "sh" {
            // A shell script without the executable bit set.
            let mut c = std::process::Command::new("sh");
            c.arg(&path);
            c
        } else {
            // Fall back to the desktop's default handler for the file type.
            let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
            let mut c = std::process::Command::new(opener);
            c.arg(&path);
            c
        };

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        cmd.spawn().map_err(|e| e.to_string()).map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reveal a file or folder in the platform file manager with the item
/// selected: Finder on macOS (`open -R`), File Explorer on Windows
/// (`explorer /select,`), and the default manager on Linux via the
/// org.freedesktop.FileManager1 D-Bus interface (falling back to opening the
/// parent folder unselected when no manager implements it).
#[tauri::command]
pub async fn reveal_path(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let p = std::path::Path::new(&path);
        if !p.exists() {
            return Err(format!("Path does not exist: {path}"));
        }

        #[cfg(target_os = "macos")]
        {
            let out = std::process::Command::new("open")
                .args(["-R", &path])
                .output()
                .map_err(|e| format!("open -R failed to run: {e}"))?;
            if out.status.success() {
                Ok(())
            } else {
                Err(format!(
                    "open -R failed: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                ))
            }
        }

        #[cfg(target_os = "windows")]
        {
            // explorer.exe exits 1 even on success, so spawn without checking
            // the status; a spawn error is the only real failure signal.
            std::process::Command::new("explorer")
                .arg(format!("/select,{path}"))
                .spawn()
                .map_err(|e| format!("explorer failed to run: {e}"))?;
            Ok(())
        }

        #[cfg(target_os = "linux")]
        {
            // ShowItems selects the file in whatever manager owns the bus name
            // (Files, Dolphin, Nemo, ...). dbus-send ships with dbus itself, so
            // it's present on any session that has a bus at all.
            let ok = std::process::Command::new("dbus-send")
                .args([
                    "--session",
                    "--print-reply",
                    "--dest=org.freedesktop.FileManager1",
                    "/org/freedesktop/FileManager1",
                    "org.freedesktop.FileManager1.ShowItems",
                    &format!("array:string:{}", file_uri(p)),
                    "string:",
                ])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if ok {
                return Ok(());
            }
            // No FileManager1 implementation: open the containing folder.
            let parent = if p.is_dir() {
                p.to_path_buf()
            } else {
                p.parent()
                    .map(|d| d.to_path_buf())
                    .ok_or_else(|| format!("No parent folder for {path}"))?
            };
            std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map_err(|e| format!("xdg-open failed to run: {e}"))?;
            Ok(())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Percent-encode a filesystem path into a file:// URI (RFC 3986 unreserved
/// characters and `/` pass through; everything else, including UTF-8 bytes,
/// is %XX-encoded). D-Bus ShowItems requires proper URIs.
#[cfg(target_os = "linux")]
fn file_uri(path: &std::path::Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut uri = String::from("file://");
    for &b in path.as_os_str().as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(b as char)
            }
            _ => uri.push_str(&format!("%{b:02X}")),
        }
    }
    uri
}

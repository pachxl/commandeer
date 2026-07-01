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
/// Unix: shell scripts, `.desktop` launchers, VS Code workspaces, AppImages,
/// or any regular file with the executable bit set.
fn is_script_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|x| x.to_str());

    #[cfg(target_os = "windows")]
    {
        matches!(
            ext,
            Some("bat") | Some("cmd") | Some("lnk") | Some("code-workspace")
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        if matches!(
            ext,
            Some("sh") | Some("desktop") | Some("code-workspace") | Some("AppImage")
        ) {
            return true;
        }
        is_executable(path)
    }
}

#[cfg(not(target_os = "windows"))]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

/// Read a single key from the `[Desktop Entry]` group of a .desktop file.
/// Localized variants (e.g. `Name[de]=`) are ignored in favour of the default.
#[cfg(not(target_os = "windows"))]
fn desktop_entry_value(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    let mut in_entry = false;
    for raw in content.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry {
            if let Some(val) = line.strip_prefix(&prefix) {
                return Some(val.trim().to_string());
            }
        }
    }
    None
}

/// Human-friendly name declared inside a .desktop file (`Name=`).
#[cfg(not(target_os = "windows"))]
fn resolve_desktop_name(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let name = desktop_entry_value(&content, "Name")?;
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Resolve the `Icon=` of a .desktop file to a base64 data URL, if findable.
#[cfg(not(target_os = "windows"))]
fn resolve_desktop_icon(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let icon = desktop_entry_value(&content, "Icon")?;
    if icon.is_empty() {
        return None;
    }

    let icon_path = if icon.starts_with('/') {
        let p = std::path::PathBuf::from(&icon);
        if p.is_file() {
            Some(p)
        } else {
            None
        }
    } else {
        find_themed_icon(&icon)
    }?;

    let bytes = fs::read(&icon_path).ok()?;
    let mime = match icon_path.extension().and_then(|e| e.to_str()) {
        Some("svg") => "image/svg+xml",
        _ => "image/png",
    };
    Some(format!("data:{mime};base64,{}", base64_encode(&bytes)))
}

/// Best-effort lookup of a freedesktop icon name across the common theme roots.
#[cfg(not(target_os = "windows"))]
fn find_themed_icon(name: &str) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;

    let mut roots: Vec<String> = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            roots.push(format!("{home}/.local/share/icons/hicolor"));
            roots.push(format!("{home}/.icons/hicolor"));
        }
    }
    roots.push("/usr/share/icons/hicolor".to_string());
    roots.push("/usr/local/share/icons/hicolor".to_string());

    let sizes = [
        "scalable", "512x512", "256x256", "128x128", "96x96", "64x64", "48x48", "32x32", "24x24",
        "16x16",
    ];
    let exts = ["png", "svg"];

    // Flat pixmaps directory (no theme/size structure).
    for ext in exts {
        let p = PathBuf::from(format!("/usr/share/pixmaps/{name}.{ext}"));
        if p.is_file() {
            return Some(p);
        }
    }

    for root in &roots {
        for size in sizes {
            for ext in exts {
                let p = PathBuf::from(format!("{root}/{size}/apps/{name}.{ext}"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }

    None
}

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

            // Unix: a .desktop entry's Name= is friendlier than its filename.
            #[cfg(not(target_os = "windows"))]
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

            // Unix: fall back to the icon declared inside a .desktop entry.
            #[cfg(not(target_os = "windows"))]
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

fn base64_encode(input: &[u8]) -> String {
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

        let mut cmd = if ext == "desktop" {
            // Launch a .desktop entry through the desktop's app launcher.
            let mut c = std::process::Command::new("gio");
            c.arg("launch").arg(&path);
            c
        } else if ext == "code-workspace" {
            let mut c = std::process::Command::new("code");
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
            let mut c = std::process::Command::new("xdg-open");
            c.arg(&path);
            c
        };

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        cmd.spawn().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
    .map(|_| ())
}

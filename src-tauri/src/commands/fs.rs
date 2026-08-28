use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::Path;

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
    /// Raycast/vicinae-style `@raycast.*` / `@vicinae.*` comment directives
    /// parsed from the script header. `None` when the file has no such
    /// metadata (the common case — bare executables and shortcuts).
    pub metadata: Option<ScriptMetadata>,
}

/// One declared script argument (`@vicinae.argument1`/`argument2`/`argument3`).
#[derive(Debug, Serialize, Clone, Default)]
pub struct ScriptArgument {
    /// 1-based position (1..=3).
    pub index: u8,
    /// `text` | `password` | `dropdown`
    pub arg_type: String,
    pub placeholder: Option<String>,
    pub optional: bool,
    /// Dropdown options as `(title, value)` pairs.
    pub data: Vec<(String, String)>,
}

/// Parsed `@raycast.*` / `@vicinae.*` script metadata. Mirrors the Raycast
/// script-command format with the vicinae additions (`keywords`,
/// `refresh_seconds`, per-app scope omitted here). Surfaced to the frontend
/// so scripts can carry a friendly title, description, icon, search keywords,
/// a confirm gate, and (for inline mode) a live refresh interval.
#[derive(Debug, Serialize, Clone, Default)]
pub struct ScriptMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    /// Named icon resolvable by the frontend `Icon` library (e.g. "calendar").
    pub icon_name: Option<String>,
    pub icon_name_dark: Option<String>,
    /// `fullOutput` | `compact` | `inline` | `silent` | `terminal`
    pub mode: Option<String>,
    pub keywords: Vec<String>,
    pub needs_confirmation: bool,
    pub author: Option<String>,
    pub package_name: Option<String>,
    pub current_directory_path: Option<String>,
    /// Parsed `@vicinae.refreshTime` ("5s"/"2m"/"1h"/"1d") → seconds.
    pub refresh_seconds: Option<u64>,
    pub arguments: Vec<ScriptArgument>,
}

/// Whether a directory entry should be surfaced as a runnable command.
///
/// Windows: batch/PowerShell scripts, shell shortcuts, and VS Code workspaces.
/// macOS/Linux: shell scripts, VS Code workspaces, or any regular file with the
/// executable bit set. Linux additionally surfaces `.desktop` launchers and
/// AppImages; macOS additionally surfaces `.command` Terminal scripts.
fn is_script_file(path: &Path) -> bool {
    let ext = path.extension().and_then(|x| x.to_str());

    #[cfg(target_os = "windows")]
    {
        matches!(
            ext,
            Some("bat") | Some("cmd") | Some("ps1") | Some("lnk") | Some("code-workspace")
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

// --- Script metadata parser ------------------------------------------------
//
// Reads the head of a script file and extracts `@raycast.*` / `@vicinae.*`
// comment directives — the Raycast script-command metadata format with the
// vicinae additions (`keywords`, `refreshTime`, `exec`). Any comment marker
// (`//`, `--`, `#`, `;`) is accepted so bash/python/lua/js all work. Binary
// files without recognizable directives yield no metadata.

const METADATA_HEAD_BYTES: usize = 8192;

/// Strip a single pair of surrounding matching quotes from a value, if present.
fn unquote(v: &str) -> &str {
    let v = v.trim();
    let bytes = v.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0] as char;
        let last = bytes[bytes.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return &v[1..v.len() - 1];
        }
    }
    v
}

/// Parse `@vicinae.refreshTime` ("5s"/"2m"/"1h"/"1d") into seconds. A bare
/// number with no unit is rejected (a unit is required to avoid ambiguity).
fn parse_refresh_time(v: &str) -> Option<u64> {
    let v = v.trim();
    let mut chars = v.chars();
    let last = chars.next_back()?;
    if last.is_ascii_digit() {
        return None;
    }
    let n: u64 = chars.as_str().trim().parse().ok()?;
    if n == 0 {
        return None;
    }
    let multiplier = match last {
        's' => 1,
        'm' => 60,
        'h' => 3600,
        'd' => 86400,
        _ => return None,
    };
    n.checked_mul(multiplier)
}

/// Parse one `@raycast.argumentN` / `@vicinae.argumentN` JSON value.
fn parse_argument(json: &str, index: u8) -> Option<ScriptArgument> {
    #[derive(Deserialize)]
    struct ArgOption {
        title: String,
        value: String,
    }
    #[derive(Deserialize)]
    struct ArgJson {
        #[serde(rename = "type")]
        arg_type: Option<String>,
        placeholder: Option<String>,
        optional: Option<bool>,
        required: Option<bool>,
        #[serde(default)]
        data: Vec<ArgOption>,
        // Legacy Raycast field: `secure: true` → password.
        secure: Option<bool>,
    }
    let a: ArgJson = serde_json::from_str(json).ok()?;
    let arg_type = a.arg_type.unwrap_or_else(|| {
        if a.secure.unwrap_or(false) {
            "password".into()
        } else {
            "text".into()
        }
    });
    // `optional` wins; else `optional = !required`; default required → not optional.
    let optional = a.optional.unwrap_or_else(|| !a.required.unwrap_or(true));
    Some(ScriptArgument {
        index,
        arg_type,
        placeholder: a.placeholder,
        optional,
        data: a.data.into_iter().map(|d| (d.title, d.value)).collect(),
    })
}

fn apply_metadata(meta: &mut ScriptMetadata, key: &str, value: &str) {
    match key {
        "title" => meta.title = Some(unquote(value).to_string()),
        "description" => meta.description = Some(unquote(value).to_string()),
        "icon" => meta.icon_name = Some(unquote(value).to_string()),
        "iconDark" => meta.icon_name_dark = Some(unquote(value).to_string()),
        "mode" => meta.mode = Some(unquote(value).to_string()),
        "packageName" => meta.package_name = Some(unquote(value).to_string()),
        "author" => meta.author = Some(unquote(value).to_string()),
        "currentDirectoryPath" => meta.current_directory_path = Some(unquote(value).to_string()),
        "needsConfirmation" => {
            meta.needs_confirmation = unquote(value).eq_ignore_ascii_case("true")
        }
        "keywords" => {
            if let Ok(arr) = serde_json::from_str::<Vec<String>>(unquote(value)) {
                meta.keywords = arr;
            }
        }
        "refreshTime" => meta.refresh_seconds = parse_refresh_time(unquote(value)),
        "argument1" | "argument2" | "argument3" => {
            let idx: u8 = key[8..].parse().unwrap_or(0);
            if let Some(arg) = parse_argument(unquote(value), idx) {
                meta.arguments.push(arg);
            }
        }
        // schemaVersion, authorURL, exec, terminal: stored/ignored as needed.
        _ => {}
    }
}

/// Parse `@raycast.*`/`@vicinae.*` directives from a script's text head.
/// Returns `None` when no such metadata is present.
fn parse_metadata_from_text(head: &str) -> Option<ScriptMetadata> {
    let mut meta = ScriptMetadata::default();
    let mut found_any = false;

    for raw in head.lines() {
        let line = raw.trim_start();
        let after_marker = if let Some(r) = line.strip_prefix("//") {
            r
        } else if let Some(r) = line.strip_prefix("--") {
            r
        } else if let Some(r) = line.strip_prefix('#') {
            r
        } else if let Some(r) = line.strip_prefix(';') {
            r
        } else {
            continue;
        };
        let after_marker = after_marker.trim_start();
        let rest = after_marker
            .strip_prefix("@raycast.")
            .or_else(|| after_marker.strip_prefix("@vicinae."))
            .or_else(|| after_marker.strip_prefix("@Raycast."))
            .or_else(|| after_marker.strip_prefix("@Vicinae."));
        let rest = match rest {
            Some(r) => r,
            None => continue,
        };
        found_any = true;
        let (key, value) = match rest.find(char::is_whitespace) {
            Some(i) => (&rest[..i], rest[i..].trim()),
            None => (rest, ""),
        };
        apply_metadata(&mut meta, key, value);
    }

    if found_any {
        Some(meta)
    } else {
        None
    }
}

/// Read at most the metadata-sized prefix and parse `@raycast.*`/`@vicinae.*`
/// directives. Lossy decoding keeps an otherwise valid header usable when the
/// byte limit lands in the middle of a UTF-8 code point.
fn parse_script_metadata_reader(reader: impl Read) -> Option<ScriptMetadata> {
    let mut bytes = Vec::with_capacity(METADATA_HEAD_BYTES);
    reader
        .take(METADATA_HEAD_BYTES as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    parse_metadata_from_text(&String::from_utf8_lossy(&bytes))
}

/// Open a script and parse only its bounded metadata header.
fn parse_script_metadata(path: &Path) -> Option<ScriptMetadata> {
    parse_script_metadata_reader(fs::File::open(path).ok()?)
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
                path.parent()
                    .map(|p| p.join("vscode.png"))
                    .unwrap_or(stem_png)
            } else {
                stem_png
            };
            let icon = if png_path.exists() {
                fs::read(&png_path)
                    .ok()
                    .map(|bytes| format!("data:image/png;base64,{}", base64_encode(&bytes)))
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

            // Parse @raycast.*/@vicinae.* header metadata. Binary files
            // (.lnk, AppImages) and bare scripts without directives → None.
            let metadata = parse_script_metadata(&path);

            Some(ScriptInfo {
                name: stem,
                path: path_str,
                ext,
                icon,
                folder: folder.clone(),
                is_folder: false,
                metadata,
            })
        })
        .collect()
}

fn discover_scripts(scripts_dir: &str) -> Result<Vec<ScriptInfo>, String> {
    let dir = Path::new(scripts_dir);
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
            fs::read(&folder_png)
                .ok()
                .map(|bytes| format!("data:image/png;base64,{}", base64_encode(&bytes)))
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
            metadata: None,
        });

        scripts.extend(collect_script_files(&path, Some(folder_name)));
    }

    Ok(scripts)
}

#[tauri::command]
pub async fn list_scripts(scripts_dir: String) -> Result<Vec<ScriptInfo>, String> {
    if scripts_dir.is_empty() {
        return Ok(vec![]);
    }

    // Directory traversal, metadata parsing, and sibling PNG reads are all
    // blocking filesystem operations. Keep them off Tauri's async executor.
    let scripts = tokio::task::spawn_blocking(move || discover_scripts(&scripts_dir))
        .await
        .map_err(|e| e.to_string())??;

    // Windows: for .lnk files without a PNG icon, extract the shell icon via PowerShell.
    #[cfg(target_os = "windows")]
    let scripts = {
        let mut scripts = scripts;
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
        scripts
    };

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
        .filter_map(|r| {
            r.icon
                .map(|ico| (r.path, format!("data:image/png;base64,{ico}")))
        })
        .collect()
}

pub(crate) fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 {
            chunk[1] as usize
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            chunk[2] as usize
        } else {
            0
        };
        out.push(CHARS[b0 >> 2] as char);
        out.push(CHARS[((b0 & 3) << 4) | (b1 >> 4)] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((b1 & 0xf) << 2) | (b2 >> 6)] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[b2 & 0x3f] as char
        } else {
            '='
        });
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
        let mut text =
            String::from_utf8_lossy(&bytes[..bytes.len().min(TEXT_PREVIEW_MAX_BYTES)]).to_string();
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
            format!(
                "Start-Process code -ArgumentList '\"{}\"' -WindowStyle Hidden",
                escaped
            )
        } else {
            // Everything else: let Windows shell handle via file association
            format!("Start-Process -FilePath '{}' -WindowStyle Hidden", escaped)
        };

        let mut cmd = std::process::Command::new("powershell.exe");
        if ext == "ps1" {
            cmd.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &win_path,
            ]);
        } else {
            cmd.args(["-NonInteractive", "-Command", &ps_cmd]);
        }
        cmd.creation_flags(CREATE_NO_WINDOW);

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
            let opener = if cfg!(target_os = "macos") {
                "open"
            } else {
                "xdg-open"
            };
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

/// Run a script and capture its first line of stdout — used by inline scripts
/// (`@vicinae.mode inline`) whose output becomes a live-refreshing palette row.
/// Only direct-exec script types are supported (shell scripts with a shebang,
/// `.sh`, or Windows `.bat`/`.cmd`/`.ps1`); launchers that open in another app
/// (`.code-workspace`, `.desktop`, `.command`, `.lnk`) can't be captured and
/// error out. A 10 s timeout guards against hung scripts blocking the palette.
fn capture_script_output(path: &str) -> Result<String, String> {
    let script_path = std::path::Path::new(path);
    let ext = script_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        if ext != "bat" && ext != "cmd" && ext != "ps1" && ext != "sh" {
            return Err("inline mode only supports .bat/.cmd/.ps1/.sh scripts on Windows".into());
        }
        let mut cmd = if ext == "ps1" {
            let mut command = std::process::Command::new("powershell.exe");
            command.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                path,
            ]);
            command
        } else {
            let mut command = std::process::Command::new("cmd");
            command.arg("/c").arg(path);
            command
        };
        cmd.creation_flags(CREATE_NO_WINDOW);
        if let Some(dir) = script_path.parent() {
            cmd.current_dir(dir);
        }
        run_and_read_first_line(&mut cmd)
    }

    #[cfg(not(target_os = "windows"))]
    {
        if ext == "code-workspace" {
            return Err("inline mode is not supported for .code-workspace files".into());
        }
        #[cfg(target_os = "linux")]
        if ext == "desktop" {
            return Err("inline mode is not supported for .desktop launchers".into());
        }
        #[cfg(target_os = "macos")]
        if ext == "command" {
            return Err("inline mode is not supported for .command files".into());
        }
        let mut cmd = if is_executable(script_path) {
            std::process::Command::new(path)
        } else if ext == "sh" {
            let mut c = std::process::Command::new("sh");
            c.arg(path);
            c
        } else {
            // No shebang, no exec bit, not .sh — try sh anyway as a last resort.
            let mut c = std::process::Command::new("sh");
            c.arg(path);
            c
        };
        if let Some(dir) = script_path.parent() {
            cmd.current_dir(dir);
        }
        run_and_read_first_line(&mut cmd)
    }
}

const INLINE_SCRIPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const INLINE_LINE_MAX_CHARS: usize = 200;
// Every Unicode scalar is at most four UTF-8 bytes. Capture only enough bytes
// to render the visible prefix, then keep draining without retaining output so
// a noisy child can never fill its pipe or grow our memory use without bound.
const INLINE_LINE_MAX_BYTES: usize = INLINE_LINE_MAX_CHARS * 4;
type PipeDrainResult = Result<Vec<u8>, String>;
type PipeDrain = (
    std::sync::mpsc::Receiver<PipeDrainResult>,
    std::thread::JoinHandle<()>,
);

fn drain_first_line(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
    let mut captured = Vec::with_capacity(INLINE_LINE_MAX_BYTES);
    let mut keep_capturing = true;
    let mut buffer = [0u8; 4096];

    loop {
        let read = pipe.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        if !keep_capturing {
            continue;
        }

        let chunk = &buffer[..read];
        let line_end = chunk.iter().position(|byte| *byte == b'\n').unwrap_or(read);
        let remaining = INLINE_LINE_MAX_BYTES.saturating_sub(captured.len());
        captured.extend_from_slice(&chunk[..line_end.min(remaining)]);
        if line_end < read || captured.len() == INLINE_LINE_MAX_BYTES {
            keep_capturing = false;
        }
    }

    Ok(captured)
}

fn visible_first_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim()
        .chars()
        .take(INLINE_LINE_MAX_CHARS)
        .collect()
}

fn spawn_pipe_drain<R>(pipe: R, stream: &'static str) -> Result<PipeDrain, String>
where
    R: Read + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    let thread = std::thread::Builder::new()
        .name(format!("inline-script-{stream}"))
        .spawn(move || {
            let result = drain_first_line(pipe)
                .map_err(|error| format!("Failed to read script {stream}: {error}"));
            let _ = tx.send(result);
        })
        .map_err(|error| format!("Failed to start script {stream} reader: {error}"))?;
    Ok((rx, thread))
}

fn terminate_script_child(child: &mut std::process::Child) {
    let pid = child.id();

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if let Ok(process_group) = i32::try_from(pid) {
        // The child is placed in its own process group before spawn. Kill the
        // whole group so a shell cannot leave grandchildren running or holding
        // our stdout/stderr pipes open after the timeout.
        let _ = unsafe { libc::kill(-process_group, libc::SIGKILL) };
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let pid = pid.to_string();
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", pid.as_str(), "/T", "/F"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }

    // Always target the direct child too: this is the fallback if group/tree
    // termination was unavailable or raced with process exit. wait() reaps it.
    let _ = child.kill();
    let _ = child.wait();
}

fn poll_pipe_result(
    receiver: &std::sync::mpsc::Receiver<Result<Vec<u8>, String>>,
    result: &mut Option<Result<Vec<u8>, String>>,
    stream: &str,
) {
    if result.is_some() {
        return;
    }
    match receiver.try_recv() {
        Ok(value) => *result = Some(value),
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            *result = Some(Err(format!("Script {stream} reader stopped unexpectedly")));
        }
    }
}

fn run_and_read_first_line_with_timeout(
    cmd: &mut std::process::Command,
    timeout: std::time::Duration,
) -> Result<String, String> {
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let mut child = cmd
        .spawn()
        .map_err(|error| format!("Failed to run script: {error}"))?;
    let Some(stdout) = child.stdout.take() else {
        terminate_script_child(&mut child);
        return Err("Failed to open script stdout".to_string());
    };
    let Some(stderr) = child.stderr.take() else {
        terminate_script_child(&mut child);
        return Err("Failed to open script stderr".to_string());
    };

    let (stdout_rx, stdout_thread) = match spawn_pipe_drain(stdout, "stdout") {
        Ok(reader) => reader,
        Err(error) => {
            terminate_script_child(&mut child);
            return Err(error);
        }
    };
    let (stderr_rx, stderr_thread) = match spawn_pipe_drain(stderr, "stderr") {
        Ok(reader) => reader,
        Err(error) => {
            terminate_script_child(&mut child);
            let _ = stdout_thread.join();
            return Err(error);
        }
    };

    let deadline = std::time::Instant::now() + timeout;
    let mut child_exited = false;
    let mut stdout_result = None;
    let mut stderr_result = None;
    let failure = loop {
        poll_pipe_result(&stdout_rx, &mut stdout_result, "stdout");
        poll_pipe_result(&stderr_rx, &mut stderr_result, "stderr");

        if let Some(Err(error)) = stdout_result.as_ref() {
            break Some(error.clone());
        }
        if let Some(Err(error)) = stderr_result.as_ref() {
            break Some(error.clone());
        }
        if !child_exited {
            match child.try_wait() {
                Ok(Some(_)) => child_exited = true,
                Ok(None) => {}
                Err(error) => break Some(format!("Failed waiting for script: {error}")),
            }
        }
        if child_exited && stdout_result.is_some() && stderr_result.is_some() {
            break None;
        }

        let now = std::time::Instant::now();
        if now >= deadline {
            break Some("Script timed out (>10s)".to_string());
        }
        std::thread::sleep(
            deadline
                .saturating_duration_since(now)
                .min(std::time::Duration::from_millis(10)),
        );
    };

    if failure.is_some() {
        terminate_script_child(&mut child);
    } else {
        // try_wait() already reaped the child; wait() returns the cached status.
        let _ = child.wait();
    }
    let stdout_join = stdout_thread.join();
    let stderr_join = stderr_thread.join();

    if let Some(error) = failure {
        return Err(error);
    }
    stdout_join.map_err(|_| "Script stdout reader panicked".to_string())?;
    stderr_join.map_err(|_| "Script stderr reader panicked".to_string())?;

    let stdout = visible_first_line(
        &stdout_result.ok_or_else(|| "Script stdout reader returned no result".to_string())??,
    );
    if !stdout.is_empty() {
        return Ok(stdout);
    }
    let stderr = visible_first_line(
        &stderr_result.ok_or_else(|| "Script stderr reader returned no result".to_string())??,
    );
    if !stderr.is_empty() {
        return Err(stderr);
    }
    Err("Script produced no output".into())
}

fn run_and_read_first_line(cmd: &mut std::process::Command) -> Result<String, String> {
    run_and_read_first_line_with_timeout(cmd, INLINE_SCRIPT_TIMEOUT)
}

#[tauri::command]
pub async fn run_script_capture(path: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || capture_script_output(&path))
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
            use std::os::windows::process::CommandExt;
            // explorer.exe has its own quirky command-line parsing: the switch
            // and path must arrive as the single token `/select,"C:\the\path"`.
            // std's `.arg()` quotes the WHOLE argument when it contains a space
            // (`"/select,C:\the path"`), which explorer can't parse — it then
            // silently falls back to opening the default folder (Documents).
            // `raw_arg` writes the command line verbatim so the quotes land
            // around the path only.
            std::process::Command::new("explorer")
                .raw_arg(format!("/select,\"{path}\""))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_time_parses_units() {
        assert_eq!(parse_refresh_time("5s"), Some(5));
        assert_eq!(parse_refresh_time("2m"), Some(120));
        assert_eq!(parse_refresh_time("1h"), Some(3600));
        assert_eq!(parse_refresh_time("1d"), Some(86400));
        // No unit → rejected.
        assert_eq!(parse_refresh_time("30"), None);
        // Zero would create a tight polling loop; overflow must not wrap.
        assert_eq!(parse_refresh_time("0s"), None);
        assert_eq!(parse_refresh_time(&format!("{}m", u64::MAX)), None);
        // Garbage.
        assert_eq!(parse_refresh_time(""), None);
        assert_eq!(parse_refresh_time("abc"), None);
    }

    #[test]
    fn inline_first_line_capture_is_bounded_and_unicode_safe() {
        let first_line = format!("a{}", "🙂".repeat(300));
        let input = format!("{first_line}\nignored");
        let captured = drain_first_line(std::io::Cursor::new(input)).unwrap();

        assert!(captured.len() <= INLINE_LINE_MAX_BYTES);
        assert_eq!(
            visible_first_line(&captured),
            first_line
                .chars()
                .take(INLINE_LINE_MAX_CHARS)
                .collect::<String>()
        );
    }

    #[test]
    fn unquote_strips_matching_quotes() {
        assert_eq!(unquote("\"hello\""), "hello");
        assert_eq!(unquote("'hello'"), "hello");
        assert_eq!(unquote("hello"), "hello");
        // Mismatched quotes are left intact.
        assert_eq!(unquote("\"hello'"), "\"hello'");
    }

    #[test]
    fn metadata_parses_raycast_style() {
        let src = "#!/bin/bash\n# @raycast.schemaVersion 1\n# @raycast.title Git Status\n# @raycast.description Show the working tree status\n# @raycast.icon git\n# @raycast.mode inline\n# @raycast.needsConfirmation true\n# @raycast.packageName dev\n";
        let m = parse_metadata_from_text(src).expect("metadata should be found");
        assert_eq!(m.title.as_deref(), Some("Git Status"));
        assert_eq!(
            m.description.as_deref(),
            Some("Show the working tree status")
        );
        assert_eq!(m.icon_name.as_deref(), Some("git"));
        assert_eq!(m.mode.as_deref(), Some("inline"));
        assert!(m.needs_confirmation);
        assert_eq!(m.package_name.as_deref(), Some("dev"));
    }

    #[test]
    fn metadata_parses_vicinae_keywords_and_refresh() {
        let src = "-- @vicinae.schemaVersion 1\n-- @vicinae.title CPU Load\n-- @vicinae.keywords [\"cpu\", \"load\", \"monitor\"]\n-- @vicinae.refreshTime 5s\n-- @vicinae.mode inline\n";
        let m = parse_metadata_from_text(src).expect("metadata should be found");
        assert_eq!(m.title.as_deref(), Some("CPU Load"));
        assert_eq!(m.keywords, vec!["cpu", "load", "monitor"]);
        assert_eq!(m.refresh_seconds, Some(5));
    }

    #[test]
    fn metadata_parses_arguments() {
        let src = "# @vicinae.title Echo\n# @vicinae.argument1 {\"type\": \"text\", \"placeholder\": \"Message\", \"required\": true}\n# @vicinae.argument2 {\"type\": \"dropdown\", \"placeholder\": \"Level\", \"data\": [{\"title\": \"Info\", \"value\": \"info\"}, {\"title\": \"Error\", \"value\": \"err\"}]}\n";
        let m = parse_metadata_from_text(src).expect("metadata should be found");
        assert_eq!(m.arguments.len(), 2);
        assert_eq!(m.arguments[0].index, 1);
        assert_eq!(m.arguments[0].arg_type, "text");
        assert!(!m.arguments[0].optional);
        assert_eq!(m.arguments[1].arg_type, "dropdown");
        assert_eq!(
            m.arguments[1].data,
            vec![
                ("Info".into(), "info".into()),
                ("Error".into(), "err".into())
            ]
        );
    }

    #[test]
    fn metadata_secure_argument_becomes_password() {
        // Legacy Raycast `secure: true` (no `type`) → password.
        let src = "# @raycast.argument1 {\"secure\": true, \"placeholder\": \"Token\"}\n";
        let m = parse_metadata_from_text(src).expect("metadata should be found");
        assert_eq!(m.arguments[0].arg_type, "password");
        assert_eq!(m.arguments[0].placeholder.as_deref(), Some("Token"));
    }

    #[test]
    fn metadata_none_for_plain_script() {
        // No @raycast.*/@vicinae.* directives → None.
        assert!(parse_metadata_from_text("#!/bin/bash\necho hello\n").is_none());
        // A plain comment without the directive prefix is ignored.
        assert!(parse_metadata_from_text("# just a comment\n").is_none());
    }

    #[test]
    fn metadata_accepts_mixed_comment_markers() {
        // `//` (js), `--` (lua), `;` (ini-style) all work.
        let src = "// @vicinae.title JS Task\n-- @vicinae.author me\n; @vicinae.packageName misc\n";
        let m = parse_metadata_from_text(src).expect("metadata should be found");
        assert_eq!(m.title.as_deref(), Some("JS Task"));
        assert_eq!(m.author.as_deref(), Some("me"));
        assert_eq!(m.package_name.as_deref(), Some("misc"));
    }

    #[test]
    fn metadata_reader_does_not_read_large_script_body() {
        let mut script = b"#!/bin/sh\n# @vicinae.title Bounded read\n".to_vec();
        script.resize(METADATA_HEAD_BYTES * 128, b'x');
        let mut reader = std::io::Cursor::new(script);

        let metadata = parse_script_metadata_reader(&mut reader).expect("metadata should parse");

        assert_eq!(metadata.title.as_deref(), Some("Bounded read"));
        assert_eq!(reader.position(), METADATA_HEAD_BYTES as u64);
    }

    #[test]
    fn metadata_beyond_reader_prefix_is_ignored() {
        let mut script = vec![b'x'; METADATA_HEAD_BYTES];
        script.extend_from_slice(b"\n# @vicinae.title Too late\n");
        let mut reader = std::io::Cursor::new(script);

        assert!(parse_script_metadata_reader(&mut reader).is_none());
        assert_eq!(reader.position(), METADATA_HEAD_BYTES as u64);
    }

    #[test]
    fn metadata_survives_utf8_code_point_split_at_reader_limit() {
        let mut script = b"# @vicinae.title Unicode boundary\n".to_vec();
        script.resize(METADATA_HEAD_BYTES - 1, b' ');
        script.extend_from_slice("é".as_bytes());
        let mut reader = std::io::Cursor::new(script);

        let metadata = parse_script_metadata_reader(&mut reader).expect("metadata should parse");

        assert_eq!(metadata.title.as_deref(), Some("Unicode boundary"));
        assert_eq!(reader.position(), METADATA_HEAD_BYTES as u64);
    }
}

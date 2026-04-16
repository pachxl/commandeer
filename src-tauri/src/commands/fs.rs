use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct ScriptInfo {
    pub name: String,
    pub path: String,
    pub ext: String,
    pub icon: Option<String>,
    pub folder: Option<String>,
    pub is_folder: bool,
}

fn collect_script_files(dir: &Path, folder: Option<String>) -> Vec<ScriptInfo> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            matches!(
                path.extension().and_then(|x| x.to_str()),
                Some("bat") | Some("cmd") | Some("lnk") | Some("code-workspace")
            )
        })
        .filter_map(|e| {
            let path = e.path();
            let ext = path.extension()?.to_string_lossy().into_owned();
            let stem = path.file_stem()?.to_string_lossy().into_owned();
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

    // For .lnk files without a PNG icon, extract the shell icon via PowerShell
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

    Ok(scripts)
}

#[derive(Deserialize)]
struct IconResult {
    path: String,
    icon: Option<String>,
}

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
$paths = @({arr})
$result = $paths | ForEach-Object {{
    $p = $_
    try {{
        $ico = [System.Drawing.Icon]::ExtractAssociatedIcon($p)
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

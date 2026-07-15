[CmdletBinding()]
param(
    [switch]$Install,
    [ValidateRange(5, 1440)]
    [int]$IntervalMinutes = 15
)

$ErrorActionPreference = 'Stop'
$TaskName = 'Commandeer Auto Update'
$RepoRoot = Split-Path -Parent $PSScriptRoot
$StateDir = Join-Path $env:LOCALAPPDATA 'Commandeer'
$LogPath = Join-Path $StateDir 'auto-update.log'
$StampPath = Join-Path $StateDir 'deployed-commit.txt'
$BinaryPath = Join-Path $RepoRoot 'bin\commandeer.exe'
$BuiltBinaryPath = Join-Path $RepoRoot 'src-tauri\target\release\commandeer.exe'

New-Item -ItemType Directory -Force -Path $StateDir | Out-Null

function Write-UpdateLog {
    param([string]$Message)
    $line = '{0} {1}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $Message
    Add-Content -LiteralPath $LogPath -Value $line
}

function Refresh-Path {
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = $machinePath + ';' + $userPath
}

function Invoke-External {
    param(
        [Parameter(Mandatory = $true)][string]$Command,
        [Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments
    )

    Write-UpdateLog (('> {0} {1}' -f $Command, ($Arguments -join ' ')).Trim())
    # Windows PowerShell 5.1 turns native stderr into ErrorRecord objects and
    # $ErrorActionPreference = Stop would terminate on harmless progress (for
    # example, git fetch's "From ..."). The native exit code remains the source
    # of truth for success or failure.
    $previousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $Command @Arguments 2>&1 | ForEach-Object {
            Add-Content -LiteralPath $LogPath -Value $_.ToString()
        }
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorActionPreference
    }
    if ($exitCode -ne 0) {
        throw "Command failed with exit code ${exitCode}: $Command"
    }
}

function Ensure-CommandeerRunning {
    if (-not (Test-Path -LiteralPath $BinaryPath)) {
        return
    }
    if (-not (Get-Process -Name 'commandeer' -ErrorAction SilentlyContinue)) {
        Start-Process -FilePath $BinaryPath -WorkingDirectory $RepoRoot
        Write-UpdateLog 'Started Commandeer.'
    }
}

function Install-UpdateTask {
    $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $arguments = '-NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "{0}"' -f $PSCommandPath
    $action = New-ScheduledTaskAction -Execute $powershell -Argument $arguments -WorkingDirectory $RepoRoot
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
    $logonTrigger = New-ScheduledTaskTrigger -AtLogOn -User $identity
    # Windows PowerShell 5.1 only exposes repetition through the Once
    # parameter set; daily trigger objects do not have mutable repetition
    # properties. Ten years is effectively indefinite for this local task.
    $repeatTrigger = New-ScheduledTaskTrigger `
        -Once `
        -At (Get-Date).AddMinutes(1) `
        -RepetitionInterval (New-TimeSpan -Minutes $IntervalMinutes) `
        -RepetitionDuration (New-TimeSpan -Days 3650)
    $settings = New-ScheduledTaskSettingsSet `
        -AllowStartIfOnBatteries `
        -DontStopIfGoingOnBatteries `
        -StartWhenAvailable `
        -MultipleInstances IgnoreNew `
        -ExecutionTimeLimit (New-TimeSpan -Hours 2)
    $principal = New-ScheduledTaskPrincipal -UserId $identity -LogonType Interactive -RunLevel Limited

    Register-ScheduledTask `
        -TaskName $TaskName `
        -Action $action `
        -Trigger @($logonTrigger, $repeatTrigger) `
        -Settings $settings `
        -Principal $principal `
        -Description 'Keeps Commandeer on origin/main and running for the signed-in user.' `
        -Force | Out-Null
    Write-UpdateLog "Installed scheduled task with a ${IntervalMinutes}-minute update interval."
}

$mutex = New-Object Threading.Mutex($false, 'Local\CommandeerAutoUpdate')
$hasMutex = $false
$failed = $false
try {
    $hasMutex = $mutex.WaitOne(0)
    if (-not $hasMutex) {
        exit 0
    }

    Refresh-Path
    if ($Install) {
        Install-UpdateTask
    }

    # Make the existing release available immediately at sign-in. If an update
    # exists, it stays live while the replacement compiles.
    Ensure-CommandeerRunning

    Push-Location $RepoRoot
    try {
        Write-UpdateLog 'Checking origin/main.'
        Invoke-External git fetch --prune origin main

        $currentBranch = (& git branch --show-current).Trim()
        if ($LASTEXITCODE -ne 0 -or $currentBranch -ne 'main') {
            throw "Expected the checkout to be on main; found '$currentBranch'."
        }

        $localCommit = (& git rev-parse HEAD).Trim()
        $remoteCommit = (& git rev-parse origin/main).Trim()
        if ($LASTEXITCODE -ne 0) {
            throw 'Could not resolve the local or remote commit.'
        }

        if ($localCommit -ne $remoteCommit) {
            & git merge-base --is-ancestor HEAD origin/main
            if ($LASTEXITCODE -ne 0) {
                throw 'Local main has diverged from origin/main; refusing to overwrite it.'
            }
            Invoke-External git merge --ff-only origin/main
            $localCommit = (& git rev-parse HEAD).Trim()
            Write-UpdateLog "Fast-forwarded to $localCommit."
        }

        $deployedCommit = ''
        if (Test-Path -LiteralPath $StampPath) {
            $deployedCommit = (Get-Content -LiteralPath $StampPath -Raw).Trim()
        }

        if ($deployedCommit -ne $localCommit -or -not (Test-Path -LiteralPath $BinaryPath)) {
            Write-UpdateLog "Building release $localCommit."
            Invoke-External bun install --frozen-lockfile
            Invoke-External npm run tauri -- build --no-bundle

            Stop-Process -Name 'commandeer' -Force -ErrorAction SilentlyContinue
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $BinaryPath) | Out-Null
            Copy-Item -LiteralPath $BuiltBinaryPath -Destination $BinaryPath -Force
            Set-Content -LiteralPath $StampPath -Value $localCommit
            Write-UpdateLog "Deployed release $localCommit."
        }
    }
    finally {
        Pop-Location
    }
}
catch {
    $failed = $true
    Write-UpdateLog ('ERROR: ' + $_.Exception.Message)
}
finally {
    Ensure-CommandeerRunning
    if ($hasMutex) {
        $mutex.ReleaseMutex()
    }
    $mutex.Dispose()
}

if ($failed) {
    exit 1
}

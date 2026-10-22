# Shroudtopia

## The app's data folder, seen from Claude

The Claude desktop app is an MSIX package. Every process started from it (its Bash/PowerShell, or an exe opened from
a link in the Claude window) reads and writes `%APPDATA%` inside
`C:\Users\conta\AppData\Local\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming`, not the real
`C:\Users\conta\AppData\Roaming`. So:

- `%APPDATA%\Shroudtopia` as Claude reads it is **not** the folder of the app Paulo opens by double-click. Read or
  write the real one through WSL: `/mnt/c/Users/conta/AppData/Roaming/Shroudtopia` (WSL is outside the package).
- An app build Claude starts uses that boxed data (other config, other shared worlds). For a test that counts, Paulo
  opens `Shroudtopia.exe` by double-click in Windows Explorer.
- On 2026-10-21 his shared worlds lived only in the boxed copy (set up in apps Claude had started); its config.json was
  copied into the real folder, the old one kept as `config.before-worlds-fix.json`.

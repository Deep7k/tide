# Activates the project-local Rust toolchain in .toolchain/ (like a Python venv).
# Usage:  . .\env.ps1     (note the leading dot)
$root = $PSScriptRoot
$env:RUSTUP_HOME = "$root\.toolchain\rustup"
$env:CARGO_HOME  = "$root\.toolchain\cargo"
if ($env:PATH -notlike "*$env:CARGO_HOME\bin*") { $env:PATH = "$env:CARGO_HOME\bin;$env:PATH" }
function global:prompt { "(tide) PS $($PWD.Path)> " }

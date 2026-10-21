$ErrorActionPreference = 'Stop'
$name  = "MaterialCraft_$($env:VERSION)_$($env:LABEL)"
$stage = "$env:RUNNER_TEMP/stage/$name"
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item -Path "$env:OUT/materialcraft.exe"     -Destination "$stage/materialcraft.exe"
Copy-Item -Path "$env:OUT/materialcraft-gui.exe" -Destination "$stage/materialcraft-gui.exe"
Copy-Item -Path "$env:RUNNER_TEMP/materialcraft-notices/*" -Destination $stage -Recurse
Copy-Item -Path README.md -Destination $stage
New-Item -ItemType Directory -Force -Path dist | Out-Null
Compress-Archive -Path $stage -DestinationPath "dist/$name.zip"

# Exercise the binaries from the finished archive.
Expand-Archive "dist/$name.zip" -DestinationPath "$env:RUNNER_TEMP/verify"
$dir = "$env:RUNNER_TEMP/verify/$name"
if (-not (Test-Path "$dir/materialcraft-gui.exe")) { throw "GUI executable missing from ZIP" }
$smoke = "$env:RUNNER_TEMP/smoke"
& "$dir/materialcraft.exe" example $smoke
if ($LASTEXITCODE) { throw "example exited $LASTEXITCODE" }
& "$dir/materialcraft.exe" render "$smoke/brick_wall.sbs" "$smoke/out" --size 64
if ($LASTEXITCODE) { throw "render exited $LASTEXITCODE" }
if (-not (Get-ChildItem "$smoke/out" -Filter *.png)) { throw "render wrote no PNG" }

# 1) Get the 5‑char SHA
$sha = git rev-parse --short=5 HEAD

# 2) Extract version from Cargo.toml
$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' |
             ForEach-Object { $_.Matches[0].Groups[1].Value })

# 3) Get yy/MM/dd date
$date = Get-Date -Format 'yy/MM/dd'

# 4) Combine the data into the LAST_RELEASE content
$content = "$date|$sha|$version"

# 5) Write to LAST_RELEASE in UTF-8 without a trailing newline
Set-Content -Path LAST_RELEASE -Value $content -Encoding UTF8
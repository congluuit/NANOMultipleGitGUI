# Creates the throw-away folders `app.real.ts` drives the real app with: a repository with some
# history, a local "server" to fetch from, a second branch and one uncommitted change.
#   powershell -File e2e-real/setup.ps1 [-Root E:\AITempKnowledge\nanomgg-real-e2e]
param([string]$Root = "E:\AITempKnowledge\nanomgg-real-e2e")

$ErrorActionPreference = "Stop"
foreach ($name in "repo", "remote.git", "profile", "webview") {
  if (Test-Path "$Root\$name") { Remove-Item -Recurse -Force "$Root\$name" }
}
New-Item -ItemType Directory -Force "$Root\repo", "$Root\profile", "$Root\webview" | Out-Null
Set-Location $Root
git init -q --bare remote.git
Set-Location "$Root\repo"
git init -q -b main
git config user.name "Tester"
git config user.email "tester@example.test"
git config core.autocrlf false
New-Item -ItemType Directory -Force src, docs, assets | Out-Null
Set-Content -Encoding utf8 README.md "# Sample`n"
Set-Content -Encoding utf8 src\calc.js "export function total(price, count) {`n  return price * count;`n}`n"
Set-Content -Encoding utf8 docs\guide.md "guide`n"
Set-Content -Encoding utf8 assets\logo.txt "logo`n"
git add -A
git commit -q -m "Initial commit"
foreach ($i in 1..6) {
  Add-Content -Encoding utf8 docs\guide.md "line $i"
  # The bug bisect has to find.
  if ($i -eq 4) {
    Set-Content -Encoding utf8 src\calc.js "export function total(price, count) {`n  return price + count;`n}`n"
  }
  git add -A
  git commit -q -m "Change $i"
}
git remote add origin "$Root\remote.git"
git push -q -u origin main
git switch -q -c feature/discount
Add-Content -Encoding utf8 README.md "discount"
git commit -q -am "Add discount note"
git switch -q main
# One uncommitted edit of a line (word-level diff).
Set-Content -Encoding utf8 src\calc.js "export function total(price, quantity) {`n  return price + quantity;`n}`n"
Write-Output "ready: $Root"

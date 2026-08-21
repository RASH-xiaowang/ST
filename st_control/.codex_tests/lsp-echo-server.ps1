# LSP test server (JSON-RPC over stdio with Content-Length framing)
# for Harness LSP client E2E. ASCII only (PowerShell 5.1 ANSI).
# Note: body is read as a line (ConvertTo-Json -Compress emits single-line JSON).
$ErrorActionPreference = 'SilentlyContinue'
function Write-Frame($obj) {
    $body = $obj | ConvertTo-Json -Depth 12 -Compress
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($body)
    $header = "Content-Length: $($bytes.Length)`r`n`r`n"
    [Console]::Out.Write($header)
    [Console]::Out.Write($body)
    [Console]::Out.Flush()
}
while ($true) {
    $contentLength = $null
    while ($true) {
        $line = [Console]::In.ReadLine()
        if ($null -eq $line) { exit }
        if ($line.Trim().Length -eq 0) { break }
        if ($line -like 'Content-Length:*') {
            $contentLength = [int](($line -replace 'Content-Length:', '').Trim())
        }
    }
    if ($null -eq $contentLength -or $contentLength -le 0) { continue }
    # 按 Content-Length 精确读取正文（[Console]::In.Read 的字符重载）
    $buffer = New-Object char[] $contentLength
    $read = 0
    while ($read -lt $contentLength) {
        $n = [Console]::In.Read($buffer, $read, $contentLength - $read)
        if ($n -le 0) { break }
        $read += $n
    }
    $body = -join $buffer[0..($read - 1)]
    if ($body.Trim().Length -eq 0) { continue }
    $msg = $body | ConvertFrom-Json
    switch ($msg.method) {
        'initialize' {
            Write-Frame ([ordered]@{
                jsonrpc = '2.0'; id = $msg.id
                result = [ordered]@{
                    capabilities = [ordered]@{ hoverProvider = $true }
                    serverInfo = [ordered]@{ name = 'st-lsp-echo'; version = '1.0.0' }
                }
            })
        }
        'shutdown' {
            Write-Frame ([ordered]@{ jsonrpc = '2.0'; id = $msg.id; result = $null })
        }
        'textDocument/hover' {
            $line = $msg.params.position.line
            $col = $msg.params.position.character
            Write-Frame ([ordered]@{
                jsonrpc = '2.0'; id = $msg.id
                result = [ordered]@{
                    contents = "hover-info:line=$line,col=$col"
                }
            })
        }
        default { }
    }
}

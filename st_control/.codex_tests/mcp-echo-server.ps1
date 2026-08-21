# MCP test server (JSON-RPC over stdio) for Harness MCP client E2E.
# ASCII only: Windows PowerShell 5.1 reads .ps1 as ANSI without BOM.
$ErrorActionPreference = 'SilentlyContinue'
function Respond($id, $result) {
    $obj = [ordered]@{ jsonrpc = '2.0'; id = $id; result = $result }
    [Console]::Out.WriteLine(($obj | ConvertTo-Json -Depth 12 -Compress))
}
while ($true) {
    $line = [Console]::In.ReadLine()
    if ($null -eq $line) { break }
    $msg = $line | ConvertFrom-Json
    switch ($msg.method) {
        'initialize' {
            Respond $msg.id ([ordered]@{
                protocolVersion = '2024-11-05'
                capabilities = [ordered]@{ tools = [ordered]@{} }
                serverInfo = [ordered]@{ name = 'st-mcp-echo'; version = '1.0.0' }
            })
        }
        'notifications/initialized' { }
        'tools/list' {
            Respond $msg.id ([ordered]@{
                tools = @(
                    [ordered]@{
                        name = 'echo'
                        description = 'echo input text (E2E test)'
                        inputSchema = [ordered]@{ type = 'object'; properties = [ordered]@{ text = [ordered]@{ type = 'string' } } }
                    }
                )
            })
        }
        'tools/call' {
            $text = $msg.params.arguments.text
            Respond $msg.id ([ordered]@{
                content = @([ordered]@{ type = 'text'; text = ('echo:' + $text) })
            })
        }
        default { }
    }
}

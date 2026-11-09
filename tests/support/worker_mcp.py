"""Model-free test server. Grants exactly one fixture run, never real Palm data."""
import json
import pathlib
import sys

grant, output = sys.argv[1:]
for line in sys.stdin:
    request = json.loads(line)
    if "id" not in request:
        continue
    method = request.get("method")
    if method == "initialize":
        result = {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}},
                  "serverInfo": {"name": "isolated-worker-fixture", "version": "1"}}
    elif method == "tools/list":
        result = {"tools": [{"name": name, "description": "Submit fixture result" if name == "run.submit" else "Not granted",
                             "inputSchema": {"type": "object", "properties": {"runId": {"type": "string"}}, "required": ["runId"], "additionalProperties": False},
                             "annotations": {"readOnlyHint": False, "destructiveHint": False, "idempotentHint": True}}
                            for name in ["run.submit", "run.forbidden"]]}
    elif method == "tools/call":
        allowed = request["params"].get("name") == "run.submit" and request["params"].get("arguments", {}).get("runId") == grant
        if allowed:
            with pathlib.Path(output).open("a") as stream:
                stream.write(grant + "\n")
        result = {"isError": not allowed, "content": [{"type": "text", "text": "submitted " + grant if allowed else "outside this run grant"}]}
    else:
        result = {}
    print(json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}), flush=True)

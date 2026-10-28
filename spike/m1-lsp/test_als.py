import json, os, sys, pathlib
sys.path.insert(0, os.path.dirname(__file__))
from lsp_client import Client

node, als, venv, ws = sys.argv[1:5]
ws = pathlib.Path(ws).resolve()
doc = ws / "playbooks" / "site.yml"
uri = doc.as_uri()
settings = {"ansible": {
    "ansible": {"path": "ansible", "useFullyQualifiedCollectionNames": True},
    "python": {"interpreterPath": f"{venv}/bin/python", "activationScript": ""},
    "validation": {"enabled": True, "lint": {"enabled": True, "path": "ansible-lint", "arguments": ""}},
    "executionEnvironment": {"enabled": False},
    "completion": {"provideRedirectModules": True, "provideModuleOptionAliases": True},
}}
settings["*"] = settings["ansible"]
# Minimal env allowlist (mirrors the planned runner policy)
env = {k: os.environ[k] for k in ("HOME", "LANG", "PATH") if k in os.environ}
c = Client([node, als, "--stdio"], settings, env=env, cwd=str(ws))
rid = c.request("initialize", {"processId": os.getpid(), "rootUri": ws.as_uri(),
    "workspaceFolders": [{"uri": ws.as_uri(), "name": "ws"}],
    "capabilities": {"workspace": {"configuration": True, "workspaceFolders": True},
                     "textDocument": {"hover": {"contentFormat": ["markdown", "plaintext"]},
                                      "publishDiagnostics": {}, "completion": {"completionItem": {"snippetSupport": True}}}}})
init, _ = c.wait(lambda m: m.get("id") == rid, 30)
print("initialize ms:", c.elapsed(), "| server:", init and init["result"].get("serverInfo"))
print("capabilities:", sorted(init["result"]["capabilities"].keys()))
c.notify("initialized", {})
c.notify("textDocument/didOpen", {"textDocument": {"uri": uri, "languageId": "ansible", "version": 1, "text": doc.read_text()}})
t_open = c.elapsed()
diag, seen = c.wait(lambda m: m.get("method") == "textDocument/publishDiagnostics" and m["params"]["uri"] == uri and m["params"]["diagnostics"], 90)
print("diagnostics after ms:", c.elapsed() - t_open)
for d in (diag["params"]["diagnostics"] if diag else []):
    print("  ", d["range"]["start"], d.get("source"), d.get("code"), "-", d["message"][:100])
if not diag:
    for m in seen: print("  seen:", json.dumps(m)[:200])
# hover on 'ansible.builtin.package' (line 6, col 10)
t = c.elapsed()
rid = c.request("textDocument/hover", {"textDocument": {"uri": uri}, "position": {"line": 6, "character": 12}})
hov, _ = c.wait(lambda m: m.get("id") == rid, 30)
val = hov and hov.get("result") and hov["result"]["contents"]
print("hover ms:", c.elapsed() - t, "|", (json.dumps(val)[:300] if val else hov))
# completion inside module options (line 8 'state' -> col 8)
t = c.elapsed()
rid = c.request("textDocument/completion", {"textDocument": {"uri": uri}, "position": {"line": 9, "character": 8}})
comp, _ = c.wait(lambda m: m.get("id") == rid, 30)
items = comp and comp.get("result")
items = items.get("items", []) if isinstance(items, dict) else (items or [])
print("completion ms:", c.elapsed() - t, "| items:", len(items), [i["label"] for i in items[:8]])
rid = c.request("shutdown", None); c.wait(lambda m: m.get("id") == rid, 10); c.notify("exit", None)
try:
    c.p.wait(5); print("exit code:", c.p.returncode)
except Exception:
    print("server did not exit; killing"); c.p.kill()

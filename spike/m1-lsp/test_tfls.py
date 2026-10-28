import os, sys, pathlib, json
sys.path.insert(0, os.path.dirname(__file__))
from lsp_client import Client
exe, ws = sys.argv[1], pathlib.Path(sys.argv[2]).resolve()
doc = ws / "main.tf"; uri = doc.as_uri()
env = {k: os.environ[k] for k in ("HOME", "LANG", "PATH") if k in os.environ}
c = Client([exe, "serve"], {}, env=env, cwd=str(ws))
rid = c.request("initialize", {"processId": os.getpid(), "rootUri": ws.as_uri(),
  "workspaceFolders": [{"uri": ws.as_uri(), "name": "ws"}],
  "capabilities": {"workspace": {"configuration": True}, "textDocument": {"completion": {"completionItem": {"snippetSupport": True}}, "publishDiagnostics": {}}}})
init, _ = c.wait(lambda m: m.get("id") == rid, 30)
print("initialize ms:", c.elapsed(), init["result"].get("serverInfo"))
c.notify("initialized", {})
c.notify("textDocument/didOpen", {"textDocument": {"uri": uri, "languageId": "terraform", "version": 1, "text": doc.read_text()}})
t = c.elapsed()
d, _ = c.wait(lambda m: m.get("method") == "textDocument/publishDiagnostics" and m["params"]["diagnostics"], 30)
print("diagnostics ms:", c.elapsed() - t, [x["message"] for x in (d["params"]["diagnostics"] if d else [])])
t = c.elapsed()
rid = c.request("textDocument/completion", {"textDocument": {"uri": uri}, "position": {"line": 18, "character": 2}})
r, _ = c.wait(lambda m: m.get("id") == rid, 30)
res = r["result"]; items = res["items"] if isinstance(res, dict) else res
print("completion ms:", c.elapsed() - t, len(items), [i["label"] for i in items[:6]])
t = c.elapsed()
rid = c.request("shutdown", None); c.wait(lambda m: m.get("id") == rid, 10); c.notify("exit", None)
c.p.wait(5); print("shutdown+exit ms:", c.elapsed() - t, "code", c.p.returncode)
# EOF-only behaviour
c2 = Client([exe, "serve"], {}, env=env, cwd=str(ws)); import time; time.sleep(0.5)
t = c2.elapsed(); c2.p.stdin.close()
try: c2.p.wait(5); print("stdin EOF -> exit ms:", c2.elapsed() - t, "code", c2.p.returncode)
except Exception: print("stdin EOF: no exit within 5 s"); c2.p.kill()

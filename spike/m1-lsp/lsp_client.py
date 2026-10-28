"""Minimal LSP stdio test client for the M1 spike (throwaway)."""
import json, os, subprocess, sys, threading, time, queue

class Client:
    def __init__(self, argv, settings, env=None, cwd=None):
        self.t0 = time.monotonic()
        self.p = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, env=env, cwd=cwd)
        self.settings = settings
        self.q = queue.Queue()
        self.next_id = 0
        threading.Thread(target=self._reader, daemon=True).start()
        threading.Thread(target=self._stderr, daemon=True).start()

    def _stderr(self):
        for line in self.p.stderr:
            sys.stderr.write("[server stderr] " + line.decode(errors="replace"))

    def _reader(self):
        f = self.p.stdout
        while True:
            headers = {}
            while True:
                line = f.readline()
                if not line:
                    self.q.put(None); return
                line = line.strip()
                if not line:
                    break
                k, v = line.decode().split(":", 1)
                headers[k.lower()] = v.strip()
            body = f.read(int(headers["content-length"]))
            msg = json.loads(body)
            # answer server->client requests
            if "id" in msg and "method" in msg:
                self._answer(msg)
            self.q.put(msg)

    def _answer(self, msg):
        m = msg["method"]
        if m == "workspace/configuration":
            res = [self.settings.get(item.get("section", ""), self.settings.get("*")) for item in msg["params"]["items"]]
        else:
            res = None
        self.send({"jsonrpc": "2.0", "id": msg["id"], "result": res})

    def send(self, msg):
        data = json.dumps(msg).encode()
        self.p.stdin.write(b"Content-Length: %d\r\n\r\n" % len(data) + data)
        self.p.stdin.flush()

    def request(self, method, params):
        self.next_id += 1
        rid = self.next_id
        self.send({"jsonrpc": "2.0", "id": rid, "method": method, **({} if params is None else {"params": params})})
        return rid

    def notify(self, method, params):
        self.send({"jsonrpc": "2.0", "method": method, **({} if params is None else {"params": params})})

    def wait(self, pred, timeout):
        end = time.monotonic() + timeout
        seen = []
        while time.monotonic() < end:
            try:
                msg = self.q.get(timeout=max(0.01, end - time.monotonic()))
            except queue.Empty:
                break
            if msg is None:
                break
            seen.append(msg)
            if pred(msg):
                return msg, seen
        return None, seen

    def elapsed(self):
        return round((time.monotonic() - self.t0) * 1000)
